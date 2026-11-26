//! Short-lived in-memory cache of resolved [`mangacore::Chapter`] metadata.

use std::sync::Arc;
use std::time::Duration;

use mangacore::Chapter;
use moka::future::Cache;
use url::Url;

const CHAPTER_CACHE_MAX_ENTRIES: u64 = 128;
const CHAPTER_CACHE_TTL: Duration = Duration::from_mins(30);

/// Concurrent TTL cache for resolved chapters.
#[derive(Debug, Clone)]
pub(crate) struct ChapterCache {
    inner: Cache<Url, Arc<Chapter>>,
}

impl Default for ChapterCache {
    fn default() -> Self {
        Self::new()
    }
}

impl ChapterCache {
    /// Creates a chapter cache with a 30-minute TTL and 128-chapter capacity.
    pub(crate) fn new() -> Self {
        Self {
            inner: Cache::builder()
                .max_capacity(CHAPTER_CACHE_MAX_ENTRIES)
                .time_to_live(CHAPTER_CACHE_TTL)
                .build(),
        }
    }

    /// Returns a cached chapter if present and not expired.
    pub(crate) async fn get(&self, url: &Url) -> Option<Arc<Chapter>> {
        self.inner.get(url).await
    }

    /// Inserts a resolved chapter into the cache.
    pub(crate) async fn insert(&self, url: Url, chapter: Arc<Chapter>) {
        self.inner.insert(url, chapter).await;
    }

    /// Invalidates all cached chapters.
    pub(crate) fn invalidate_all(&self) {
        self.inner.invalidate_all();
    }
}
