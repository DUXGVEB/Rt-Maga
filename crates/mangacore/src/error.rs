//! Error types returned by [`crate::Registry`] and [`crate::HttpClient`].

use thiserror::Error;
use url::Url;

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

    /// Transport-level HTTP failure (DNS, TLS, connection reset).
    #[error("network request failed: {0}")]
    Network(#[from] reqwest::Error),

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

    /// Page loaded successfully, but neither manga metadata nor chapter images were recognized.
    #[error("could not recognize manga content at {url}: {reason}")]
    NotRecognized {
        /// Target URL that could not be parsed.
        url: Url,
        /// Short machine-friendly reason describing what was missing.
        reason: &'static str,
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
}
