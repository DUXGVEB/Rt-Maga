//! Source adapter trait and built-in site implementations.

pub mod generic;
#[cfg(feature = "headless")]
pub mod headless;

use async_trait::async_trait;
use url::Url;

use crate::error::ScrapeError;
use crate::http::HttpClient;
use crate::model::{Chapter, Resolved};

/// A site adapter capable of resolving manga/chapter URLs and fetching chapter images.
#[async_trait]
pub trait Source: Send + Sync + std::fmt::Debug {
    /// Short stable identifier for this source (e.g. `"generic"` or a custom adapter key).
    fn id(&self) -> &'static str;

    /// Returns `true` if this adapter handles the given `url`.
    fn matches(&self, url: &Url) -> bool;

    /// Resolves either a manga detail page or a direct chapter page.
    ///
    /// # Errors
    ///
    /// Returns a [`ScrapeError`] if fetching or parsing fails.
    async fn resolve(&self, http: &HttpClient, url: &Url) -> Result<Resolved, ScrapeError>;

    /// Fetches ordered page images and prev/next navigation for a chapter URL.
    ///
    /// # Errors
    ///
    /// Returns a [`ScrapeError`] if fetching or parsing fails.
    async fn chapter(&self, http: &HttpClient, url: &Url) -> Result<Chapter, ScrapeError>;
}
