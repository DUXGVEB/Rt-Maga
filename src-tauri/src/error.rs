//! Application-level error type serialized across the Tauri IPC boundary with structured
//! diagnostic categories and actionable hints.

use mangacore::{ScrapeError, SourceDiagnosticReport, SourceErrorCode};
use serde::Serialize;
use thiserror::Error;

/// Structured error returned by Tauri commands to the Svelte frontend.
#[derive(Debug, Error, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[non_exhaustive]
pub(crate) enum AppError {
    /// User pasted an invalid or disallowed URL.
    #[error("invalid url: {message}")]
    InvalidUrl { message: String },

    /// Target host or URL structure is not a supported manga source.
    #[error("unsupported source ({host}): {message}")]
    UnsupportedSource { host: String, message: String },

    /// Request timed out waiting for the remote server.
    #[error("request timeout ({timeout_secs}s): {message}")]
    Timeout { timeout_secs: u64, message: String },

    /// DNS hostname resolution failed.
    #[error("dns failure ({host}): {message}")]
    DnsFailure { host: String, message: String },

    /// TLS handshake, certificate, or SNI connection reset failed.
    #[error("tls failure ({host}): {message}")]
    TlsFailure { host: String, message: String },

    /// Remote server rate-limited requests (HTTP 429).
    #[error("rate limited: {message}")]
    RateLimited {
        retry_after_secs: Option<u64>,
        message: String,
    },

    /// Website explicitly restricted access (HTTP 401/403).
    #[error("access restricted ({status}) on {host}: {message}")]
    AccessRestricted {
        host: String,
        status: u16,
        message: String,
    },

    /// Remote documented website API returned an error or malformed schema.
    #[error("{service} api failure: {message}")]
    ApiFailure {
        service: String,
        status: Option<u16>,
        message: String,
    },

    /// Upstream server returned an HTTP error status code (4xx/5xx).
    #[error("http {status}: {message}")]
    HttpError { status: u16, message: String },

    /// Network or proxy transport failure.
    #[error("network error: {message}")]
    Network { message: String },

    /// Site presented a bot-verification challenge that requires manual user action.
    #[error("site requires verification: {host}")]
    Blocked { host: String, url: String },

    /// Chapter uses browser-only display protection (e.g. IMGX) and cannot be rendered as plain images.
    #[error("content display protection on {host}: {message}")]
    ContentProtected {
        host: String,
        url: String,
        message: String,
    },

    /// Page loaded, but parser failed to recognize manga metadata or chapter layout.
    #[error("parser failure: {message}")]
    NotRecognized { message: String },

    /// Expected manga metadata was missing from the fetched resource.
    #[error("missing metadata: {message}")]
    MissingMetadata { message: String },

    /// Chapter page parsed, but contained zero readable images.
    #[error("missing images: {message}")]
    MissingImages { message: String },

    /// Image payload had an unsupported MIME type or invalid binary header.
    #[error("invalid image format: {message}")]
    InvalidImageFormat { message: String },

    /// Corrupted or truncated upstream/local data.
    #[error("corrupted data: {message}")]
    CorruptedData { message: String },

    /// Local archive or document is encrypted / password-protected and cannot be read.
    #[error("encrypted archive: {message}")]
    EncryptedArchive { message: String },

    /// Local file format is unsupported or contains no readable comic pages.
    #[error("unsupported format: {message}")]
    UnsupportedFormat { message: String },

    /// Requested database record does not exist.
    #[error("not found: {message}")]
    NotFound { message: String },

    /// Local SQLite database error.
    #[error("database error: {message}")]
    Db { message: String },

    /// Rich classified source resolution, extraction, format, or access failure with diagnostic report.
    #[error("{message}")]
    SourceFailure {
        code: SourceErrorCode,
        message: String,
        report: Box<SourceDiagnosticReport>,
    },

    /// Unexpected internal error.
    #[error("internal error: {message}")]
    Internal { message: String },
}

impl AppError {
    /// Constructs [`Self::SourceFailure`] carrying a 27-code classification and full sanitized report.
    pub(crate) fn from_scrape_and_report(
        err: &ScrapeError,
        report: SourceDiagnosticReport,
    ) -> Self {
        let code = report.error_code.unwrap_or_else(|| err.error_code());
        let message = err.to_string();
        Self::SourceFailure {
            code,
            message,
            report: Box::new(report),
        }
    }

    /// Returns a stable machine-readable error category string for diagnostic logging.
    pub(crate) const fn category(&self) -> &'static str {
        match self {
            Self::InvalidUrl { .. } => "invalid_url",
            Self::UnsupportedSource { .. } => "unsupported_source",
            Self::Timeout { .. } => "timeout",
            Self::DnsFailure { .. } => "dns_failure",
            Self::TlsFailure { .. } => "tls_failure",
            Self::RateLimited { .. } => "rate_limited",
            Self::AccessRestricted { .. } => "access_restricted",
            Self::ApiFailure { .. } => "api_failure",
            Self::HttpError { .. } => "http_error",
            Self::Network { .. } => "network",
            Self::Blocked { .. } => "blocked",
            Self::ContentProtected { .. } => "content_protected",
            Self::NotRecognized { .. } => "parser_failure",
            Self::MissingMetadata { .. } => "missing_metadata",
            Self::MissingImages { .. } => "missing_images",
            Self::InvalidImageFormat { .. } => "invalid_image_format",
            Self::CorruptedData { .. } => "corrupted_data",
            Self::EncryptedArchive { .. } => "encrypted_archive",
            Self::UnsupportedFormat { .. } => "unsupported_format",
            Self::NotFound { .. } => "not_found",
            Self::Db { .. } => "database",
            Self::SourceFailure { .. } => "source_failure",
            Self::Internal { .. } => "internal",
        }
    }
}

impl From<ScrapeError> for AppError {
    fn from(err: ScrapeError) -> Self {
        match err {
            ScrapeError::InvalidUrl(message) | ScrapeError::DisallowedHost(message) => {
                Self::InvalidUrl { message }
            }
            ScrapeError::UnsupportedSource { host, url } => Self::UnsupportedSource {
                host,
                message: format!("Unsupported manga source at {url}"),
            },
            ScrapeError::Timeout { url, timeout_secs } => Self::Timeout {
                timeout_secs,
                message: format!("Timed out after {timeout_secs}s while connecting to {url}"),
            },
            ScrapeError::DnsFailure { host, url, detail } => Self::DnsFailure {
                host,
                message: format!("DNS resolution failed for {url}: {detail}"),
            },
            ScrapeError::TlsFailure { host, url, detail } => Self::TlsFailure {
                host,
                message: format!("TLS connection failed for {url}: {detail}"),
            },
            ScrapeError::RateLimited {
                url,
                retry_after_secs,
            } => Self::RateLimited {
                retry_after_secs,
                message: retry_after_secs.map_or_else(
                    || format!("Rate limited by {url}"),
                    |s| format!("Rate limited by {url} (retry after {s}s)"),
                ),
            },
            ScrapeError::AccessRestricted {
                host,
                url,
                status,
                detail,
            } => Self::AccessRestricted {
                host,
                status,
                message: format!("HTTP {status} restricted access at {url}: {detail}"),
            },
            ScrapeError::ApiFailure {
                service,
                url,
                status,
                detail,
            } => Self::ApiFailure {
                service,
                status,
                message: format!("API error at {url}: {detail}"),
            },
            ScrapeError::Blocked { host, url } => Self::Blocked {
                host,
                url: url.into(),
            },
            ScrapeError::ContentProtected { host, url, detail } => Self::ContentProtected {
                host,
                url,
                message: detail,
            },
            ScrapeError::NotRecognized { reason, url } => {
                if reason.contains("image") {
                    Self::MissingImages {
                        message: format!("No readable chapter images found at {url} ({reason})"),
                    }
                } else {
                    Self::NotRecognized {
                        message: format!("{reason} ({url})"),
                    }
                }
            }
            ScrapeError::MissingMetadata { url, detail } => Self::MissingMetadata {
                message: format!("Missing metadata at {url}: {detail}"),
            },
            ScrapeError::MissingImages { url } => Self::MissingImages {
                message: format!("Chapter contains no readable images ({url})"),
            },
            ScrapeError::InvalidImageFormat { url, detail } => Self::InvalidImageFormat {
                message: format!("{detail} ({url})"),
            },
            ScrapeError::CorruptedData { detail } => Self::CorruptedData { message: detail },
            ScrapeError::Network(e) => Self::Network {
                message: e.to_string(),
            },
            ScrapeError::Proxy(message) => Self::Network { message },
            ScrapeError::HttpStatus { status, url } => Self::HttpError {
                status,
                message: format!("HTTP {status} from {url}"),
            },
            ScrapeError::Headless(message) => Self::NotRecognized { message },
            other => Self::Internal {
                message: other.to_string(),
            },
        }
    }
}

impl From<sqlx::Error> for AppError {
    fn from(err: sqlx::Error) -> Self {
        Self::Db {
            message: err.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::AppError;

    #[test]
    fn app_error_size_is_within_clippy_limit() {
        assert!(
            std::mem::size_of::<AppError>() <= 128,
            "AppError exceeded clippy::result_large_err limit: {} bytes",
            std::mem::size_of::<AppError>()
        );
    }
}
