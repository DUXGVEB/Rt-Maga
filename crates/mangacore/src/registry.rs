//! Source registry coordinating pluggable site adapters, generic heuristics, headless fallback,
//! and structured URL recognition diagnostics.

use std::sync::Arc;

use tracing::{debug, warn};
use url::Url;

use crate::error::ScrapeError;
use crate::http::HttpClient;
use crate::model::{Chapter, Resolved};
use crate::recognize::{
    RecognitionDiagnostic, RecognitionStatus, UrlKind, classify_url_pre_fetch,
    diagnostic_from_error, now_unix_secs,
};
use crate::source::generic::GenericSource;
#[cfg(feature = "headless")]
use crate::source::headless::HeadlessBrowser;
use crate::source::{Source, default_adapters};

/// Controls whether the registry may launch a headless browser when static HTML parsing fails.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum HeadlessPolicy {
    /// Fall back to a headless browser when static parsing fails to recognize content.
    #[default]
    Enabled,
    /// Never launch a headless browser; fail fast on JS-only pages.
    Disabled,
}

/// Central coordinator that resolves arbitrary homepage, catalog, series, or chapter URLs.
#[derive(Debug, Clone)]
pub struct Registry {
    http: HttpClient,
    adapters: Arc<[Box<dyn Source>]>,
    generic: GenericSource,
    #[cfg(feature = "headless")]
    headless: HeadlessBrowser,
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

    /// Creates a [`Registry`] with custom [`Source`] adapters checked before the generic fallback.
    pub fn with_adapters(http: HttpClient, adapters: Vec<Box<dyn Source>>) -> Self {
        Self {
            http,
            adapters: adapters.into(),
            generic: GenericSource,
            #[cfg(feature = "headless")]
            headless: HeadlessBrowser::new(),
        }
    }

    /// Returns a reference to the shared [`HttpClient`].
    pub fn http(&self) -> &HttpClient {
        &self.http
    }

    /// Resolves any user-pasted homepage, catalog, search, series, or chapter link and returns
    /// both the [`Resolved`] entity and a structured [`RecognitionDiagnostic`].
    ///
    /// # Errors
    ///
    /// Returns `Box<(ScrapeError, RecognitionDiagnostic)>` detailing the exact stage and failure reason.
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

        let url = pre.normalized_url;
        if pre.url_kind == UrlKind::ImageUrl {
            return Err(image_url_rejected(
                raw_url,
                &url,
                &pre.website,
                pre.adapter_id,
            ));
        }

        // Tier 1: dedicated site adapter matched by domain.
        if let Some(adapter) = self.find_adapter(&url) {
            debug!(adapter = adapter.id(), kind = ?pre.url_kind, %url, "trying dedicated adapter");
            match adapter.resolve(&self.http, &url).await {
                Ok(resolved) => {
                    let diag = build_success_diagnostic(
                        raw_url,
                        &resolved,
                        pre.url_kind,
                        &pre.website,
                        adapter.id(),
                    );
                    return Ok((resolved, diag));
                }
                Err(err) => {
                    let allow_generic =
                        adapter.id() != "mangadex" && !is_terminal_scrape_error(&err);
                    if !allow_generic {
                        let diag = diagnostic_from_error(
                            &err,
                            raw_url.trim(),
                            Some(&url),
                            pre.url_kind,
                            Some(&pre.website),
                            Some(adapter.id()),
                            "adapter_extraction",
                        );
                        return Err(Box::new((err, diag)));
                    }
                    warn!(adapter = adapter.id(), %err, "dedicated adapter failed to parse, falling back to generic");
                }
            }
        }

        self.resolve_generic_and_headless(
            raw_url,
            &url,
            pre.url_kind,
            &pre.website,
            headless_policy,
        )
        .await
    }

    async fn resolve_generic_and_headless(
        &self,
        raw_url: &str,
        url: &Url,
        url_kind: UrlKind,
        website: &str,
        headless_policy: HeadlessPolicy,
    ) -> Result<(Resolved, RecognitionDiagnostic), Box<(ScrapeError, RecognitionDiagnostic)>> {
        let static_err = match self.generic.resolve(&self.http, url).await {
            Ok(resolved) => {
                let diag =
                    build_success_diagnostic(raw_url, &resolved, url_kind, website, "generic");
                return Ok((resolved, diag));
            }
            Err(err)
                if is_terminal_scrape_error(&err)
                    || headless_policy == HeadlessPolicy::Disabled =>
            {
                let diag = diagnostic_from_error(
                    &err,
                    raw_url.trim(),
                    Some(url),
                    url_kind,
                    Some(website),
                    Some("generic"),
                    "generic_extraction",
                );
                return Err(Box::new((err, diag)));
            }
            Err(err) => {
                debug!(%err, %url, "generic static resolve failed, trying headless");
                err
            }
        };

        match self.resolve_headless(url).await {
            Ok(resolved) => {
                let diag = build_success_diagnostic(
                    raw_url,
                    &resolved,
                    url_kind,
                    website,
                    "generic_headless",
                );
                Ok((resolved, diag))
            }
            Err(ScrapeError::Headless(_)) => {
                let diag = diagnostic_from_error(
                    &static_err,
                    raw_url.trim(),
                    Some(url),
                    url_kind,
                    Some(website),
                    Some("generic"),
                    "headless_fallback",
                );
                Err(Box::new((static_err, diag)))
            }
            Err(other) => {
                let diag = diagnostic_from_error(
                    &other,
                    raw_url.trim(),
                    Some(url),
                    url_kind,
                    Some(website),
                    Some("generic_headless"),
                    "headless_fallback",
                );
                Err(Box::new((other, diag)))
            }
        }
    }

    /// Resolves any user-pasted series, chapter, or catalog link through the 3-tier pipeline.
    ///
    /// # Errors
    ///
    /// Returns [`ScrapeError`] on validation, transport, API, or parsing failure.
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

    /// Fetches ordered images and neighbor navigation for a specific chapter URL.
    ///
    /// # Errors
    ///
    /// Returns a [`ScrapeError`] if all enabled tiers fail to extract chapter images.
    pub async fn chapter(
        &self,
        url: &Url,
        headless_policy: HeadlessPolicy,
    ) -> Result<Chapter, ScrapeError> {
        if let Some(adapter) = self.find_adapter(url) {
            debug!(adapter = adapter.id(), %url, "fetching chapter via dedicated adapter");
            match adapter.chapter(&self.http, url).await {
                Ok(chapter) => return Ok(chapter),
                Err(err) if adapter.id() == "mangadex" || is_terminal_scrape_error(&err) => {
                    return Err(err);
                }
                Err(err) => {
                    warn!(adapter = adapter.id(), %err, "adapter chapter failed, trying generic");
                }
            }
        }

        let static_err = match self.generic.chapter(&self.http, url).await {
            Ok(chapter) => return Ok(chapter),
            Err(err) if is_terminal_scrape_error(&err) => return Err(err),
            Err(err) => {
                if headless_policy == HeadlessPolicy::Disabled {
                    return Err(err);
                }
                debug!(%err, %url, "generic chapter failed, trying headless");
                err
            }
        };

        match self.chapter_headless(url).await {
            Ok(chapter) => Ok(chapter),
            Err(ScrapeError::Headless(_)) => Err(static_err),
            Err(other) => Err(other),
        }
    }

    /// Opens a visible browser window so the user can manually complete a bot-verification check,
    /// then stores the verified session cookies in the shared [`HttpClient`].
    ///
    /// # Errors
    ///
    /// Returns [`ScrapeError::Headless`] if the `headless` feature is disabled or the browser fails.
    pub async fn open_manual_verification(&self, url: &Url) -> Result<(), ScrapeError> {
        #[cfg(feature = "headless")]
        {
            self.headless
                .open_manual_verification(url, &self.http)
                .await
        }
        #[cfg(not(feature = "headless"))]
        {
            let _ = url;
            Err(ScrapeError::Headless(
                "headless feature is disabled".to_owned(),
            ))
        }
    }

    fn find_adapter(&self, url: &Url) -> Option<&dyn Source> {
        self.adapters
            .iter()
            .find(|adapter| adapter.matches(url))
            .map(AsRef::as_ref)
    }

    async fn resolve_headless(&self, url: &Url) -> Result<Resolved, ScrapeError> {
        #[cfg(feature = "headless")]
        {
            let rendered = self.headless.fetch_rendered(url, &self.http).await?;
            GenericSource::resolve_html(&self.http, &rendered.html, &rendered.final_url).await
        }
        #[cfg(not(feature = "headless"))]
        {
            let _ = url;
            Err(ScrapeError::Headless(
                "headless fallback unavailable".to_owned(),
            ))
        }
    }

    async fn chapter_headless(&self, url: &Url) -> Result<Chapter, ScrapeError> {
        #[cfg(feature = "headless")]
        {
            let rendered = self.headless.fetch_rendered(url, &self.http).await?;
            GenericSource::parse_chapter_html(&rendered.html, &rendered.final_url)
        }
        #[cfg(not(feature = "headless"))]
        {
            let _ = url;
            Err(ScrapeError::Headless(
                "headless fallback unavailable".to_owned(),
            ))
        }
    }
}

fn image_url_rejected(
    raw_url: &str,
    url: &Url,
    website: &str,
    adapter_id: Option<&'static str>,
) -> Box<(ScrapeError, RecognitionDiagnostic)> {
    let err = ScrapeError::UnsupportedSource {
        host: website.to_owned(),
        url: url.clone(),
    };
    let diag = RecognitionDiagnostic {
        status: RecognitionStatus::WebsiteSupportedButPageUnrecognized,
        url_kind: UrlKind::ImageUrl,
        website: Some(website.to_owned()),
        adapter: adapter_id.map(str::to_owned),
        submitted_url: raw_url.trim().to_owned(),
        resolved_url: Some(url.as_str().to_owned()),
        stage: "page_classification".to_owned(),
        http_status: None,
        detail: "Direct image URLs cannot be opened as a manga or chapter page.".to_owned(),
        suggested_action: Some(
            "Paste the manga detail, chapter, catalog, or homepage URL instead.".to_owned(),
        ),
        timestamp_unix: now_unix_secs(),
    };
    Box::new((err, diag))
}

fn build_success_diagnostic(
    raw_url: &str,
    resolved: &Resolved,
    pre_kind: UrlKind,
    website: &str,
    adapter: &str,
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

    RecognitionDiagnostic {
        status,
        url_kind: if pre_kind == UrlKind::RedirectedUrl {
            UrlKind::RedirectedUrl
        } else {
            url_kind
        },
        website: Some(website.to_owned()),
        adapter: Some(adapter.to_owned()),
        submitted_url: raw_url.trim().to_owned(),
        resolved_url: Some(resolved_url),
        stage: "completed".to_owned(),
        http_status: Some(200),
        detail,
        suggested_action: None,
        timestamp_unix: now_unix_secs(),
    }
}

/// Errors that represent transport, HTTP status, policy, security, or proxy failures and must
/// never fall through to launching a headless browser.
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
