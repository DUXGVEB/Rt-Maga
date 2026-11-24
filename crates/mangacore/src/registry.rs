//! Source registry coordinating pluggable site adapters, adaptive structured/HTML extraction,
//! browser rendering fallback, panic isolation, and 27-code structured diagnostics.

use std::future::Future;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Instant;

use scraper::Html;
use tracing::{debug, error, warn};
use url::Url;

use crate::detect::{
    AccessGateKind, PageAnalysis, PageInspectInput, SessionPresence, analyze_html_page,
};
use crate::diagnostics::{
    BrowserFallbackAvailability, ExtractionStrategyKind, SourceDiagnosticReport, SourceErrorCode,
    StrategyAttemptTrace, redact_url_for_diagnostics,
};
use crate::error::{ClassifiedFailureDetail, ScrapeError};
use crate::extract::{
    HtmlMinimumPolicy, extract_chapter_html_adaptive, extract_structured_chapter,
    is_unsupported_api_payload,
};
use crate::http::{FetchedPage, HttpClient};
use crate::model::{Chapter, ChapterRef, Manga, Resolved, SiteId};
use crate::parse::chapters::extract_chapter_number;
use crate::parse::classify::{PageKind, classify_page};
use crate::parse::dom::effective_base_url;
use crate::recognize::{
    PreFetchClassification, RecognitionDiagnostic, RecognitionStatus, UrlKind,
    classify_normalized_url, classify_url_pre_fetch, diagnostic_from_error, now_unix_secs,
};
use crate::source::generic::GenericSource;
#[cfg(feature = "headless")]
use crate::source::headless::{HeadlessBrowser, InteractiveSessionMode};
use crate::source::{Source, default_adapters};

/// Controls whether the registry may launch a headless browser when static extraction fails.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum HeadlessPolicy {
    /// Fall back to a headless browser when static parsing indicates JS/dynamic rendering.
    #[default]
    Enabled,
    /// Never launch a headless browser; classify JS-only pages with `JAVASCRIPT_RENDER_REQUIRED`.
    Disabled,
}

/// Central coordinator that resolves homepage, catalog, search, series, and chapter URLs across
/// site-specific adapters, structured APIs/JSON, native HTML, and browser rendering.
#[derive(Debug, Clone)]
pub struct Registry {
    http: HttpClient,
    adapters: Arc<[Box<dyn Source>]>,
    #[cfg(feature = "headless")]
    headless: HeadlessBrowser,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PriorAdapterStatus {
    NoneOrUnused,
    OutdatedFallback,
}

struct CatchUnwindFuture<F>(F);

impl<F> Future for CatchUnwindFuture<F>
where
    F: Future + Unpin,
{
    type Output = Result<F::Output, &'static str>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let res = catch_unwind(AssertUnwindSafe(|| Pin::new(&mut self.0).poll(cx)));
        match res {
            Ok(Poll::Ready(out)) => Poll::Ready(Ok(out)),
            Ok(Poll::Pending) => Poll::Pending,
            Err(_) => Poll::Ready(Err("adapter panicked during execution")),
        }
    }
}

impl Registry {
    /// Creates a [`Registry`] with a default [`HttpClient`] and all built-in site adapters.
    ///
    /// # Errors
    ///
    /// Returns [`ScrapeError::Network`] if the underlying [`HttpClient`] fails to initialize.
    pub fn new() -> Result<Self, ScrapeError> {
        let http = HttpClient::new()?;
        Ok(Self::with_http(http))
    }

    /// Creates a [`Registry`] using an existing [`HttpClient`] and all built-in site adapters.
    pub fn with_http(http: HttpClient) -> Self {
        Self::with_adapters(http, default_adapters())
    }

    /// Creates a [`Registry`] with custom [`Source`] adapters checked before the generic engine.
    pub fn with_adapters(http: HttpClient, adapters: Vec<Box<dyn Source>>) -> Self {
        Self {
            http,
            adapters: adapters.into(),
            #[cfg(feature = "headless")]
            headless: HeadlessBrowser::new(),
        }
    }

    /// Returns a reference to the shared [`HttpClient`].
    pub fn http(&self) -> &HttpClient {
        &self.http
    }

    /// Resolves any user-pasted homepage, catalog, search, series, or chapter link and returns
    /// both the [`Resolved`] entity and a structured [`RecognitionDiagnostic`] (carrying the full
    /// [`SourceDiagnosticReport`]).
    ///
    /// # Errors
    ///
    /// Returns `Box<(ScrapeError, RecognitionDiagnostic)>` detailing the failed stage, 27-code
    /// classification, attempted strategies, and redacted evidence.
    pub async fn resolve_with_diagnostics(
        &self,
        raw_url: &str,
        headless_policy: HeadlessPolicy,
    ) -> Result<(Resolved, RecognitionDiagnostic), Box<(ScrapeError, RecognitionDiagnostic)>> {
        let pre = match classify_url_pre_fetch(raw_url) {
            Ok(p) => p,
            Err(diag) => {
                let err = ScrapeError::InvalidUrl(diag.detail.clone());
                return Err(Box::new((err, *diag)));
            }
        };

        let url = pre.normalized_url.clone();
        if pre.url_kind == UrlKind::ImageUrl {
            return Err(image_url_rejected(raw_url, &pre));
        }

        let mut report =
            SourceDiagnosticReport::start(raw_url.trim(), pre.url_kind, Some(pre.website.clone()));
        report.final_url = Some(redact_url_for_diagnostics(url.as_str()));
        report.detection_confidence = if pre.adapter_id.is_some() { 0.92 } else { 0.65 };

        let mut prior_status = PriorAdapterStatus::NoneOrUnused;
        let mut tried_adapter_id: Option<&'static str> = None;

        if let Some(adapter) = self.find_adapter(&url) {
            tried_adapter_id = Some(adapter.id());
            let meta = adapter.metadata();
            report.adapter_name = Some(meta.name.to_owned());
            report.adapter_version = Some(meta.version.to_owned());
            debug!(adapter = adapter.id(), kind = ?pre.url_kind, %url, "trying Strategy D adapter");

            match run_isolated_adapter_resolve(adapter, &self.http, &url, &mut report).await {
                Ok(resolved) => {
                    report.extraction_confidence = 0.95;
                    report.selected_strategy = Some(ExtractionStrategyKind::SiteAdapter);
                    report.failed_stage = Some("completed".to_owned());
                    let diag =
                        build_success_diagnostic(raw_url, &resolved, &pre, adapter.id(), report);
                    return Ok((resolved, diag));
                }
                Err(AdapterOutcome::Terminal(err)) => {
                    return Err(finalize_resolve_failure(
                        err,
                        raw_url,
                        &url,
                        &pre,
                        "adapter_execution",
                        report,
                    ));
                }
                Err(AdapterOutcome::OutdatedFallback(err)) => {
                    prior_status = PriorAdapterStatus::OutdatedFallback;
                    report
                        .evidence
                        .push(format!("adapter '{}' fell back: {err}", adapter.id()));
                    warn!(adapter = adapter.id(), %err, "adapter outdated/missed layout, continuing fallback pipeline");
                }
            }
        }

        self.resolve_fallback_pipeline(
            raw_url,
            &url,
            &pre,
            headless_policy,
            tried_adapter_id,
            prior_status,
            report,
        )
        .await
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "orchestrates fetched HTML, redirects, detection, structured/HTML, and browser fallback"
    )]
    async fn resolve_fallback_pipeline(
        &self,
        raw_url: &str,
        url: &Url,
        pre: &PreFetchClassification,
        headless_policy: HeadlessPolicy,
        tried_adapter_id: Option<&'static str>,
        prior_status: PriorAdapterStatus,
        mut report: SourceDiagnosticReport,
    ) -> Result<(Resolved, RecognitionDiagnostic), Box<(ScrapeError, RecognitionDiagnostic)>> {
        let fetch_start = Instant::now();
        let page = match self.http.fetch_html(url).await {
            Ok(p) => p,
            Err(err) => {
                report.http_status = status_from_scrape_error(&err);
                report.evidence.push(err.to_string());
                return Err(finalize_resolve_failure(
                    err,
                    raw_url,
                    url,
                    pre,
                    "http_fetch",
                    report,
                ));
            }
        };
        record_fetched_page_meta(&page, &mut report);

        if let Some(res) = self
            .try_redirected_adapter_resolve(raw_url, url, pre, &page, tried_adapter_id, &mut report)
            .await
        {
            return Ok(res);
        }

        if is_unsupported_api_payload(page.content_type.as_deref(), &page.html) {
            let err =
                classified_api_format(&page.final_url, page.content_type.as_deref(), page.status);
            return Err(finalize_resolve_failure(
                err,
                raw_url,
                &page.final_url,
                pre,
                "api_decoding",
                report,
            ));
        }

        let analysis = inspect_fetched_page(url, &page, &self.http);
        apply_analysis_to_report(&analysis, &mut report);

        if let Some(gate) = analysis.access_gate {
            let err =
                access_gate_error(gate, &page.final_url, Some(page.status), &analysis.signals);
            return Err(finalize_resolve_failure(
                err,
                raw_url,
                &page.final_url,
                pre,
                "access_gate_detection",
                report,
            ));
        }

        if let Some((resolved, strategy)) =
            resolve_from_fetched_document(&self.http, &page, &analysis, &mut report, fetch_start)
                .await
        {
            report.selected_strategy = Some(strategy);
            report.extraction_confidence = 0.88;
            let diag = build_success_diagnostic(raw_url, &resolved, pre, "generic", report);
            return Ok((resolved, diag));
        }

        if let Some(early_err) = check_static_terminal_outcome(
            &page,
            &analysis,
            headless_policy,
            prior_status,
            raw_url,
            pre,
            report.clone(),
        ) {
            return Err(early_err);
        }

        self.try_browser_resolve_fallback(
            raw_url,
            url,
            &page.final_url,
            pre,
            &analysis,
            prior_status,
            report,
        )
        .await
    }

    async fn try_redirected_adapter_resolve(
        &self,
        raw_url: &str,
        orig_url: &Url,
        pre: &PreFetchClassification,
        page: &FetchedPage,
        tried_adapter_id: Option<&'static str>,
        report: &mut SourceDiagnosticReport,
    ) -> Option<(Resolved, RecognitionDiagnostic)> {
        if page.final_url.host_str() == orig_url.host_str() {
            return None;
        }
        let redirected_adapter = self.find_adapter(&page.final_url)?;
        if Some(redirected_adapter.id()) == tried_adapter_id {
            return None;
        }
        let meta = redirected_adapter.metadata();
        report.adapter_name = Some(meta.name.to_owned());
        report.adapter_version = Some(meta.version.to_owned());
        let resolved =
            run_isolated_adapter_resolve(redirected_adapter, &self.http, &page.final_url, report)
                .await
                .ok()?;
        report.extraction_confidence = 0.94;
        report.selected_strategy = Some(ExtractionStrategyKind::SiteAdapter);
        let diag = build_success_diagnostic(
            raw_url,
            &resolved,
            pre,
            redirected_adapter.id(),
            report.clone(),
        );
        Some((resolved, diag))
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "coordinates headless fallback diagnostics without hiding browser errors"
    )]
    async fn try_browser_resolve_fallback(
        &self,
        raw_url: &str,
        submitted_url: &Url,
        target_url: &Url,
        pre: &PreFetchClassification,
        prior_analysis: &PageAnalysis,
        prior_status: PriorAdapterStatus,
        mut report: SourceDiagnosticReport,
    ) -> Result<(Resolved, RecognitionDiagnostic), Box<(ScrapeError, RecognitionDiagnostic)>> {
        let t0 = Instant::now();
        let strategy_kind = if self.http.has_session_cookies_for(target_url) {
            ExtractionStrategyKind::AuthenticatedBrowser
        } else {
            ExtractionStrategyKind::BrowserRendering
        };

        let rendered = match self
            .render_or_fail_resolve(
                raw_url,
                target_url,
                pre,
                prior_analysis,
                prior_status,
                strategy_kind,
                t0,
                &mut report,
            )
            .await
        {
            Ok(p) => p,
            Err(err_box) => return Err(err_box),
        };

        record_fetched_page_meta(&rendered, &mut report);
        let analysis = inspect_fetched_page(submitted_url, &rendered, &self.http);
        apply_analysis_to_report(&analysis, &mut report);

        if let Some(gate) = analysis.access_gate {
            let err = access_gate_error(
                gate,
                &rendered.final_url,
                Some(rendered.status),
                &analysis.signals,
            );
            return Err(finalize_resolve_failure(
                err,
                raw_url,
                &rendered.final_url,
                pre,
                "browser_access_gate",
                report,
            ));
        }

        if let Some((resolved, _)) =
            resolve_from_fetched_document(&self.http, &rendered, &analysis, &mut report, t0).await
        {
            report.selected_strategy = Some(strategy_kind);
            report.extraction_confidence = 0.85;
            let diag =
                build_success_diagnostic(raw_url, &resolved, pre, "generic_headless", report);
            return Ok((resolved, diag));
        }

        let code = classify_unresolved_code(&analysis, prior_status);
        let err = ScrapeError::classified(ClassifiedFailureDetail {
            code,
            url: redact_url_for_diagnostics(rendered.final_url.as_str()),
            host: rendered.final_url.host_str().map(str::to_owned),
            status: Some(rendered.status),
            stage: "browser_extraction".to_owned(),
            detail: format!("could not extract comic content after browser render ({code:?})"),
            evidence: analysis.signals,
        });
        Err(finalize_resolve_failure(
            err,
            raw_url,
            &rendered.final_url,
            pre,
            "browser_extraction",
            report,
        ))
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "renders page via headless browser and translates failures cleanly"
    )]
    async fn render_or_fail_resolve(
        &self,
        raw_url: &str,
        target_url: &Url,
        pre: &PreFetchClassification,
        prior_analysis: &PageAnalysis,
        prior_status: PriorAdapterStatus,
        strategy_kind: ExtractionStrategyKind,
        t0: Instant,
        report: &mut SourceDiagnosticReport,
    ) -> Result<FetchedPage, Box<(ScrapeError, RecognitionDiagnostic)>> {
        match self.render_page_via_browser(target_url).await {
            Ok(p) => {
                report.strategies_attempted.push(StrategyAttemptTrace {
                    strategy: strategy_kind,
                    adapter_id: None,
                    outcome: "rendered_dom".to_owned(),
                    candidates_found: 0,
                    validated_count: 0,
                    duration_ms: elapsed_ms(t0),
                });
                Ok(p)
            }
            Err(ScrapeError::Headless(msg)) => {
                report.browser_errors.push(msg.clone());
                report.strategies_attempted.push(StrategyAttemptTrace {
                    strategy: strategy_kind,
                    adapter_id: None,
                    outcome: "browser_unavailable".to_owned(),
                    candidates_found: 0,
                    validated_count: 0,
                    duration_ms: elapsed_ms(t0),
                });
                let code = if prior_analysis.js_shell_detected {
                    SourceErrorCode::BrowserEngineUnavailable
                } else if prior_status == PriorAdapterStatus::OutdatedFallback {
                    SourceErrorCode::AdapterOutdated
                } else {
                    SourceErrorCode::SourceDetectionFailed
                };
                let err = ScrapeError::classified(ClassifiedFailureDetail {
                    code,
                    url: redact_url_for_diagnostics(target_url.as_str()),
                    host: target_url.host_str().map(str::to_owned),
                    status: report.http_status,
                    stage: "browser_rendering".to_owned(),
                    detail: msg,
                    evidence: prior_analysis.signals.clone(),
                });
                Err(finalize_resolve_failure(
                    err,
                    raw_url,
                    target_url,
                    pre,
                    "browser_rendering",
                    report.clone(),
                ))
            }
            Err(err) => {
                report.browser_errors.push(err.to_string());
                Err(finalize_resolve_failure(
                    err,
                    raw_url,
                    target_url,
                    pre,
                    "browser_rendering",
                    report.clone(),
                ))
            }
        }
    }

    /// Resolves any user-pasted series, chapter, or catalog link.
    ///
    /// # Errors
    ///
    /// Returns [`ScrapeError`] on validation, transport, API, access, or parsing failure.
    pub async fn resolve(
        &self,
        raw_url: &str,
        headless_policy: HeadlessPolicy,
    ) -> Result<Resolved, ScrapeError> {
        self.resolve_with_diagnostics(raw_url, headless_policy)
            .await
            .map(|(resolved, _)| resolved)
            .map_err(|boxed| boxed.0)
    }

    /// Fetches ordered images and neighbor navigation for a specific chapter URL while returning
    /// the full [`SourceDiagnosticReport`] on both success and failure.
    ///
    /// # Errors
    ///
    /// Returns `Box<(ScrapeError, SourceDiagnosticReport)>` on failure.
    pub async fn chapter_with_diagnostics(
        &self,
        url: &Url,
        headless_policy: HeadlessPolicy,
    ) -> Result<(Chapter, SourceDiagnosticReport), Box<(ScrapeError, SourceDiagnosticReport)>> {
        let host = url.host_str().unwrap_or("unknown").to_owned();
        let bare_domain = host.trim_start_matches("www.").to_ascii_lowercase();
        let mut report =
            SourceDiagnosticReport::start(url.as_str(), UrlKind::ChapterPage, Some(bare_domain));
        report.final_url = Some(redact_url_for_diagnostics(url.as_str()));
        report.detection_confidence = 0.85;

        let mut prior_status = PriorAdapterStatus::NoneOrUnused;
        let mut tried_adapter_id: Option<&'static str> = None;

        if let Some(adapter) = self.find_adapter(url) {
            tried_adapter_id = Some(adapter.id());
            let meta = adapter.metadata();
            report.adapter_name = Some(meta.name.to_owned());
            report.adapter_version = Some(meta.version.to_owned());
            debug!(adapter = adapter.id(), %url, "fetching chapter via Strategy D adapter");

            match run_isolated_adapter_chapter(adapter, &self.http, url, &mut report).await {
                Ok(ch) => {
                    report.image_candidates_count = ch.images.len();
                    report.validated_images_count = ch.images.len();
                    report.extraction_confidence = 0.96;
                    report.selected_strategy = Some(ExtractionStrategyKind::SiteAdapter);
                    return Ok((ch, report));
                }
                Err(AdapterOutcome::Terminal(err)) => {
                    report.http_status = status_from_scrape_error(&err);
                    report.evidence.push(err.to_string());
                    report.apply_failure(
                        err.error_code(),
                        "adapter_chapter_extraction",
                        BrowserFallbackAvailability::Available,
                    );
                    return Err(Box::new((err, report)));
                }
                Err(AdapterOutcome::OutdatedFallback(err)) => {
                    prior_status = PriorAdapterStatus::OutdatedFallback;
                    report
                        .evidence
                        .push(format!("adapter '{}' fell back: {err}", adapter.id()));
                    warn!(adapter = adapter.id(), %err, "adapter chapter failed, continuing fallback pipeline");
                }
            }
        }

        self.chapter_fallback_pipeline(url, headless_policy, tried_adapter_id, prior_status, report)
            .await
    }

    async fn chapter_fallback_pipeline(
        &self,
        url: &Url,
        headless_policy: HeadlessPolicy,
        tried_adapter_id: Option<&'static str>,
        prior_status: PriorAdapterStatus,
        mut report: SourceDiagnosticReport,
    ) -> Result<(Chapter, SourceDiagnosticReport), Box<(ScrapeError, SourceDiagnosticReport)>> {
        let page = match self.http.fetch_html(url).await {
            Ok(p) => p,
            Err(err) => {
                report.http_status = status_from_scrape_error(&err);
                report.evidence.push(err.to_string());
                report.apply_failure(
                    err.error_code(),
                    "chapter_http_fetch",
                    BrowserFallbackAvailability::Available,
                );
                return Err(Box::new((err, report)));
            }
        };
        record_fetched_page_meta(&page, &mut report);

        if page.final_url.host_str() != url.host_str()
            && let Some(redirected_adapter) = self.find_adapter(&page.final_url)
            && Some(redirected_adapter.id()) != tried_adapter_id
        {
            let meta = redirected_adapter.metadata();
            report.adapter_name = Some(meta.name.to_owned());
            report.adapter_version = Some(meta.version.to_owned());
            if let Ok(ch) = run_isolated_adapter_chapter(
                redirected_adapter,
                &self.http,
                &page.final_url,
                &mut report,
            )
            .await
            {
                report.image_candidates_count = ch.images.len();
                report.validated_images_count = ch.images.len();
                report.extraction_confidence = 0.94;
                report.selected_strategy = Some(ExtractionStrategyKind::SiteAdapter);
                return Ok((ch, report));
            }
        }

        if is_unsupported_api_payload(page.content_type.as_deref(), &page.html) {
            let err =
                classified_api_format(&page.final_url, page.content_type.as_deref(), page.status);
            report.apply_failure(
                err.error_code(),
                "chapter_api_decoding",
                BrowserFallbackAvailability::Available,
            );
            return Err(Box::new((err, report)));
        }

        let analysis = inspect_fetched_page(url, &page, &self.http);
        apply_analysis_to_report(&analysis, &mut report);

        if let Some(gate) = analysis.access_gate {
            let err =
                access_gate_error(gate, &page.final_url, Some(page.status), &analysis.signals);
            report.apply_failure(
                err.error_code(),
                "chapter_access_gate",
                BrowserFallbackAvailability::Available,
            );
            return Err(Box::new((err, report)));
        }

        if let Some((chapter, strategy)) = extract_chapter_from_page(&page, &analysis, &mut report)
        {
            report.selected_strategy = Some(strategy);
            report.extraction_confidence = 0.90;
            return Ok((chapter, report));
        }

        if headless_policy == HeadlessPolicy::Disabled
            || (analysis.page_kind == UrlKind::NotAComicPage && !analysis.js_shell_detected)
        {
            let code = if analysis.js_shell_detected {
                SourceErrorCode::JavascriptRenderRequired
            } else {
                classify_chapter_failure_code(&analysis, prior_status, &report)
            };
            let err = ScrapeError::classified(ClassifiedFailureDetail {
                code,
                url: redact_url_for_diagnostics(page.final_url.as_str()),
                host: page.final_url.host_str().map(str::to_owned),
                status: Some(page.status),
                stage: "chapter_static_extraction".to_owned(),
                detail: format!(
                    "chapter images unavailable ({code:?}) at {}",
                    page.final_url
                ),
                evidence: analysis.signals,
            });
            report.apply_failure(
                code,
                "chapter_static_extraction",
                BrowserFallbackAvailability::Available,
            );
            return Err(Box::new((err, report)));
        }

        self.try_browser_chapter_fallback(url, &page.final_url, &analysis, prior_status, report)
            .await
    }

    async fn try_browser_chapter_fallback(
        &self,
        submitted_url: &Url,
        target_url: &Url,
        prior_analysis: &PageAnalysis,
        prior_status: PriorAdapterStatus,
        mut report: SourceDiagnosticReport,
    ) -> Result<(Chapter, SourceDiagnosticReport), Box<(ScrapeError, SourceDiagnosticReport)>> {
        let t0 = Instant::now();
        let strategy_kind = if self.http.has_session_cookies_for(target_url) {
            ExtractionStrategyKind::AuthenticatedBrowser
        } else {
            ExtractionStrategyKind::BrowserRendering
        };

        let rendered = match self.render_page_via_browser(target_url).await {
            Ok(p) => {
                report.strategies_attempted.push(StrategyAttemptTrace {
                    strategy: strategy_kind,
                    adapter_id: None,
                    outcome: "rendered_dom".to_owned(),
                    candidates_found: 0,
                    validated_count: 0,
                    duration_ms: elapsed_ms(t0),
                });
                p
            }
            Err(ScrapeError::Headless(msg)) => {
                report.browser_errors.push(msg.clone());
                let code = if prior_analysis.js_shell_detected {
                    SourceErrorCode::BrowserEngineUnavailable
                } else {
                    classify_chapter_failure_code(prior_analysis, prior_status, &report)
                };
                let err = ScrapeError::classified(ClassifiedFailureDetail {
                    code,
                    url: redact_url_for_diagnostics(target_url.as_str()),
                    host: target_url.host_str().map(str::to_owned),
                    status: report.http_status,
                    stage: "chapter_browser_rendering".to_owned(),
                    detail: msg,
                    evidence: prior_analysis.signals.clone(),
                });
                report.apply_failure(
                    code,
                    "chapter_browser_rendering",
                    BrowserFallbackAvailability::Available,
                );
                return Err(Box::new((err, report)));
            }
            Err(err) => {
                report.browser_errors.push(err.to_string());
                report.apply_failure(
                    err.error_code(),
                    "chapter_browser_rendering",
                    BrowserFallbackAvailability::Available,
                );
                return Err(Box::new((err, report)));
            }
        };

        record_fetched_page_meta(&rendered, &mut report);
        let analysis = inspect_fetched_page(submitted_url, &rendered, &self.http);
        apply_analysis_to_report(&analysis, &mut report);

        if let Some(gate) = analysis.access_gate {
            let err = access_gate_error(
                gate,
                &rendered.final_url,
                Some(rendered.status),
                &analysis.signals,
            );
            report.apply_failure(
                err.error_code(),
                "chapter_browser_access_gate",
                BrowserFallbackAvailability::Available,
            );
            return Err(Box::new((err, report)));
        }

        if let Some((chapter, _)) = extract_chapter_from_page(&rendered, &analysis, &mut report) {
            report.selected_strategy = Some(strategy_kind);
            report.extraction_confidence = 0.87;
            return Ok((chapter, report));
        }

        let code = classify_chapter_failure_code(&analysis, prior_status, &report);
        let err = ScrapeError::classified(ClassifiedFailureDetail {
            code,
            url: redact_url_for_diagnostics(rendered.final_url.as_str()),
            host: rendered.final_url.host_str().map(str::to_owned),
            status: Some(rendered.status),
            stage: "chapter_browser_extraction".to_owned(),
            detail: format!("chapter content not loaded after browser render ({code:?})"),
            evidence: analysis.signals,
        });
        report.apply_failure(
            code,
            "chapter_browser_extraction",
            BrowserFallbackAvailability::Available,
        );
        Err(Box::new((err, report)))
    }

    /// Fetches ordered images and neighbor navigation for a specific chapter URL.
    ///
    /// # Errors
    ///
    /// Returns a [`ScrapeError`] if all enabled strategies fail to extract chapter images.
    pub async fn chapter(
        &self,
        url: &Url,
        headless_policy: HeadlessPolicy,
    ) -> Result<Chapter, ScrapeError> {
        self.chapter_with_diagnostics(url, headless_policy)
            .await
            .map(|(ch, _)| ch)
            .map_err(|boxed| boxed.0)
    }

    /// Opens a visible browser window so the user can manually complete a bot-verification check,
    /// then stores verified session cookies in the shared [`HttpClient`].
    ///
    /// # Errors
    ///
    /// Returns [`ScrapeError::Headless`] if the browser runtime is unavailable or the check fails.
    pub async fn open_manual_verification(&self, url: &Url) -> Result<(), ScrapeError> {
        #[cfg(feature = "headless")]
        {
            self.headless
                .open_interactive_session(
                    url,
                    InteractiveSessionMode::ManualVerification,
                    &self.http,
                )
                .await
        }
        #[cfg(not(feature = "headless"))]
        {
            let _ = url;
            Err(ScrapeError::Headless(
                "headless browser engine unavailable on this platform".to_owned(),
            ))
        }
    }

    /// Opens an interactive visible browser window so the user can sign in with an ordinary
    /// account on `url` and synchronizes authorized session cookies into [`HttpClient`].
    ///
    /// Never captures or logs passwords or raw cookie tokens.
    ///
    /// # Errors
    ///
    /// Returns [`ScrapeError::Headless`] if the browser runtime is unavailable or login times out.
    pub async fn open_user_login_session(&self, url: &Url) -> Result<(), ScrapeError> {
        #[cfg(feature = "headless")]
        {
            self.headless
                .open_interactive_session(
                    url,
                    InteractiveSessionMode::UserAuthentication,
                    &self.http,
                )
                .await
        }
        #[cfg(not(feature = "headless"))]
        {
            let _ = url;
            Err(ScrapeError::Headless(
                "browser login runtime unavailable on this platform".to_owned(),
            ))
        }
    }

    fn find_adapter(&self, url: &Url) -> Option<&dyn Source> {
        self.adapters
            .iter()
            .find(|adapter| adapter.matches(url))
            .map(AsRef::as_ref)
    }

    async fn render_page_via_browser(&self, url: &Url) -> Result<FetchedPage, ScrapeError> {
        #[cfg(feature = "headless")]
        {
            self.headless.fetch_rendered(url, &self.http).await
        }
        #[cfg(not(feature = "headless"))]
        {
            let _ = url;
            Err(ScrapeError::Headless(
                "headless browser engine unavailable on this platform".to_owned(),
            ))
        }
    }
}

enum AdapterOutcome {
    Terminal(ScrapeError),
    OutdatedFallback(ScrapeError),
}

async fn run_isolated_adapter_resolve(
    adapter: &dyn Source,
    http: &HttpClient,
    url: &Url,
    report: &mut SourceDiagnosticReport,
) -> Result<Resolved, AdapterOutcome> {
    let t0 = Instant::now();
    let fut = adapter.resolve(http, url);
    match CatchUnwindFuture(fut).await {
        Ok(Ok(resolved)) => {
            report.strategies_attempted.push(StrategyAttemptTrace {
                strategy: ExtractionStrategyKind::SiteAdapter,
                adapter_id: Some(adapter.id().to_owned()),
                outcome: "succeeded".to_owned(),
                candidates_found: 0,
                validated_count: 0,
                duration_ms: elapsed_ms(t0),
            });
            Ok(resolved)
        }
        Ok(Err(err)) => {
            let allow_fallback = adapter.id() != "mangadex" && !is_terminal_scrape_error(&err);
            report.strategies_attempted.push(StrategyAttemptTrace {
                strategy: ExtractionStrategyKind::SiteAdapter,
                adapter_id: Some(adapter.id().to_owned()),
                outcome: if allow_fallback {
                    "adapter_outdated_fallback".to_owned()
                } else {
                    format!("{:?}", err.error_code())
                },
                candidates_found: 0,
                validated_count: 0,
                duration_ms: elapsed_ms(t0),
            });
            if allow_fallback {
                Err(AdapterOutcome::OutdatedFallback(err))
            } else {
                Err(AdapterOutcome::Terminal(err))
            }
        }
        Err(panic_msg) => {
            error!(adapter = adapter.id(), "isolated panic inside site adapter");
            let err = ScrapeError::classified(ClassifiedFailureDetail {
                code: SourceErrorCode::AdapterExecutionFailed,
                url: redact_url_for_diagnostics(url.as_str()),
                host: url.host_str().map(str::to_owned),
                status: None,
                stage: "adapter_execution".to_owned(),
                detail: format!("adapter '{}' failed: {panic_msg}", adapter.id()),
                evidence: vec![panic_msg.to_owned()],
            });
            report.strategies_attempted.push(StrategyAttemptTrace {
                strategy: ExtractionStrategyKind::SiteAdapter,
                adapter_id: Some(adapter.id().to_owned()),
                outcome: "panicked_isolated".to_owned(),
                candidates_found: 0,
                validated_count: 0,
                duration_ms: elapsed_ms(t0),
            });
            Err(AdapterOutcome::Terminal(err))
        }
    }
}

async fn run_isolated_adapter_chapter(
    adapter: &dyn Source,
    http: &HttpClient,
    url: &Url,
    report: &mut SourceDiagnosticReport,
) -> Result<Chapter, AdapterOutcome> {
    let t0 = Instant::now();
    let fut = adapter.chapter(http, url);
    match CatchUnwindFuture(fut).await {
        Ok(Ok(ch)) => {
            report.strategies_attempted.push(StrategyAttemptTrace {
                strategy: ExtractionStrategyKind::SiteAdapter,
                adapter_id: Some(adapter.id().to_owned()),
                outcome: "succeeded".to_owned(),
                candidates_found: ch.images.len(),
                validated_count: ch.images.len(),
                duration_ms: elapsed_ms(t0),
            });
            Ok(ch)
        }
        Ok(Err(err)) => {
            let allow_fallback = adapter.id() != "mangadex" && !is_terminal_scrape_error(&err);
            report.strategies_attempted.push(StrategyAttemptTrace {
                strategy: ExtractionStrategyKind::SiteAdapter,
                adapter_id: Some(adapter.id().to_owned()),
                outcome: if allow_fallback {
                    "adapter_outdated_fallback".to_owned()
                } else {
                    format!("{:?}", err.error_code())
                },
                candidates_found: 0,
                validated_count: 0,
                duration_ms: elapsed_ms(t0),
            });
            if allow_fallback {
                Err(AdapterOutcome::OutdatedFallback(err))
            } else {
                Err(AdapterOutcome::Terminal(err))
            }
        }
        Err(panic_msg) => {
            error!(
                adapter = adapter.id(),
                "isolated panic inside site adapter chapter"
            );
            let err = ScrapeError::classified(ClassifiedFailureDetail {
                code: SourceErrorCode::AdapterExecutionFailed,
                url: redact_url_for_diagnostics(url.as_str()),
                host: url.host_str().map(str::to_owned),
                status: None,
                stage: "adapter_chapter_execution".to_owned(),
                detail: format!("adapter '{}' failed: {panic_msg}", adapter.id()),
                evidence: vec![panic_msg.to_owned()],
            });
            report.strategies_attempted.push(StrategyAttemptTrace {
                strategy: ExtractionStrategyKind::SiteAdapter,
                adapter_id: Some(adapter.id().to_owned()),
                outcome: "panicked_isolated".to_owned(),
                candidates_found: 0,
                validated_count: 0,
                duration_ms: elapsed_ms(t0),
            });
            Err(AdapterOutcome::Terminal(err))
        }
    }
}

fn inspect_fetched_page(submitted: &Url, page: &FetchedPage, http: &HttpClient) -> PageAnalysis {
    let doc = Html::parse_document(&page.html);
    let session = if http.has_session_cookies_for(&page.final_url) {
        SessionPresence::WithCookies
    } else {
        SessionPresence::NoCookies
    };
    analyze_html_page(PageInspectInput {
        submitted_url: submitted,
        final_url: &page.final_url,
        status: page.status,
        content_type: page.content_type.as_deref(),
        html: &page.html,
        doc: &doc,
        session,
    })
}

fn record_fetched_page_meta(page: &FetchedPage, report: &mut SourceDiagnosticReport) {
    report.final_url = Some(redact_url_for_diagnostics(page.final_url.as_str()));
    report.http_status = Some(page.status);
    report.content_type.clone_from(&page.content_type);
    report.response_bytes = Some(page.response_bytes);
    if let Some(host) = page.final_url.host_str() {
        report.domain = Some(host.trim_start_matches("www.").to_ascii_lowercase());
    }
}

fn apply_analysis_to_report(analysis: &PageAnalysis, report: &mut SourceDiagnosticReport) {
    report.detected_page_type = analysis.page_kind;
    report.detection_confidence = analysis.confidence;
    for sig in &analysis.signals {
        if !report.evidence.contains(sig) {
            report.evidence.push(sig.clone());
        }
    }
}

fn check_static_terminal_outcome(
    page: &FetchedPage,
    analysis: &PageAnalysis,
    headless_policy: HeadlessPolicy,
    prior_status: PriorAdapterStatus,
    raw_url: &str,
    pre: &PreFetchClassification,
    report: SourceDiagnosticReport,
) -> Option<Box<(ScrapeError, RecognitionDiagnostic)>> {
    if analysis.page_kind == UrlKind::NotAComicPage && !analysis.js_shell_detected {
        let err = ScrapeError::classified(ClassifiedFailureDetail {
            code: SourceErrorCode::NotAComicPage,
            url: redact_url_for_diagnostics(page.final_url.as_str()),
            host: page.final_url.host_str().map(str::to_owned),
            status: Some(page.status),
            stage: "source_detection".to_owned(),
            detail: "fetched page appears to be general web content rather than a comic page"
                .to_owned(),
            evidence: analysis.signals.clone(),
        });
        return Some(finalize_resolve_failure(
            err,
            raw_url,
            &page.final_url,
            pre,
            "source_detection",
            report,
        ));
    }

    if headless_policy == HeadlessPolicy::Disabled {
        let code = if analysis.js_shell_detected {
            SourceErrorCode::JavascriptRenderRequired
        } else if prior_status == PriorAdapterStatus::OutdatedFallback {
            SourceErrorCode::AdapterOutdated
        } else if analysis.page_kind == UrlKind::ChapterPage {
            SourceErrorCode::ChapterContentNotLoaded
        } else {
            SourceErrorCode::SourceDetectionFailed
        };
        let err = ScrapeError::classified(ClassifiedFailureDetail {
            code,
            url: redact_url_for_diagnostics(page.final_url.as_str()),
            host: page.final_url.host_str().map(str::to_owned),
            status: Some(page.status),
            stage: "static_extraction".to_owned(),
            detail: format!("static extraction failed ({code:?}) on {}", page.final_url),
            evidence: analysis.signals.clone(),
        });
        return Some(finalize_resolve_failure(
            err,
            raw_url,
            &page.final_url,
            pre,
            "static_extraction",
            report,
        ));
    }

    None
}

async fn resolve_from_fetched_document(
    http: &HttpClient,
    page: &FetchedPage,
    analysis: &PageAnalysis,
    report: &mut SourceDiagnosticReport,
    timer: Instant,
) -> Option<(Resolved, ExtractionStrategyKind)> {
    let host = page
        .final_url
        .host_str()
        .unwrap_or("unknown")
        .to_ascii_lowercase();
    let (_, _, url_kind) = classify_normalized_url(&page.final_url, &host);

    let effective_catalog_kind = if matches!(
        url_kind,
        UrlKind::WebsiteHome | UrlKind::CatalogPage | UrlKind::SearchPage
    ) {
        Some(url_kind)
    } else if matches!(
        analysis.page_kind,
        UrlKind::WebsiteHome | UrlKind::CatalogPage | UrlKind::SearchPage
    ) {
        Some(analysis.page_kind)
    } else {
        None
    };

    if let Some(cat_kind) = effective_catalog_kind
        && let Ok(catalog) =
            GenericSource::parse_catalog_html(&page.html, &page.final_url, cat_kind)
    {
        record_catalog_trace(report, "catalog_html", catalog.entries.len(), timer);
        return Some((
            Resolved::Catalog(Box::new(catalog)),
            ExtractionStrategyKind::NativeHtml,
        ));
    }

    if analysis.page_kind == UrlKind::MangaDetail
        && let Ok(manga) = GenericSource::parse_manga_html(&page.html, &page.final_url)
    {
        let used_structured = Html::parse_document(&page.html)
            .select(&scraper::Selector::parse("script#__NEXT_DATA__").expect("valid selector"))
            .next()
            .is_some();
        let strategy = if used_structured {
            ExtractionStrategyKind::StructuredDataAndApi
        } else {
            ExtractionStrategyKind::NativeHtml
        };
        record_catalog_trace_with_strategy(
            report,
            strategy,
            "manga_detail",
            manga.chapters.len(),
            timer,
        );
        return Some((Resolved::Manga(Box::new(manga)), strategy));
    }

    if let Some((chapter, strategy)) = extract_chapter_from_page(page, analysis, report) {
        let resolved = resolve_chapter_with_parent(http, chapter, &host, report).await;
        return Some((resolved, strategy));
    }

    if let Ok(manga) = GenericSource::parse_manga_html(&page.html, &page.final_url) {
        let used_structured = Html::parse_document(&page.html)
            .select(&scraper::Selector::parse("script#__NEXT_DATA__").expect("valid selector"))
            .next()
            .is_some()
            && matches!(analysis.page_kind, UrlKind::MangaDetail);
        let strategy = if used_structured {
            ExtractionStrategyKind::StructuredDataAndApi
        } else {
            ExtractionStrategyKind::NativeHtml
        };
        record_catalog_trace_with_strategy(
            report,
            strategy,
            "manga_detail",
            manga.chapters.len(),
            timer,
        );
        return Some((Resolved::Manga(Box::new(manga)), strategy));
    }

    let kind = classify_page(&Html::parse_document(&page.html), &page.final_url);
    if kind == PageKind::Unknown
        && let Ok(catalog) =
            GenericSource::parse_catalog_html(&page.html, &page.final_url, UrlKind::CatalogPage)
    {
        record_catalog_trace(report, "catalog_fallback", catalog.entries.len(), timer);
        return Some((
            Resolved::Catalog(Box::new(catalog)),
            ExtractionStrategyKind::NativeHtml,
        ));
    }

    None
}

async fn resolve_chapter_with_parent(
    http: &HttpClient,
    chapter: Chapter,
    host: &str,
    report: &mut SourceDiagnosticReport,
) -> Resolved {
    let ch_ref = ChapterRef::simple(
        chapter.url.clone(),
        chapter.name.clone(),
        extract_chapter_number(&chapter.name)
            .or_else(|| extract_chapter_number(chapter.url.path())),
    );
    if let Some(manga_url) = chapter.manga_url.as_ref() {
        match http.fetch_html(manga_url).await {
            Ok(parent_page) => {
                if let Ok(manga) =
                    GenericSource::parse_manga_html(&parent_page.html, &parent_page.final_url)
                {
                    let matched = manga
                        .chapters
                        .iter()
                        .find(|c| c.url == chapter.url)
                        .cloned()
                        .unwrap_or(ch_ref);
                    return Resolved::Chapter {
                        manga: Box::new(manga),
                        at: Box::new(matched),
                    };
                }
                report
                    .evidence
                    .push("parent_series_page_lacked_chapter_list".to_owned());
            }
            Err(err) => {
                report
                    .evidence
                    .push(format!("parent_series_fetch_skipped: {err}"));
            }
        }
    }
    let fallback_manga = Manga::minimal(
        chapter.manga_url.unwrap_or_else(|| chapter.url.clone()),
        SiteId::generic(host),
        chapter.name.clone(),
        None,
        vec![ch_ref.clone()],
    );
    Resolved::Chapter {
        manga: Box::new(fallback_manga),
        at: Box::new(ch_ref),
    }
}

fn record_catalog_trace(
    report: &mut SourceDiagnosticReport,
    tag: &str,
    count: usize,
    timer: Instant,
) {
    record_catalog_trace_with_strategy(
        report,
        ExtractionStrategyKind::NativeHtml,
        tag,
        count,
        timer,
    );
}

fn record_catalog_trace_with_strategy(
    report: &mut SourceDiagnosticReport,
    strategy: ExtractionStrategyKind,
    tag: &str,
    count: usize,
    timer: Instant,
) {
    report.strategies_attempted.push(StrategyAttemptTrace {
        strategy,
        adapter_id: Some(tag.to_owned()),
        outcome: "succeeded".to_owned(),
        candidates_found: count,
        validated_count: count,
        duration_ms: elapsed_ms(timer),
    });
}

fn extract_chapter_from_page(
    page: &FetchedPage,
    analysis: &PageAnalysis,
    report: &mut SourceDiagnosticReport,
) -> Option<(Chapter, ExtractionStrategyKind)> {
    let doc = Html::parse_document(&page.html);
    let base = effective_base_url(&doc, &page.final_url);

    let t_c = Instant::now();
    if let Some(structured) = extract_structured_chapter(&doc, &page.html, &page.final_url, &base) {
        let validated = structured.chapter.images.len();
        report.image_candidates_count =
            report.image_candidates_count.max(structured.raw_candidates);
        report.validated_images_count = validated;
        report.strategies_attempted.push(StrategyAttemptTrace {
            strategy: ExtractionStrategyKind::StructuredDataAndApi,
            adapter_id: Some(structured.source_tag.to_owned()),
            outcome: "succeeded".to_owned(),
            candidates_found: structured.raw_candidates,
            validated_count: validated,
            duration_ms: elapsed_ms(t_c),
        });
        return Some((
            structured.chapter,
            ExtractionStrategyKind::StructuredDataAndApi,
        ));
    }
    report.strategies_attempted.push(StrategyAttemptTrace {
        strategy: ExtractionStrategyKind::StructuredDataAndApi,
        adapter_id: None,
        outcome: "no_structured_chapter".to_owned(),
        candidates_found: 0,
        validated_count: 0,
        duration_ms: elapsed_ms(t_c),
    });

    let t_a = Instant::now();
    let policy = if analysis.page_kind == UrlKind::ChapterPage && analysis.confidence >= 0.70 {
        HtmlMinimumPolicy::ConfidentChapterOneOrMore
    } else {
        HtmlMinimumPolicy::StandardThree
    };
    let html_out = extract_chapter_html_adaptive(&doc, &page.final_url, &base, policy);
    report.image_candidates_count = report.image_candidates_count.max(html_out.raw_candidates);
    if html_out.blob_urls_seen > 0 {
        report
            .evidence
            .push(format!("blob_urls_seen={}", html_out.blob_urls_seen));
    }
    if let Some(ch) = html_out.chapter {
        let validated = ch.images.len();
        report.validated_images_count = validated;
        report.strategies_attempted.push(StrategyAttemptTrace {
            strategy: ExtractionStrategyKind::NativeHtml,
            adapter_id: None,
            outcome: "succeeded".to_owned(),
            candidates_found: html_out.raw_candidates,
            validated_count: validated,
            duration_ms: elapsed_ms(t_a),
        });
        return Some((ch, ExtractionStrategyKind::NativeHtml));
    }
    report.strategies_attempted.push(StrategyAttemptTrace {
        strategy: ExtractionStrategyKind::NativeHtml,
        adapter_id: None,
        outcome: format!("{}_validated_too_few", html_out.pages.len()),
        candidates_found: html_out.raw_candidates,
        validated_count: html_out.pages.len(),
        duration_ms: elapsed_ms(t_a),
    });

    None
}

fn classified_api_format(url: &Url, content_type: Option<&str>, status: u16) -> ScrapeError {
    ScrapeError::classified(ClassifiedFailureDetail {
        code: SourceErrorCode::UnsupportedApiFormat,
        url: redact_url_for_diagnostics(url.as_str()),
        host: url.host_str().map(str::to_owned),
        status: Some(status),
        stage: "api_decoding".to_owned(),
        detail: format!(
            "unsupported response format '{}' at {}",
            content_type.unwrap_or("binary/unknown"),
            url
        ),
        evidence: vec![format!(
            "content_type={}",
            content_type.unwrap_or("unknown")
        )],
    })
}

fn access_gate_error(
    gate: AccessGateKind,
    url: &Url,
    status: Option<u16>,
    signals: &[String],
) -> ScrapeError {
    let host = url.host_str().unwrap_or("unknown").to_owned();
    if gate == AccessGateKind::Verification {
        return ScrapeError::Blocked {
            host,
            url: url.clone(),
        };
    }
    let code = match gate {
        AccessGateKind::Verification => SourceErrorCode::VerificationRequired,
        AccessGateKind::LoginRequired => SourceErrorCode::LoginRequired,
        AccessGateKind::SessionExpired => SourceErrorCode::SessionExpired,
        AccessGateKind::Paywall => SourceErrorCode::PaywallOrSubscriptionRequired,
        AccessGateKind::GeoRestriction => SourceErrorCode::GeoRestriction,
        AccessGateKind::DrmProtected => SourceErrorCode::DrmProtectedContent,
        AccessGateKind::ExtractionNotPermitted => SourceErrorCode::ExtractionNotPermitted,
    };
    ScrapeError::classified(ClassifiedFailureDetail {
        code,
        url: redact_url_for_diagnostics(url.as_str()),
        host: Some(host),
        status,
        stage: "access_gate_detection".to_owned(),
        detail: format!("access gate detected ({code:?}) at {url}"),
        evidence: signals.to_vec(),
    })
}

fn classify_unresolved_code(
    analysis: &PageAnalysis,
    prior_status: PriorAdapterStatus,
) -> SourceErrorCode {
    if prior_status == PriorAdapterStatus::OutdatedFallback {
        return SourceErrorCode::AdapterOutdated;
    }
    match analysis.page_kind {
        UrlKind::NotAComicPage => SourceErrorCode::NotAComicPage,
        UrlKind::ChapterPage => SourceErrorCode::ChapterContentNotLoaded,
        _ if !analysis.signals.is_empty() => SourceErrorCode::SourceDetectionFailed,
        _ => SourceErrorCode::UnknownSourceError,
    }
}

fn classify_chapter_failure_code(
    analysis: &PageAnalysis,
    prior_status: PriorAdapterStatus,
    report: &SourceDiagnosticReport,
) -> SourceErrorCode {
    if !report.browser_errors.is_empty() && analysis.js_shell_detected {
        return SourceErrorCode::BrowserEngineUnavailable;
    }
    if prior_status == PriorAdapterStatus::OutdatedFallback {
        return SourceErrorCode::AdapterOutdated;
    }
    if analysis.page_kind == UrlKind::NotAComicPage {
        return SourceErrorCode::NotAComicPage;
    }
    SourceErrorCode::ChapterContentNotLoaded
}

fn finalize_resolve_failure(
    err: ScrapeError,
    raw_url: &str,
    url: &Url,
    pre: &PreFetchClassification,
    stage: &str,
    mut report: SourceDiagnosticReport,
) -> Box<(ScrapeError, RecognitionDiagnostic)> {
    let mut diag = diagnostic_from_error(
        &err,
        raw_url.trim(),
        Some(url),
        pre.url_kind,
        Some(&pre.website),
        report.adapter_name.as_deref().or(Some("generic")),
        stage,
    );
    report.apply_failure(
        err.error_code(),
        stage,
        BrowserFallbackAvailability::Available,
    );
    if !report.evidence.iter().any(|e| e == &diag.detail) {
        report.evidence.push(diag.detail.clone());
    }
    diag.confidence = report.detection_confidence;
    diag.report = Some(report);
    Box::new((err, diag))
}

fn image_url_rejected(
    raw_url: &str,
    pre: &PreFetchClassification,
) -> Box<(ScrapeError, RecognitionDiagnostic)> {
    let err = ScrapeError::UnsupportedSource {
        host: pre.website.clone(),
        url: pre.normalized_url.clone(),
    };
    let mut report =
        SourceDiagnosticReport::start(raw_url.trim(), UrlKind::ImageUrl, Some(pre.website.clone()));
    report.final_url = Some(redact_url_for_diagnostics(pre.normalized_url.as_str()));
    report.detection_confidence = 0.95;
    report
        .evidence
        .push("url_points_directly_to_image_file".to_owned());
    report.apply_failure(
        SourceErrorCode::UnsupportedWebsite,
        "page_classification",
        BrowserFallbackAvailability::Available,
    );
    let diag = RecognitionDiagnostic {
        status: RecognitionStatus::WebsiteSupportedButPageUnrecognized,
        url_kind: UrlKind::ImageUrl,
        website: Some(pre.website.clone()),
        adapter: pre.adapter_id.map(str::to_owned),
        submitted_url: redact_url_for_diagnostics(raw_url.trim()),
        resolved_url: Some(redact_url_for_diagnostics(pre.normalized_url.as_str())),
        stage: "page_classification".to_owned(),
        http_status: None,
        detail: "Direct image URLs cannot be opened as a manga or chapter page.".to_owned(),
        suggested_action: Some(
            "Paste the manga detail, chapter, catalog, or homepage URL instead.".to_owned(),
        ),
        confidence: 0.95,
        report: Some(report),
        timestamp_unix: now_unix_secs(),
    };
    Box::new((err, diag))
}

fn build_success_diagnostic(
    raw_url: &str,
    resolved: &Resolved,
    pre: &PreFetchClassification,
    adapter: &str,
    mut report: SourceDiagnosticReport,
) -> RecognitionDiagnostic {
    let (status, url_kind, resolved_url, detail) = match resolved {
        Resolved::Manga(m) => (
            RecognitionStatus::MangaIdentified,
            UrlKind::MangaDetail,
            m.url.as_str().to_owned(),
            format!(
                "Identified manga '{}' with {} chapters",
                m.title,
                m.chapters.len()
            ),
        ),
        Resolved::Chapter { manga, at } => (
            RecognitionStatus::ChapterIdentified,
            UrlKind::ChapterPage,
            at.url.as_str().to_owned(),
            format!("Identified chapter '{}' of '{}'", at.name, manga.title),
        ),
        Resolved::Catalog(cat) => {
            let st = if cat.url_kind == UrlKind::SearchPage {
                RecognitionStatus::SearchResultsExtracted
            } else {
                RecognitionStatus::CatalogIdentified
            };
            (
                st,
                cat.url_kind,
                cat.url.as_str().to_owned(),
                format!(
                    "Extracted {} manga entries from {}",
                    cat.entries.len(),
                    cat.site_name
                ),
            )
        }
    };

    let redacted_resolved = redact_url_for_diagnostics(&resolved_url);
    report.final_url = Some(redacted_resolved.clone());
    report.detected_page_type = url_kind;
    report.failed_stage = Some("completed".to_owned());

    RecognitionDiagnostic {
        status,
        url_kind: if pre.url_kind == UrlKind::RedirectedUrl {
            UrlKind::RedirectedUrl
        } else {
            url_kind
        },
        website: Some(pre.website.clone()),
        adapter: Some(adapter.to_owned()),
        submitted_url: redact_url_for_diagnostics(raw_url.trim()),
        resolved_url: Some(redacted_resolved),
        stage: "completed".to_owned(),
        http_status: report.http_status.or(Some(200)),
        detail,
        suggested_action: None,
        confidence: report
            .detection_confidence
            .max(report.extraction_confidence),
        report: Some(report),
        timestamp_unix: now_unix_secs(),
    }
}

const fn status_from_scrape_error(err: &ScrapeError) -> Option<u16> {
    match err {
        ScrapeError::HttpStatus { status, .. } | ScrapeError::AccessRestricted { status, .. } => {
            Some(*status)
        }
        ScrapeError::RateLimited { .. } => Some(429),
        ScrapeError::Blocked { .. } => Some(403),
        ScrapeError::ApiFailure { status, .. } => *status,
        ScrapeError::Classified(d) => d.status,
        _ => None,
    }
}

fn elapsed_ms(t0: Instant) -> u64 {
    u64::try_from(t0.elapsed().as_millis()).unwrap_or(u64::MAX)
}

/// Errors that represent transport, HTTP status, policy, security, authentication, or proxy
/// failures and must never silently retry through a headless browser.
const fn is_terminal_scrape_error(err: &ScrapeError) -> bool {
    !matches!(
        err,
        ScrapeError::NotRecognized { .. } | ScrapeError::MissingImages { .. }
    )
}

const _: () = {
    const fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<Registry>();
};
