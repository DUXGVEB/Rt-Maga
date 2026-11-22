//! Source registry coordinating pluggable site adapters, generic heuristics, and headless fallback.

use std::sync::Arc;

use tracing::{debug, warn};
use url::Url;

use crate::error::ScrapeError;
use crate::http::{HttpClient, normalize_url};
use crate::model::{Chapter, Resolved};
use crate::source::Source;
use crate::source::generic::GenericSource;
#[cfg(feature = "headless")]
use crate::source::headless::HeadlessBrowser;

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

/// Central coordinator that resolves arbitrary series or chapter URLs across three tiers.
#[derive(Debug, Clone)]
pub struct Registry {
    http: HttpClient,
    adapters: Arc<[Box<dyn Source>]>,
    generic: GenericSource,
    #[cfg(feature = "headless")]
    headless: HeadlessBrowser,
}

impl Registry {
    /// Creates a [`Registry`] with a default [`HttpClient`] and the generic heuristic pipeline.
    ///
    /// # Errors
    ///
    /// Returns [`ScrapeError::Network`] if the underlying [`HttpClient`] fails to initialize.
    pub fn new() -> Result<Self, ScrapeError> {
        let http = HttpClient::new()?;
        Ok(Self::with_http(http))
    }

    /// Creates a [`Registry`] using an existing [`HttpClient`].
    pub fn with_http(http: HttpClient) -> Self {
        Self::with_adapters(http, Vec::new())
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

    /// Resolves any user-pasted series or chapter link through the 3-tier pipeline:
    /// 1. Any matching registered custom [`Source`] adapter
    /// 2. Generic static HTML heuristics
    /// 3. Headless browser rendering (when `headless_policy` is [`HeadlessPolicy::Enabled`])
    ///
    /// # Errors
    ///
    /// Returns [`ScrapeError::InvalidUrl`], [`ScrapeError::DisallowedHost`],
    /// [`ScrapeError::Blocked`], [`ScrapeError::NotRecognized`], or network/headless errors.
    pub async fn resolve(
        &self,
        raw_url: &str,
        headless_policy: HeadlessPolicy,
    ) -> Result<Resolved, ScrapeError> {
        let url = normalize_url(raw_url)?;

        // Tier 1: registered custom adapter (if any).
        if let Some(adapter) = self.find_adapter(&url) {
            debug!(adapter = adapter.id(), %url, "trying registered adapter");
            match adapter.resolve(&self.http, &url).await {
                Ok(resolved) => return Ok(resolved),
                Err(err) if is_terminal_scrape_error(&err) => return Err(err),
                Err(err) => {
                    warn!(adapter = adapter.id(), %err, "adapter failed, falling back to generic");
                }
            }
        }

        // Tier 2: generic heuristic on static HTML.
        match self.generic.resolve(&self.http, &url).await {
            Ok(resolved) => return Ok(resolved),
            Err(err) if is_terminal_scrape_error(&err) => return Err(err),
            Err(err) => {
                if headless_policy == HeadlessPolicy::Disabled {
                    return Err(err);
                }
                debug!(%err, %url, "generic static resolve failed, trying headless");
            }
        }

        // Tier 3: headless browser fallback.
        self.resolve_headless(&url).await
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
            debug!(adapter = adapter.id(), %url, "fetching chapter via registered adapter");
            match adapter.chapter(&self.http, url).await {
                Ok(chapter) => return Ok(chapter),
                Err(err) if is_terminal_scrape_error(&err) => return Err(err),
                Err(err) => {
                    warn!(adapter = adapter.id(), %err, "adapter chapter failed, trying generic");
                }
            }
        }

        match self.generic.chapter(&self.http, url).await {
            Ok(chapter) => return Ok(chapter),
            Err(err) if is_terminal_scrape_error(&err) => return Err(err),
            Err(err) => {
                if headless_policy == HeadlessPolicy::Disabled {
                    return Err(err);
                }
                debug!(%err, %url, "generic chapter failed, trying headless");
            }
        }

        self.chapter_headless(url).await
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
            Err(ScrapeError::NotRecognized {
                url: url.clone(),
                reason: "headless fallback unavailable",
            })
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
            Err(ScrapeError::NotRecognized {
                url: url.clone(),
                reason: "headless fallback unavailable",
            })
        }
    }
}

/// Errors that represent policy/security/proxy enforcement and must never fall through to headless.
const fn is_terminal_scrape_error(err: &ScrapeError) -> bool {
    matches!(
        err,
        ScrapeError::Blocked { .. }
            | ScrapeError::Proxy(_)
            | ScrapeError::DisallowedHost(_)
            | ScrapeError::InvalidUrl(_)
    )
}

const _: () = {
    const fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<Registry>();
};
