//! Error types returned by [`crate::Registry`] and [`crate::HttpClient`].

use thiserror::Error;
use url::Url;

use crate::diagnostics::SourceErrorCode;

/// Boxed structured classification details stored inside [`ScrapeError::Classified`] to keep
/// the enum size compact.
#[derive(Debug, Clone)]
pub struct ClassifiedFailureDetail {
    /// Precise 27-code classification of the failure.
    pub code: SourceErrorCode,
    /// Target or final URL (redacted when necessary).
    pub url: String,
    /// Hostname associated with the request, if known.
    pub host: Option<String>,
    /// Upstream HTTP status code, if applicable.
    pub status: Option<u16>,
    /// Processing stage where failure occurred.
    pub stage: String,
    /// Human-readable diagnostic message.
    pub detail: String,
    /// Evidence items inspected before failing.
    pub evidence: Vec<String>,
}

/// Failure modes for resolving manga links, fetching pages, and parsing content.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum ScrapeError {
    /// The input string is not a valid HTTP or HTTPS URL.
    #[error("invalid url: {0}")]
    InvalidUrl(String),

    /// The target URL points to a blocked or non-public address.
    #[error("blocked address: {0}")]
    DisallowedHost(String),

    /// The target host or page structure is not a supported manga source.
    #[error("unsupported source ({host}): {url}")]
    UnsupportedSource {
        /// Hostname that is unsupported.
        host: String,
        /// Full target URL.
        url: Url,
    },

    /// Request timed out before the remote server responded.
    #[error("request timed out after {timeout_secs}s: {url}")]
    Timeout {
        /// Target URL that timed out.
        url: Url,
        /// Configured timeout in seconds.
        timeout_secs: u64,
    },

    /// DNS hostname resolution failed.
    #[error("dns resolution failed for {host} ({url}): {detail}")]
    DnsFailure {
        /// Hostname that could not be resolved.
        host: String,
        /// Full target URL.
        url: String,
        /// Underlying diagnostic detail.
        detail: String,
    },

    /// TLS handshake, certificate validation, or SNI connection reset failed.
    #[error("tls connection failed for {host} ({url}): {detail}")]
    TlsFailure {
        /// Hostname whose TLS handshake failed.
        host: String,
        /// Full target URL.
        url: String,
        /// Underlying diagnostic detail.
        detail: String,
    },

    /// Transport-level HTTP failure (other connection/I/O errors).
    #[error("network request failed: {0}")]
    Network(#[from] reqwest::Error),

    /// Remote server returned HTTP 429 Too Many Requests.
    #[error("rate limited by {url}")]
    RateLimited {
        /// Target URL that returned HTTP 429.
        url: String,
        /// Optional retry delay parsed from `Retry-After`.
        retry_after_secs: Option<u64>,
    },

    /// Remote server explicitly restricted access (HTTP 401/403 without an interactive challenge).
    #[error("access restricted ({status}) for {host} at {url}: {detail}")]
    AccessRestricted {
        /// Hostname denying access.
        host: String,
        /// Full target URL.
        url: String,
        /// HTTP status code (typically 401 or 403).
        status: u16,
        /// Diagnostic detail.
        detail: String,
    },

    /// Remote documented API endpoint returned an error status or invalid schema.
    #[error("{service} api failure at {url}: {detail}")]
    ApiFailure {
        /// Name of the upstream API service adapter.
        service: String,
        /// Request URL that failed.
        url: String,
        /// Optional HTTP status code returned by the API.
        status: Option<u16>,
        /// Diagnostic detail.
        detail: String,
    },

    /// Remote server returned an unexpected HTTP status code.
    #[error("http {status} while fetching {url}")]
    HttpStatus {
        /// HTTP status code returned by the remote server.
        status: u16,
        /// Target URL that produced the status code.
        url: Url,
    },

    /// The site presented a bot-verification or anti-bot challenge.
    #[error("site requires manual verification: {host}")]
    Blocked {
        /// Hostname that presented the challenge.
        host: String,
        /// Full URL that triggered the challenge.
        url: Url,
    },

    /// The chapter uses browser-only display protection (e.g. IMGX canvas tainting) that cannot be
    /// rendered directly as plain image files.
    #[error("content display protection active on {host} ({url}): {detail}")]
    ContentProtected {
        /// Hostname enforcing content display protection.
        host: String,
        /// Chapter or page URL protected by the upstream viewer.
        url: String,
        /// Short explanation of the protection mechanism detected.
        detail: String,
    },

    /// Page loaded successfully, but neither manga metadata nor chapter images were recognized.
    #[error("could not recognize manga content at {url}: {reason}")]
    NotRecognized {
        /// Target URL that could not be parsed.
        url: Url,
        /// Short machine-friendly reason describing what was missing.
        reason: &'static str,
    },

    /// Page matched a manga structure, but required metadata fields were missing or unusable.
    #[error("missing required metadata at {url}: {detail}")]
    MissingMetadata {
        /// Target URL lacking required metadata.
        url: String,
        /// Diagnostic description of missing metadata.
        detail: String,
    },

    /// Chapter page was parsed, but contained zero usable page images.
    #[error("no readable images found at {url}")]
    MissingImages {
        /// Chapter URL with missing images.
        url: Url,
    },

    /// Downloaded image payload had an unsupported MIME type or invalid binary header.
    #[error("invalid image format from {url}: {detail}")]
    InvalidImageFormat {
        /// Image URL returning invalid data.
        url: Url,
        /// Short diagnostic detail (MIME type or magic-byte mismatch).
        detail: String,
    },

    /// Upstream returned truncated or corrupted data.
    #[error("corrupted response data: {detail}")]
    CorruptedData {
        /// Diagnostic description of corruption.
        detail: String,
    },

    /// Headless browser fallback failed or no compatible browser executable was found.
    #[error("headless browser error: {0}")]
    Headless(String),

    /// Proxy configuration, authentication, or connection failure.
    #[error("proxy error: {0}")]
    Proxy(String),

    /// Precision-classified source resolution, extraction, format, or access failure.
    #[error("{0}")]
    Classified(Box<ClassifiedFailureDetail>),
}

impl std::fmt::Display for ClassifiedFailureDetail {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.detail)
    }
}

impl ScrapeError {
    /// Constructs a compact [`ScrapeError::Classified`] carrying a 27-code [`SourceErrorCode`].
    #[must_use]
    pub fn classified(detail: ClassifiedFailureDetail) -> Self {
        Self::Classified(Box::new(detail))
    }

    /// Maps any [`ScrapeError`] into the canonical 27-variant [`SourceErrorCode`] taxonomy.
    #[must_use]
    pub fn error_code(&self) -> SourceErrorCode {
        match self {
            Self::InvalidUrl(_) | Self::DisallowedHost(_) => SourceErrorCode::InvalidUrl,
            Self::UnsupportedSource { .. } => SourceErrorCode::UnsupportedWebsite,
            Self::Timeout { .. } => SourceErrorCode::NetworkTimeout,
            Self::DnsFailure { .. } | Self::TlsFailure { .. } | Self::Network(_) => {
                SourceErrorCode::UnknownSourceError
            }
            Self::Proxy(_) => SourceErrorCode::AccessDenied,
            Self::RateLimited { .. } => SourceErrorCode::RateLimited,
            Self::AccessRestricted { status, .. } => match *status {
                401 => SourceErrorCode::LoginRequired,
                402 => SourceErrorCode::PaywallOrSubscriptionRequired,
                451 => SourceErrorCode::GeoRestriction,
                _ => SourceErrorCode::AccessDenied,
            },
            Self::ApiFailure { status, .. } => match *status {
                Some(429) => SourceErrorCode::RateLimited,
                Some(401) => SourceErrorCode::LoginRequired,
                Some(403) => SourceErrorCode::AccessDenied,
                Some(404) => SourceErrorCode::ChapterNotFound,
                Some(s) if s >= 500 => SourceErrorCode::ServerError,
                _ => SourceErrorCode::UnsupportedApiFormat,
            },
            Self::HttpStatus { status, .. } => match *status {
                401 => SourceErrorCode::LoginRequired,
                402 => SourceErrorCode::PaywallOrSubscriptionRequired,
                403 => SourceErrorCode::AccessDenied,
                404 | 410 => SourceErrorCode::ChapterNotFound,
                429 => SourceErrorCode::RateLimited,
                451 => SourceErrorCode::GeoRestriction,
                s if s >= 500 => SourceErrorCode::ServerError,
                _ => SourceErrorCode::UnknownSourceError,
            },
            Self::Blocked { .. } => SourceErrorCode::VerificationRequired,
            Self::ContentProtected { .. } => SourceErrorCode::DrmProtectedContent,
            Self::NotRecognized { .. } | Self::MissingMetadata { .. } => {
                SourceErrorCode::SourceDetectionFailed
            }
            Self::MissingImages { .. } => SourceErrorCode::ChapterContentNotLoaded,
            Self::InvalidImageFormat { .. } => SourceErrorCode::UnsupportedImageFormat,
            Self::CorruptedData { .. } => SourceErrorCode::ImageDecodeFailed,
            Self::Headless(msg) => {
                let lower = msg.to_ascii_lowercase();
                if lower.contains("executable")
                    || lower.contains("disabled")
                    || lower.contains("unavailable")
                {
                    SourceErrorCode::BrowserEngineUnavailable
                } else {
                    SourceErrorCode::ChapterContentNotLoaded
                }
            }
            Self::Classified(boxed) => boxed.code,
        }
    }

    /// Returns the diagnostic [`ClassifiedFailureDetail`] if this error is [`Self::Classified`].
    #[must_use]
    pub fn classified_detail(&self) -> Option<&ClassifiedFailureDetail> {
        match self {
            Self::Classified(boxed) => Some(boxed),
            _ => None,
        }
    }
}
