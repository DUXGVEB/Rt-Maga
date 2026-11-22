//! Application-level error type serialized across the Tauri IPC boundary.

use mangacore::ScrapeError;
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

    /// Network or remote HTTP failure.
    #[error("network error: {message}")]
    Network { message: String },

    /// Site presented a bot-verification challenge that requires manual user action.
    #[error("site requires verification: {host}")]
    Blocked { host: String, url: String },

    /// Page loaded, but no manga metadata or chapter images could be extracted.
    #[error("unrecognized manga page: {message}")]
    NotRecognized { message: String },

    /// Requested database record does not exist.
    #[error("not found: {message}")]
    NotFound { message: String },

    /// Local SQLite database error.
    #[error("database error: {message}")]
    Db { message: String },

    /// Unexpected internal error.
    #[error("internal error: {message}")]
    Internal { message: String },
}

impl From<ScrapeError> for AppError {
    fn from(err: ScrapeError) -> Self {
        match err {
            ScrapeError::InvalidUrl(message) | ScrapeError::DisallowedHost(message) => {
                Self::InvalidUrl { message }
            }
            ScrapeError::Blocked { host, url } => Self::Blocked {
                host,
                url: url.into(),
            },
            ScrapeError::NotRecognized { reason, .. } => Self::NotRecognized {
                message: reason.to_owned(),
            },
            ScrapeError::Network(e) => Self::Network {
                message: e.to_string(),
            },
            ScrapeError::Proxy(message) => Self::Network { message },
            ScrapeError::HttpStatus { status, url } => Self::Network {
                message: format!("HTTP {status} ({url})"),
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
