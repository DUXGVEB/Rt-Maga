//! Structured URL classification, website identification, and failure diagnosis.

use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use url::Url;

use crate::diagnostics::{
    BrowserFallbackAvailability, ExtractionStrategyKind, SourceDiagnosticReport, SourceErrorCode,
    describe_error_code, redact_url_for_diagnostics,
};
use crate::error::ScrapeError;
use crate::http::normalize_url;

/// Semantic category assigned to a user-submitted or redirected URL.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[non_exhaustive]
pub enum UrlKind {
    /// Website homepage or root landing page.
    WebsiteHome,
    /// Manga directory, category, genre, latest updates, or popular listing.
    CatalogPage,
    /// Website search results page.
    SearchPage,
    /// Individual manga information and chapter listing page.
    MangaDetail,
    /// Individual chapter or episode reading page.
    ChapterPage,
    /// Direct manga page image or media resource URL.
    ImageUrl,
    /// Recognized documented website API endpoint.
    ApiEndpoint,
    /// URL that redirected to another page type or supported host.
    RedirectedUrl,
    /// User authentication / login page.
    LoginPage,
    /// Anti-bot challenge / verification page.
    VerificationPage,
    /// Paid subscription / VIP / locked chapter purchase page.
    PaywallPage,
    /// Valid webpage that contains non-comic content (blog, store, docs, etc.).
    NotAComicPage,
    /// URL belonging to an unsupported website or page type.
    #[default]
    UnsupportedUrl,
    /// Malformed or disallowed URL.
    InvalidUrl,
    /// Website explicitly denied automated access or requires verification.
    BlockedOrRestricted,
}

/// Structured processing state produced after analyzing or resolving a URL.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[non_exhaustive]
pub enum RecognitionStatus {
    /// Website domain/adapter was recognized.
    WebsiteIdentified,
    /// Specific page category was identified.
    PageTypeIdentified,
    /// Individual manga detail page was extracted successfully.
    MangaIdentified,
    /// Website homepage or catalog listing was extracted successfully.
    CatalogIdentified,
    /// Website search results were extracted successfully.
    SearchResultsExtracted,
    /// Individual chapter page and parent manga were identified.
    ChapterIdentified,
    /// Chapter image resources were extracted successfully.
    ImageResourcesExtracted,
    /// Website is supported, but this specific page layout or path was not recognized.
    WebsiteSupportedButPageUnrecognized,
    /// Target website is not supported for the requested operation.
    UnsupportedWebsite,
    /// Submitted URL is syntactically invalid or targets a disallowed address.
    InvalidUrl,
    /// General transport or connectivity failure.
    NetworkFailure,
    /// DNS resolution failed.
    DnsFailure,
    /// TLS handshake, certificate, or SNI connection failure.
    TlsFailure,
    /// Documented website API returned an error or malformed payload.
    ApiFailure,
    /// Remote server rate-limited the request (HTTP 429).
    RateLimited,
    /// Remote server returned an unexpected HTTP status code.
    HttpError,
    /// HTML or JSON payload could not be parsed as expected.
    ParsingFailure,
    /// Expected manga or catalog metadata was absent.
    MissingMetadata,
    /// Chapter page had no extractable or valid image resources.
    ImageExtractionFailure,
    /// Website explicitly restricted access (HTTP 401/403).
    AccessRestricted,
    /// Website presented an interactive verification/anti-bot check.
    VerificationRequired,
    /// Chapter uses browser-only content display protection (e.g., IMGX) and must be opened in a browser.
    ContentProtected,
}

/// Detailed, structured diagnostic output for every URL recognition attempt.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecognitionDiagnostic {
    /// Final recognition or failure state.
    pub status: RecognitionStatus,
    /// Classified semantic type of the URL.
    pub url_kind: UrlKind,
    /// Human-readable website or host identifier.
    pub website: Option<String>,
    /// Selected website adapter ID (`"mangadex"`, `"truyenqq"`, `"moetruyen"`, `"generic"`).
    pub adapter: Option<String>,
    /// Raw URL submitted by the user (sensitive query params redacted).
    pub submitted_url: String,
    /// Final normalized or redirected URL, if available (sensitive query params redacted).
    pub resolved_url: Option<String>,
    /// Pipeline stage where recognition completed or failed.
    pub stage: String,
    /// HTTP status code received from upstream, if applicable.
    pub http_status: Option<u16>,
    /// Human-readable diagnostic summary without sensitive credentials.
    pub detail: String,
    /// Optional actionable next step for the user.
    pub suggested_action: Option<String>,
    /// Confidence score (`0.0..=1.0`) for URL/page identification.
    #[serde(default)]
    pub confidence: f32,
    /// Full multi-strategy source diagnostic report when available.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub report: Option<SourceDiagnosticReport>,
    /// Unix timestamp in seconds when this diagnostic was created.
    pub timestamp_unix: i64,
}

/// Preliminary classification produced from URL structure before issuing any network requests.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreFetchClassification {
    /// Normalized, SSRF-validated URL.
    pub normalized_url: Url,
    /// Initial classified [`UrlKind`].
    pub url_kind: UrlKind,
    /// Known dedicated adapter ID if matched by domain, or `None` for generic websites.
    pub adapter_id: Option<&'static str>,
    /// Human-readable website label.
    pub website: String,
}

/// Returns the current Unix timestamp in seconds.
#[must_use]
pub fn now_unix_secs() -> i64 {
    i64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
    )
    .unwrap_or(0)
}

/// Performs structured URL validation, normalization, website identification, and pre-fetch page
/// classification before any HTTP request is made.
///
/// # Errors
///
/// Returns a structured [`RecognitionDiagnostic`] with [`RecognitionStatus::InvalidUrl`] when
/// `raw` is empty, malformed, uses a non-HTTP(S) scheme, or targets a private/local address.
pub fn classify_url_pre_fetch(
    raw: &str,
) -> Result<PreFetchClassification, Box<RecognitionDiagnostic>> {
    let submitted = raw.trim().to_owned();
    let redacted = redact_url_for_diagnostics(&submitted);
    let normalized = match normalize_url(&submitted) {
        Ok(u) => u,
        Err(err) => {
            let mut report = SourceDiagnosticReport::start(&redacted, UrlKind::InvalidUrl, None);
            report.evidence.push(err.to_string());
            report.apply_failure(
                SourceErrorCode::InvalidUrl,
                "url_validation",
                BrowserFallbackAvailability::Unavailable,
            );
            return Err(Box::new(RecognitionDiagnostic {
                status: RecognitionStatus::InvalidUrl,
                url_kind: UrlKind::InvalidUrl,
                website: None,
                adapter: None,
                submitted_url: redacted,
                resolved_url: None,
                stage: "url_validation".to_owned(),
                http_status: None,
                detail: err.to_string(),
                suggested_action: Some(
                    "Enter a complete public http:// or https:// manga website URL.".to_owned(),
                ),
                confidence: 0.0,
                report: Some(report),
                timestamp_unix: now_unix_secs(),
            }));
        }
    };

    let host = normalized
        .host_str()
        .unwrap_or("unknown")
        .to_ascii_lowercase();
    let (adapter_id, website, url_kind) = classify_normalized_url(&normalized, &host);

    Ok(PreFetchClassification {
        normalized_url: normalized,
        url_kind,
        adapter_id,
        website,
    })
}

/// Classifies an already-normalized URL into `(adapter_id, website_label, UrlKind)`.
#[must_use]
pub fn classify_normalized_url(
    url: &Url,
    host_lower: &str,
) -> (Option<&'static str>, String, UrlKind) {
    if is_direct_image_url(url) {
        let adapter = detect_known_adapter(host_lower);
        let site = adapter.map_or_else(
            || host_lower.trim_start_matches("www.").to_owned(),
            adapter_display_name,
        );
        return (adapter, site, UrlKind::ImageUrl);
    }

    if let Some(kind) = crate::source::mangadex::classify_url(url) {
        return (Some("mangadex"), "MangaDex".to_owned(), kind);
    }
    if let Some(kind) = crate::source::truyenqq::classify_url(url) {
        return (Some("truyenqq"), "TruyenQQ".to_owned(), kind);
    }
    if let Some(kind) = crate::source::moetruyen::classify_url(url) {
        return (Some("moetruyen"), "MoeTruyen".to_owned(), kind);
    }

    let kind = classify_generic_url_kind(url);
    let site = host_lower.trim_start_matches("www.").to_owned();
    (None, site, kind)
}

fn detect_known_adapter(host_lower: &str) -> Option<&'static str> {
    if host_lower == "mangadex.org" || host_lower.ends_with(".mangadex.org") {
        Some("mangadex")
    } else if crate::source::truyenqq::is_truyenqq_host(host_lower) {
        Some("truyenqq")
    } else if crate::source::moetruyen::is_moetruyen_host(host_lower) {
        Some("moetruyen")
    } else {
        None
    }
}

fn adapter_display_name(id: &str) -> String {
    match id {
        "mangadex" => "MangaDex".to_owned(),
        "truyenqq" => "TruyenQQ".to_owned(),
        "moetruyen" => "MoeTruyen".to_owned(),
        other => other.to_owned(),
    }
}

fn is_direct_image_url(url: &Url) -> bool {
    let path_lower = url.path().to_ascii_lowercase();
    [".jpg", ".jpeg", ".png", ".webp", ".avif", ".gif", ".bmp"]
        .iter()
        .any(|ext| path_lower.ends_with(ext))
}

fn classify_generic_url_kind(url: &Url) -> UrlKind {
    let path = url.path().trim_matches('/').to_ascii_lowercase();
    let has_search_query = url.query_pairs().any(|(k, v)| {
        let key = k.to_ascii_lowercase();
        !v.trim().is_empty() && matches!(key.as_str(), "q" | "s" | "search" | "keyword" | "query")
    });

    if path.starts_with("api/") || path == "api" || path.starts_with("graphql") {
        return UrlKind::ApiEndpoint;
    }
    if has_search_query || path.contains("search") || path.contains("tim-kiem") {
        return UrlKind::SearchPage;
    }
    if path.is_empty() {
        return UrlKind::WebsiteHome;
    }

    let segments: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    if segments.iter().any(|seg| {
        matches!(
            *seg,
            "login" | "signin" | "sign-in" | "dang-nhap" | "auth" | "oauth"
        )
    }) {
        return UrlKind::LoginPage;
    }
    if path.contains("cdn-cgi/challenge")
        || segments
            .iter()
            .any(|seg| matches!(*seg, "captcha" | "challenge" | "verify"))
    {
        return UrlKind::VerificationPage;
    }

    let has_chapter_query = url.query_pairs().any(|(k, v)| {
        let key = k.to_ascii_lowercase();
        !v.trim().is_empty()
            && matches!(
                key.as_str(),
                "chapter" | "chap" | "ch" | "ep" | "episode" | "chuong"
            )
    });
    if has_chapter_query || segments.iter().any(|seg| is_chapter_segment(seg)) {
        return UrlKind::ChapterPage;
    }

    if is_catalog_segments(&segments) {
        return UrlKind::CatalogPage;
    }

    UrlKind::MangaDetail
}

fn is_chapter_segment(seg: &str) -> bool {
    seg.starts_with("chap")
        || seg.starts_with("chuong")
        || seg.starts_with("episode")
        || seg.contains("-chap-")
        || seg.contains("-chapter-")
        || seg.contains("-chuong-")
        || matches!(seg, "read" | "viewer")
}

fn is_catalog_segments(segments: &[&str]) -> bool {
    if segments.len() == 1
        && matches!(
            segments[0],
            "manga"
                | "mangas"
                | "comics"
                | "webtoon"
                | "manhwa"
                | "manhua"
                | "truyen-tranh"
                | "truyen"
                | "latest"
                | "popular"
                | "catalog"
                | "directory"
                | "genres"
                | "the-loai"
                | "danh-sach"
        )
    {
        return true;
    }
    segments.first().is_some_and(|first| {
        matches!(
            *first,
            "category"
                | "categories"
                | "genre"
                | "genres"
                | "tag"
                | "tags"
                | "the-loai"
                | "danh-sach"
        )
    })
}

/// Builds a structured [`RecognitionDiagnostic`] from a [`ScrapeError`] and request context.
#[must_use]
pub fn diagnostic_from_error(
    err: &ScrapeError,
    submitted_url: &str,
    resolved_url: Option<&Url>,
    url_kind: UrlKind,
    website: Option<&str>,
    adapter: Option<&str>,
    stage: &str,
) -> RecognitionDiagnostic {
    let (status, effective_kind, http_status, suggested_action) =
        classify_error_fields(err, url_kind, adapter);

    let redacted_submitted = redact_url_for_diagnostics(submitted_url);
    let redacted_resolved = resolved_url.map(|u| redact_url_for_diagnostics(u.as_str()));

    let mut report = SourceDiagnosticReport::start(
        &redacted_submitted,
        effective_kind,
        website.map(str::to_owned),
    );
    report.final_url.clone_from(&redacted_resolved);
    report.http_status = http_status;
    report.adapter_name = adapter.map(str::to_owned);
    if let Some(a) = adapter {
        report.selected_strategy = Some(if a == "generic_headless" {
            ExtractionStrategyKind::BrowserRendering
        } else if a == "generic" {
            ExtractionStrategyKind::NativeHtml
        } else {
            ExtractionStrategyKind::SiteAdapter
        });
    }
    if let Some(detail) = err.classified_detail() {
        report.evidence.clone_from(&detail.evidence);
    } else {
        report.evidence.push(err.to_string());
    }
    let browser_avail = if matches!(
        err,
        ScrapeError::InvalidUrl(_) | ScrapeError::DisallowedHost(_)
    ) {
        BrowserFallbackAvailability::Unavailable
    } else {
        BrowserFallbackAvailability::Available
    };
    report.apply_failure(err.error_code(), stage, browser_avail);

    RecognitionDiagnostic {
        status,
        url_kind: effective_kind,
        website: website.map(str::to_owned),
        adapter: adapter.map(str::to_owned),
        submitted_url: redacted_submitted,
        resolved_url: redacted_resolved,
        stage: stage.to_owned(),
        http_status,
        detail: err.to_string(),
        suggested_action,
        confidence: report.detection_confidence,
        report: Some(report),
        timestamp_unix: now_unix_secs(),
    }
}

fn classify_error_fields(
    err: &ScrapeError,
    url_kind: UrlKind,
    adapter: Option<&str>,
) -> (RecognitionStatus, UrlKind, Option<u16>, Option<String>) {
    match err {
        ScrapeError::Classified(boxed) => classify_classified_detail(boxed.code, boxed.status, url_kind, adapter),
        ScrapeError::InvalidUrl(_) | ScrapeError::DisallowedHost(_) => (
            RecognitionStatus::InvalidUrl,
            UrlKind::InvalidUrl,
            None,
            Some("Check the URL format and make sure it points to a public website.".to_owned()),
        ),
        ScrapeError::UnsupportedSource { .. } => (
            RecognitionStatus::UnsupportedWebsite,
            UrlKind::UnsupportedUrl,
            None,
            Some("Verify the link points to a supported manga website or catalog page.".to_owned()),
        ),
        ScrapeError::DnsFailure { .. } => (
            RecognitionStatus::DnsFailure,
            url_kind,
            None,
            Some("Check your DNS connection or try switching the in-app Encrypted DNS (DoH) provider in Settings -> Network.".to_owned()),
        ),
        ScrapeError::TlsFailure { .. } => (
            RecognitionStatus::TlsFailure,
            url_kind,
            None,
            Some("TLS handshake failed or was reset by the network/ISP. Try enabling an in-app Proxy or WARP tunnel in Settings -> Network.".to_owned()),
        ),
        ScrapeError::Timeout { .. } | ScrapeError::Network(_) | ScrapeError::Proxy(_) => (
            RecognitionStatus::NetworkFailure,
            url_kind,
            None,
            Some("Verify internet connectivity, DNS, or proxy configuration and retry.".to_owned()),
        ),
        ScrapeError::RateLimited { retry_after_secs, .. } => (
            RecognitionStatus::RateLimited,
            UrlKind::BlockedOrRestricted,
            Some(429),
            Some(retry_after_secs.map_or_else(
                || "The website rate-limited requests. Wait a moment before retrying.".to_owned(),
                |secs| format!("The website rate-limited requests. Wait about {secs}s before retrying."),
            )),
        ),
        ScrapeError::AccessRestricted { status, .. } => (
            RecognitionStatus::AccessRestricted,
            UrlKind::BlockedOrRestricted,
            Some(*status),
            Some("This website explicitly restricts automated access to this resource.".to_owned()),
        ),
        ScrapeError::Blocked { .. } => (
            RecognitionStatus::VerificationRequired,
            UrlKind::BlockedOrRestricted,
            Some(403),
            Some("This website requires interactive human verification before access.".to_owned()),
        ),
        ScrapeError::ContentProtected { .. } => (
            RecognitionStatus::ContentProtected,
            UrlKind::BlockedOrRestricted,
            None,
            Some("This chapter uses browser-only display protection. Open it directly in your external browser.".to_owned()),
        ),
        ScrapeError::ApiFailure { status, .. } => (
            RecognitionStatus::ApiFailure,
            url_kind,
            *status,
            Some("The website's API returned an error or changed schema. Retry later or check diagnostics.".to_owned()),
        ),
        ScrapeError::HttpStatus { status, .. } => (
            RecognitionStatus::HttpError,
            url_kind,
            Some(*status),
            Some(format!("The website returned HTTP {status}. Verify the page exists and is publicly accessible.")),
        ),
        ScrapeError::MissingMetadata { .. } => (
            RecognitionStatus::MissingMetadata,
            url_kind,
            None,
            Some("The page was reached, but required manga metadata could not be extracted.".to_owned()),
        ),
        ScrapeError::MissingImages { .. } | ScrapeError::InvalidImageFormat { .. } => (
            RecognitionStatus::ImageExtractionFailure,
            url_kind,
            None,
            Some("No readable chapter page images were exposed by this page.".to_owned()),
        ),
        ScrapeError::NotRecognized { .. }
        | ScrapeError::CorruptedData { .. }
        | ScrapeError::Headless(_) => {
            let st = if adapter.is_some_and(|a| a != "generic") {
                RecognitionStatus::WebsiteSupportedButPageUnrecognized
            } else {
                RecognitionStatus::ParsingFailure
            };
            (
                st,
                url_kind,
                None,
                Some("Try opening the website's catalog or an individual manga detail page.".to_owned()),
            )
        }
    }
}

fn classify_classified_detail(
    code: SourceErrorCode,
    status_opt: Option<u16>,
    url_kind: UrlKind,
    adapter: Option<&str>,
) -> (RecognitionStatus, UrlKind, Option<u16>, Option<String>) {
    let desc = describe_error_code(code);
    let status = status_for_error_code(code, adapter);
    let kind = match code {
        SourceErrorCode::LoginRequired | SourceErrorCode::SessionExpired => UrlKind::LoginPage,
        SourceErrorCode::VerificationRequired => UrlKind::VerificationPage,
        SourceErrorCode::PaywallOrSubscriptionRequired => UrlKind::PaywallPage,
        SourceErrorCode::NotAComicPage => UrlKind::NotAComicPage,
        SourceErrorCode::InvalidUrl => UrlKind::InvalidUrl,
        SourceErrorCode::UnsupportedWebsite => UrlKind::UnsupportedUrl,
        SourceErrorCode::AccessDenied
        | SourceErrorCode::GeoRestriction
        | SourceErrorCode::DrmProtectedContent
        | SourceErrorCode::ExtractionNotPermitted => UrlKind::BlockedOrRestricted,
        _ => url_kind,
    };
    (
        status,
        kind,
        status_opt,
        Some(desc.suggested_action_en.to_owned()),
    )
}

fn status_for_error_code(code: SourceErrorCode, adapter: Option<&str>) -> RecognitionStatus {
    match code {
        SourceErrorCode::InvalidUrl => RecognitionStatus::InvalidUrl,
        SourceErrorCode::UnsupportedWebsite => RecognitionStatus::UnsupportedWebsite,
        SourceErrorCode::NotAComicPage
        | SourceErrorCode::SourceDetectionFailed
        | SourceErrorCode::AdapterOutdated
        | SourceErrorCode::AdapterExecutionFailed
        | SourceErrorCode::UnsupportedApiFormat => {
            if adapter.is_some_and(|a| a != "generic" && a != "generic_headless") {
                RecognitionStatus::WebsiteSupportedButPageUnrecognized
            } else {
                RecognitionStatus::ParsingFailure
            }
        }
        SourceErrorCode::ChapterNotFound | SourceErrorCode::ServerError => {
            RecognitionStatus::HttpError
        }
        SourceErrorCode::ChapterContentNotLoaded
        | SourceErrorCode::JavascriptRenderRequired
        | SourceErrorCode::BrowserEngineUnavailable
        | SourceErrorCode::UnsupportedImageFormat
        | SourceErrorCode::ImageDecodeFailed
        | SourceErrorCode::ImageRequestFailed
        | SourceErrorCode::ImageSourceChanged => RecognitionStatus::ImageExtractionFailure,
        SourceErrorCode::LoginRequired
        | SourceErrorCode::SessionExpired
        | SourceErrorCode::AccessDenied
        | SourceErrorCode::GeoRestriction
        | SourceErrorCode::PaywallOrSubscriptionRequired
        | SourceErrorCode::ExtractionNotPermitted => RecognitionStatus::AccessRestricted,
        SourceErrorCode::VerificationRequired => RecognitionStatus::VerificationRequired,
        SourceErrorCode::DrmProtectedContent => RecognitionStatus::ContentProtected,
        SourceErrorCode::RateLimited => RecognitionStatus::RateLimited,
        SourceErrorCode::NetworkTimeout | SourceErrorCode::UnknownSourceError => {
            RecognitionStatus::NetworkFailure
        }
    }
}
