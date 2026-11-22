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

    /// Transport-level HTTP failure (DNS, TLS, timeout, connection reset).
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

    /// Headless browser fallback failed or no compatible browser executable was found.
    #[error("headless browser error: {0}")]
    Headless(String),

    /// Proxy configuration, authentication, or connection failure.
    #[error("proxy error: {0}")]
    Proxy(String),
}
