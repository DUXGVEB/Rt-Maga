//! Short-lived in-memory cache of resolved [`crate::services::library::CatalogPageView`] pages
//! so back/forward pagination, filter toggles, and next-page prefetches render instantaneously.

use std::time::Duration;

use moka::future::Cache;

use crate::services::library::CatalogPageView;

const CATALOG_CACHE_MAX_ENTRIES: u64 = 48;
const CATALOG_CACHE_TTL: Duration = Duration::from_secs(3 * 60);

/// Concurrent TTL cache for catalog/search pages keyed by canonical request signature.
#[derive(Debug, Clone)]
pub(crate) struct CatalogCache {
    inner: Cache<String, CatalogPageView>,
}

impl Default for CatalogCache {
    fn default() -> Self {
        Self::new()
    }
}

impl CatalogCache {
    /// Creates a catalog page cache with a 3-minute TTL and 48-page capacity.
    pub(crate) fn new() -> Self {
        Self {
            inner: Cache::builder()
                .max_capacity(CATALOG_CACHE_MAX_ENTRIES)
                .time_to_live(CATALOG_CACHE_TTL)
                .build(),
        }
    }

    /// Returns a cached [`CatalogPageView`] if present and not expired.
    pub(crate) async fn get(&self, key: &str) -> Option<CatalogPageView> {
        self.inner.get(key).await
    }

    /// Stores a resolved [`CatalogPageView`] in the RAM cache.
    pub(crate) async fn insert(&self, key: String, page: CatalogPageView) {
        self.inner.insert(key, page).await;
    }

    /// Invalidates all cached catalog pages.
    pub(crate) fn invalidate_all(&self) {
        self.inner.invalidate_all();
    }
}
