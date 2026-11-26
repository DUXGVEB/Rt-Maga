//! Source adapter trait and built-in site implementations.

pub mod generic;
#[cfg(feature = "headless")]
pub mod headless;
pub mod mangadex;
pub mod moetruyen;
pub mod truyenqq;

use async_trait::async_trait;
use url::Url;

use crate::diagnostics::ExtractionStrategyKind;
use crate::error::ScrapeError;
use crate::http::HttpClient;
use crate::model::{Chapter, Resolved};
use crate::parse::filters::{CatalogFilterQuery, SiteFilterSchema, UnhandledFilterMask};

/// Authentication requirement profile declared by a site adapter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum AdapterAuthSupport {
    /// Content is generally public without an account.
    #[default]
    PublicOnly,
    /// Public content works anonymously, and browser session cookies unlock member pages.
    OptionalUserSession,
    /// Adapter normally expects an authenticated user session.
    AccountRequired,
}

/// Versioned metadata describing a site adapter's capabilities, domains, and known limitations.
#[derive(Debug, Clone)]
pub struct AdapterMetadata {
    /// Stable adapter identifier (matches [`Source::id`]).
    pub id: &'static str,
    /// Human-readable display name.
    pub name: &'static str,
    /// Independent adapter semantic version for update & outdated detection.
    pub version: &'static str,
    /// Supported domain patterns.
    pub domains: &'static [&'static str],
    /// Authentication capability declared by the adapter.
    pub auth_support: AdapterAuthSupport,
    /// Extraction strategies used by this adapter.
    pub strategies: &'static [ExtractionStrategyKind],
    /// Transparent user/diagnostic notes on known technical limitations (DRM, JS-only, etc.).
    pub known_limitations: &'static [&'static str],
}

/// Returns the built-in dedicated website adapters checked before the generic fallback.
#[must_use]
pub fn default_adapters() -> Vec<Box<dyn Source>> {
    vec![
        Box::new(mangadex::MangaDexSource),
        Box::new(truyenqq::TruyenQQSource),
        Box::new(moetruyen::MoeTruyenSource),
    ]
}

/// Returns the default baseline [`SiteFilterSchema`] for a built-in adapter ID or host.
#[must_use]
pub fn default_filter_schema_for_adapter(
    adapter_id: Option<&str>,
    host: Option<&str>,
) -> SiteFilterSchema {
    let resolved = adapter_id.or_else(|| host.and_then(crate::cms::match_builtin_adapter_id));
    match resolved {
        Some("truyenqq") => {
            let origin = format!("https://{}", host.unwrap_or("truyenqq.com.vn"));
            truyenqq::truyenqq_filter_schema(&origin, None, None)
        }
        Some("mangadex") => mangadex::mangadex_filter_schema(),
        _ => SiteFilterSchema::default(),
    }
}

pub(crate) fn try_build_builtin_filtered_url(
    base_url: &Url,
    current_url: Option<&Url>,
    parser_ref: Option<&str>,
    clean_q: Option<&str>,
    filters: &CatalogFilterQuery,
    page_num: u32,
) -> Option<Result<(Url, UnhandledFilterMask), ScrapeError>> {
    if parser_ref == Some("truyenqq")
        || base_url
            .host_str()
            .is_some_and(|h| truyenqq::is_truyenqq_host(&h.to_ascii_lowercase()))
    {
        return Some(truyenqq::build_truyenqq_filtered_url(
            base_url,
            current_url,
            clean_q,
            filters,
            page_num,
        ));
    }
    if parser_ref == Some("mangadex")
        || base_url
            .host_str()
            .is_some_and(|h| mangadex::is_mangadex_host(&h.to_ascii_lowercase()))
    {
        return Some(mangadex::build_mangadex_filtered_url(
            clean_q, filters, page_num,
        ));
    }
    None
}

/// A site adapter capable of resolving manga/chapter URLs and fetching chapter images.
#[async_trait]
pub trait Source: Send + Sync + std::fmt::Debug {
    /// Short stable identifier for this source (e.g. `"generic"` or a custom adapter key).
    fn id(&self) -> &'static str;

    /// Returns versioned capabilities and limitation metadata for this adapter.
    fn metadata(&self) -> AdapterMetadata {
        AdapterMetadata {
            id: self.id(),
            name: self.id(),
            version: "1.0.0",
            domains: &[],
            auth_support: AdapterAuthSupport::OptionalUserSession,
            strategies: &[ExtractionStrategyKind::SiteAdapter],
            known_limitations: &[],
        }
    }

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
