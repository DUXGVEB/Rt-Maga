//! Disk-backed LRU transient page cache (`<data_dir>/cache/img/pages/`) paired with a
//! dedicated, LRU-immune persistent shared cover store (`<data_dir>/covers/<xx>/<sha256>`).
//!
//! All screens (Home, Sources/Browse, Bookmarks, History, Manga detail, Random) share ONE
//! canonical URL SHA-256 key in `<data_dir>/covers/` so covers never re-download once stored
//! and survive LRU eviction as well as transient page-cache clears.

use std::borrow::Cow;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock};
use std::time::Duration;

use bytes::Bytes;
use dashmap::DashMap;
use dashmap::mapref::entry::Entry;
use mangacore::HttpClient;
use moka::future::Cache as MokaCache;
use serde::Serialize;
use sha2::{Digest, Sha256};
use tokio::sync::{Semaphore, watch};
use tokio::task::JoinSet;
use tracing::{debug, warn};
use url::Url;

use crate::error::AppError;
use crate::repo::{Database, now_unix_secs};

const FLAG_COVER: u8 = 1 << 0;
const FLAG_SITE: u8 = 1 << 1;
const FLAG_MANGA_ID: u8 = 1 << 2;
const FLAG_CHAPTER_URL: u8 = 1 << 3;

/// Global concurrency semaphore bounding background cover prefetch / hydration tasks.
static PREFETCH_SEM: LazyLock<Semaphore> = LazyLock::new(|| Semaphore::new(4));

/// Maximum concurrent workers inside a single batch cover hydration pass.
const BATCH_HYDRATION_CONCURRENCY: usize = 4;

/// Per-cover timeout during background backup hydration so stalled hosts never wedge the queue.
const COVER_HYDRATION_TIMEOUT: Duration = Duration::from_secs(12);

/// Subdirectory holding persistent manga cover thumbnails (`<data_dir>/covers/<xx>/<sha256>`
/// and legacy `<cache_dir>/covers/<xx>/<sha256>` fallback).
const COVERS_SUBDIR: &str = "covers";

/// Subdirectory holding transient chapter page images (`<cache_dir>/pages/<xx>/<sha256>`),
/// aggressively purged after finishing a chapter/manga, exiting the app, or clearing cache.
const PAGES_SUBDIR: &str = "pages";

/// Summary statistics for the local image cache.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CacheInfo {
    /// Total bytes stored on disk.
    pub bytes: u64,
    /// Number of cached image files.
    pub files: u64,
}

/// Disk cache usage aggregated by manga source hostname.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CacheSourceStats {
    pub site: String,
    pub bytes: u64,
    pub files: u64,
}

/// Disk cache usage aggregated by individual manga.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CacheMangaStats {
    pub manga_id: i64,
    pub title: String,
    pub site: String,
    pub bytes: u64,
    pub files: u64,
}

/// Optional ownership metadata associated with a cached image for scoped clearing.
#[derive(Debug, Clone, Default)]
pub(crate) struct CacheEntryMeta<'a> {
    pub site: Option<&'a str>,
    pub manga_id: Option<i64>,
    pub is_cover: bool,
    pub chapter_url: Option<&'a str>,
}

/// Single remote cover entry queued for bounded background hydration into `<data_dir>/covers/`.
#[derive(Debug, Clone)]
pub(crate) struct CoverHydrationTarget {
    pub cover_url: Url,
    pub referer: Url,
    pub site: Option<String>,
    pub manga_id: Option<i64>,
}

/// A cached image loaded from disk.
#[derive(Debug, Clone)]
pub(crate) struct CachedImage {
    /// Raw image bytes.
    pub bytes: Bytes,
    /// MIME content type.
    pub content_type: String,
}

#[derive(Debug, Clone)]
struct CachedIndexEntry {
    content_type: String,
    flags: u8,
    last_touch_secs: i64,
}

impl CachedIndexEntry {
    const fn is_cover(&self) -> bool {
        (self.flags & FLAG_COVER) != 0
    }

    fn apply_meta(&mut self, meta: &CacheEntryMeta<'_>, now: i64) {
        if meta.is_cover {
            self.flags |= FLAG_COVER;
        }
        if meta.site.is_some() {
            self.flags |= FLAG_SITE;
        }
        if meta.manga_id.is_some() {
            self.flags |= FLAG_MANGA_ID;
        }
        if meta.chapter_url.is_some() {
            self.flags |= FLAG_CHAPTER_URL;
        }
        self.last_touch_secs = now;
    }

    fn needs_db_update(&self, meta: &CacheEntryMeta<'_>, now: i64) -> bool {
        now.saturating_sub(self.last_touch_secs) >= 3600
            || (meta.is_cover && (self.flags & FLAG_COVER) == 0)
            || (meta.site.is_some() && (self.flags & FLAG_SITE) == 0)
            || (meta.manga_id.is_some() && (self.flags & FLAG_MANGA_ID) == 0)
            || (meta.chapter_url.is_some() && (self.flags & FLAG_CHAPTER_URL) == 0)
    }
}

/// Derives the persistent `<data_dir>/covers/` path from `cache_dir`.
///
/// When `cache_dir` follows the standard `<data_dir>/cache/img` layout, covers live at
/// `<data_dir>/covers` completely separate from transient `<data_dir>/cache/img/pages/`.
/// For standalone test directories that do not end with `cache/img`, covers live in an
/// isolated sibling `<dir>/covers` subdirectory so tests never pollute shared temp paths.
fn resolve_persistent_covers_dir(cache_dir: &Path) -> PathBuf {
    let is_img_leaf = cache_dir
        .file_name()
        .and_then(|s| s.to_str())
        .is_some_and(|n| n.eq_ignore_ascii_case("img"));
    if is_img_leaf
        && let Some(cache_parent) = cache_dir.parent()
        && cache_parent
            .file_name()
            .and_then(|s| s.to_str())
            .is_some_and(|n| n.eq_ignore_ascii_case("cache"))
        && let Some(data_dir) = cache_parent.parent()
    {
        return data_dir.join(COVERS_SUBDIR);
    }
    cache_dir.join(COVERS_SUBDIR)
}

/// Canonicalizes an image/cover URL before hashing so trivial formatting differences
/// (surrounding whitespace, uppercase scheme/host, default ports, URL `#fragment`) map to
/// the exact same SHA-256 key across all app views while preserving existing keys verbatim.
fn canonicalize_url_for_key(raw: &str) -> Cow<'_, str> {
    let trimmed = raw.trim();
    let Ok(mut parsed) = Url::parse(trimmed) else {
        return Cow::Borrowed(trimmed);
    };
    if parsed.fragment().is_some() {
        parsed.set_fragment(None);
    }
    let normalized = parsed.as_str();
    if normalized == trimmed {
        Cow::Borrowed(trimmed)
    } else {
        Cow::Owned(normalized.to_owned())
    }
}

fn infer_content_type_from_bytes(bytes: &[u8]) -> String {
    infer::get(bytes)
        .map(|kind| kind.mime_type().to_owned())
        .filter(|mime| mime.starts_with("image/"))
        .unwrap_or_else(|| "image/jpeg".to_owned())
}

/// Maximum byte budget for the hot in-RAM cover cache (`24 MiB` holds ~200–400 small covers
/// so Home, Bookmarks, History, Sources, and Manga detail paint covers with zero disk/SQLite lag).
const COVER_RAM_CACHE_MAX_BYTES: u64 = 24 * 1024 * 1024;

/// Manages local image files across two strictly separated stores:
/// - **Persistent shared cover store** (`<data_dir>/covers/<xx>/<sha256>`): immune to LRU
///   eviction and transient cache clears; only unreferenced covers are pruned on permanent
///   manga deletion or explicit cover removal.
/// - **Transient chapter page cache** (`<data_dir>/cache/img/pages/<xx>/<sha256>`): bounded by
///   SQLite LRU eviction and cleared automatically on chapter/manga exit or manual cache clear.
#[derive(Debug, Clone)]
pub(crate) struct ImageCache {
    dir: PathBuf,
    covers_dir: PathBuf,
    db: Database,
    in_flight: Arc<DashMap<String, watch::Sender<bool>>>,
    index_mem: Arc<DashMap<String, CachedIndexEntry>>,
    cover_ram: MokaCache<String, CachedImage>,
}

impl ImageCache {
    /// Creates a new [`ImageCache`] rooted at `dir`, resolving the persistent cover store at
    /// `<data_dir>/covers/` when `dir` is `<data_dir>/cache/img`, migrating any legacy
    /// `<cache_dir>/covers/` files seamlessly, and hydrating the in-memory index.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Internal`] if the cache or cover store directories cannot be created.
    pub(crate) async fn new(dir: PathBuf, db: Database) -> Result<Self, AppError> {
        let covers_dir = resolve_persistent_covers_dir(&dir);
        Self::new_with_covers_dir(dir, covers_dir, db).await
    }

    /// Creates a new [`ImageCache`] with an explicit persistent `covers_dir` separate from the
    /// transient page cache root `dir`.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Internal`] if the directories cannot be created.
    pub(crate) async fn new_with_covers_dir(
        dir: PathBuf,
        covers_dir: PathBuf,
        db: Database,
    ) -> Result<Self, AppError> {
        tokio::fs::create_dir_all(&covers_dir)
            .await
            .map_err(|e| AppError::Internal {
                message: format!("failed to create persistent cover store dir: {e}"),
            })?;
        tokio::fs::create_dir_all(dir.join(PAGES_SUBDIR))
            .await
            .map_err(|e| AppError::Internal {
                message: format!("failed to create page cache dir: {e}"),
            })?;

        let rows: Vec<(String, String, i64, i64, i64, i64, i64)> = sqlx::query_as(
            "SELECT key, content_type, is_cover, last_access,
                    CASE WHEN site IS NOT NULL THEN 1 ELSE 0 END,
                    CASE WHEN manga_id IS NOT NULL THEN 1 ELSE 0 END,
                    CASE WHEN chapter_url IS NOT NULL THEN 1 ELSE 0 END
             FROM image_cache",
        )
        .fetch_all(db.pool())
        .await
        .unwrap_or_default();

        let index_mem = Arc::new(DashMap::with_capacity(rows.len()));
        for (k, ct, cov, last_acc, has_site, has_manga, has_chap) in rows {
            let mut flags = 0_u8;
            if cov != 0 {
                flags |= FLAG_COVER;
            }
            if has_site != 0 {
                flags |= FLAG_SITE;
            }
            if has_manga != 0 {
                flags |= FLAG_MANGA_ID;
            }
            if has_chap != 0 {
                flags |= FLAG_CHAPTER_URL;
            }
            index_mem.insert(
                k,
                CachedIndexEntry {
                    content_type: ct,
                    flags,
                    last_touch_secs: last_acc,
                },
            );
        }

        let cover_ram = MokaCache::builder()
            .max_capacity(COVER_RAM_CACHE_MAX_BYTES)
            .weigher(|_k: &String, v: &CachedImage| {
                u32::try_from(v.bytes.len().max(1)).unwrap_or(u32::MAX)
            })
            .build();

        let cache = Self {
            dir,
            covers_dir,
            db,
            in_flight: Arc::new(DashMap::new()),
            index_mem,
            cover_ram,
        };
        cache.migrate_legacy_covers_dir().await;
        Ok(cache)
    }

    /// Returns the root directory of the persistent shared cover store (`<data_dir>/covers/`).
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "inspected by cover store integration tests")
    )]
    pub(crate) fn covers_dir(&self) -> &Path {
        &self.covers_dir
    }

    /// Returns `true` when `url` is currently indexed in the local cache or present on disk in
    /// the persistent shared cover store.
    pub(crate) fn has_cached_url(&self, url: &str) -> bool {
        let key = Self::key_for_url(url);
        self.index_mem.contains_key(&key) || self.cover_file_path(&key).is_file()
    }

    /// Returns `true` when `url` is already stored persistently in `<data_dir>/covers/`.
    pub(crate) fn has_persistent_cover(&self, url: &str) -> bool {
        let key = Self::key_for_url(url);
        self.index_mem
            .get(&key)
            .is_some_and(|entry| entry.is_cover())
            || self.cover_file_path(&key).is_file()
    }

    /// Spawns a bounded background task to prefetch and persistently store a cover thumbnail or
    /// site icon into `<data_dir>/covers/` (`is_cover = 1`) when not already cached.
    pub(crate) fn spawn_cover_prefetch(
        &self,
        http: HttpClient,
        cover_url: Url,
        referer: Url,
        site: Option<String>,
        manga_id: Option<i64>,
        _max_mb: u32,
    ) {
        if self.has_persistent_cover(cover_url.as_str()) {
            return;
        }
        let cache = self.clone();
        tokio::spawn(async move {
            let Ok(_permit) = PREFETCH_SEM.acquire().await else {
                return;
            };
            cache
                .fetch_and_store_cover_once(&http, &cover_url, &referer, site.as_deref(), manga_id)
                .await;
        });
    }

    /// Spawns a single bounded background hydration task that deduplicates and fetches missing
    /// remote covers into `<data_dir>/covers/` (used after `.tachibk` and `.rtmaga` imports).
    pub(crate) fn spawn_batch_cover_hydration(
        &self,
        http: HttpClient,
        targets: Vec<CoverHydrationTarget>,
    ) {
        if targets.is_empty() {
            return;
        }
        let mut seen_keys: HashSet<String> = HashSet::with_capacity(targets.len());
        let pending: Vec<CoverHydrationTarget> = targets
            .into_iter()
            .filter(|t| {
                let key = Self::key_for_url(t.cover_url.as_str());
                seen_keys.insert(key) && !self.has_persistent_cover(t.cover_url.as_str())
            })
            .collect();
        if pending.is_empty() {
            return;
        }

        let cache = self.clone();
        tokio::spawn(async move {
            let mut join_set = JoinSet::new();
            for target in pending {
                while join_set.len() >= BATCH_HYDRATION_CONCURRENCY {
                    let _ = join_set.join_next().await;
                }
                let cache_clone = cache.clone();
                let http_clone = http.clone();
                join_set.spawn(async move {
                    let Ok(_permit) = PREFETCH_SEM.acquire().await else {
                        return;
                    };
                    let _ = tokio::time::timeout(
                        COVER_HYDRATION_TIMEOUT,
                        cache_clone.fetch_and_store_cover_once(
                            &http_clone,
                            &target.cover_url,
                            &target.referer,
                            target.site.as_deref(),
                            target.manga_id,
                        ),
                    )
                    .await;
                });
            }
            while join_set.join_next().await.is_some() {}
        });
    }

    async fn fetch_and_store_cover_once(
        &self,
        http: &HttpClient,
        cover_url: &Url,
        referer: &Url,
        site: Option<&str>,
        manga_id: Option<i64>,
    ) {
        let key = Self::key_for_url(cover_url.as_str());
        if self.has_persistent_cover(cover_url.as_str()) {
            return;
        }
        let Some(_guard) = self.begin_single_flight(&key).await else {
            return;
        };
        if self.has_persistent_cover(cover_url.as_str()) {
            return;
        }
        if let Ok(fetched) = http.fetch_image(cover_url, Some(referer)).await
            && !fetched.bytes.is_empty()
        {
            self.put_cover(
                &key,
                &fetched.bytes,
                &fetched.content_type,
                CacheEntryMeta {
                    site,
                    manga_id,
                    is_cover: true,
                    chapter_url: None,
                },
            )
            .await;
        }
    }

    /// Computes a deterministic SHA-256 hex key for a canonicalized image or cover URL.
    pub(crate) fn key_for_url(url: &str) -> String {
        let canonical = canonicalize_url_for_key(url);
        let mut hasher = Sha256::new();
        hasher.update(canonical.as_bytes());
        let digest = hasher.finalize();
        let mut out = String::with_capacity(64);
        for byte in digest {
            use std::fmt::Write;
            let _ = write!(&mut out, "{byte:02x}");
        }
        out
    }

    /// Reads a persistent cover directly from `<data_dir>/covers/<xx>/<sha256>` (with seamless
    /// fallback & lazy migration from legacy `<cache_dir>/covers/` or promoted `pages/`).
    ///
    /// Even if the SQLite `image_cache` row was missing, an existing persistent cover file on
    /// disk is served immediately and re-indexed so covers are NEVER re-downloaded once saved.
    pub(crate) async fn get_cover(
        &self,
        key: &str,
        mut meta: CacheEntryMeta<'_>,
    ) -> Option<CachedImage> {
        meta.is_cover = true;
        meta.chapter_url = None;

        if let Some(hit) = self.cover_ram.get(key).await {
            return Some(hit);
        }

        let data = self.read_or_migrate_cover_file(key).await?;
        let now = now_unix_secs();

        if let Some(mut entry) = self.load_or_fetch_index_entry(key).await {
            let cached = CachedImage {
                bytes: Bytes::from(data),
                content_type: entry.content_type.clone(),
            };
            self.cover_ram.insert(key.to_owned(), cached.clone()).await;

            if entry.needs_db_update(&meta, now) {
                entry.apply_meta(&meta, now);
                self.index_mem.insert(key.to_owned(), entry);
                let db = self.db.clone();
                let key_owned = key.to_owned();
                let site_owned = meta.site.map(str::to_owned);
                let manga_id = meta.manga_id;
                tokio::spawn(async move {
                    let _ = sqlx::query(
                        "UPDATE image_cache
                         SET last_access = ?,
                             site = COALESCE(site, ?),
                             manga_id = COALESCE(manga_id, ?),
                             is_cover = 1,
                             chapter_url = NULL
                         WHERE key = ?",
                    )
                    .bind(now)
                    .bind(site_owned)
                    .bind(manga_id)
                    .bind(key_owned)
                    .execute(db.pool())
                    .await;
                });
            }
            return Some(cached);
        }

        // Disk-hit recovery: file exists in persistent `<data_dir>/covers/` without an index row.
        let content_type = infer_content_type_from_bytes(&data);
        let size = i64::try_from(data.len()).unwrap_or(i64::MAX);
        let cached = CachedImage {
            bytes: Bytes::from(data),
            content_type: content_type.clone(),
        };
        self.cover_ram.insert(key.to_owned(), cached.clone()).await;
        self.upsert_index_row(key, &content_type, size, now, &meta)
            .await;

        Some(cached)
    }

    /// Writes a cover image atomically into the persistent shared cover store
    /// (`<data_dir>/covers/<xx>/<sha256>`). Never triggers transient page LRU eviction.
    pub(crate) async fn put_cover(
        &self,
        key: &str,
        bytes: &Bytes,
        content_type: &str,
        mut meta: CacheEntryMeta<'_>,
    ) {
        meta.is_cover = true;
        meta.chapter_url = None;

        self.cover_ram
            .insert(
                key.to_owned(),
                CachedImage {
                    bytes: bytes.clone(),
                    content_type: content_type.to_owned(),
                },
            )
            .await;

        let path = self.cover_file_path(key);
        if !write_file_atomically(&path, bytes).await {
            return;
        }

        // Remove any duplicate legacy copy under `<cache_dir>/covers/` if distinct.
        if let Some(legacy_cover) = self.legacy_cover_file_path(key) {
            let _ = tokio::fs::remove_file(legacy_cover).await;
        }

        let size = i64::try_from(bytes.len()).unwrap_or(i64::MAX);
        let now = now_unix_secs();
        self.upsert_index_row(key, content_type, size, now, &meta)
            .await;
    }

    /// Reads a cached image if present on disk, routing cover requests (`meta.is_cover == true`)
    /// directly to the persistent shared cover store and transient pages to `<cache_dir>/pages/`.
    pub(crate) async fn get(&self, key: &str, meta: CacheEntryMeta<'_>) -> Option<CachedImage> {
        if meta.is_cover {
            return self.get_cover(key, meta).await;
        }

        let mut entry = self.load_or_fetch_index_entry(key).await?;
        if entry.is_cover() {
            return self.get_cover(key, meta).await;
        }

        let data = self.read_existing_page_file(key).await;
        let Some(data) = data else {
            self.index_mem.remove(key);
            let _ = sqlx::query("DELETE FROM image_cache WHERE key = ? AND is_cover = 0")
                .bind(key)
                .execute(self.db.pool())
                .await;
            return None;
        };

        let now = now_unix_secs();
        if entry.needs_db_update(&meta, now) {
            let _ = sqlx::query(
                "UPDATE image_cache
                 SET last_access = ?,
                     site = COALESCE(site, ?),
                     manga_id = COALESCE(manga_id, ?),
                     is_cover = MAX(is_cover, ?),
                     chapter_url = COALESCE(chapter_url, ?)
                 WHERE key = ?",
            )
            .bind(now)
            .bind(meta.site)
            .bind(meta.manga_id)
            .bind(i64::from(meta.is_cover))
            .bind(meta.chapter_url)
            .bind(key)
            .execute(self.db.pool())
            .await;
            entry.apply_meta(&meta, now);
            self.index_mem.insert(key.to_owned(), entry.clone());
        }

        Some(CachedImage {
            bytes: Bytes::from(data),
            content_type: entry.content_type,
        })
    }

    /// Writes an image atomically to disk:
    /// - Covers (`meta.is_cover == true`, or keys already indexed as persistent covers) go
    ///   straight into `<data_dir>/covers/` and bypass transient LRU eviction.
    /// - Transient chapter pages (`meta.is_cover == false`) go into `<cache_dir>/pages/` and
    ///   trigger page-only LRU eviction if transient usage exceeds `max_mb`.
    pub(crate) async fn put(
        &self,
        key: &str,
        bytes: &Bytes,
        content_type: &str,
        max_mb: u32,
        meta: CacheEntryMeta<'_>,
    ) {
        let already_cover = self.index_mem.get(key).is_some_and(|e| e.is_cover());
        if meta.is_cover || already_cover {
            self.put_cover(key, bytes, content_type, meta).await;
            return;
        }

        let path = self.page_file_path(key);
        if !write_file_atomically(&path, bytes).await {
            return;
        }

        let size = i64::try_from(bytes.len()).unwrap_or(i64::MAX);
        let now = now_unix_secs();
        self.upsert_index_row(key, content_type, size, now, &meta)
            .await;

        self.evict_if_needed(max_mb).await;
    }

    /// Coordinates concurrent requests for the same key so only one network fetch executes.
    ///
    /// Returns `Some(guard)` when the caller owns the fetch, or `None` after waiting for an
    /// in-flight fetch to complete.
    pub(crate) async fn begin_single_flight(&self, key: &str) -> Option<SingleFlightGuard> {
        let mut rx = match self.in_flight.entry(key.to_owned()) {
            Entry::Occupied(occ) => occ.get().subscribe(),
            Entry::Vacant(vac) => {
                let (tx, _rx) = watch::channel(false);
                vac.insert(tx.clone());
                return Some(SingleFlightGuard {
                    key: key.to_owned(),
                    in_flight: Arc::clone(&self.in_flight),
                    tx,
                });
            }
        };

        let _ = rx.wait_for(|done| *done).await;
        None
    }

    /// Returns total bytes and file count currently stored in the clearable transient page cache
    /// (`is_cover = 0`). Persistent covers in `<data_dir>/covers/` are tracked separately.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Db`] on SQLite query failure.
    pub(crate) async fn info(&self) -> Result<CacheInfo, AppError> {
        let (bytes, files): (i64, i64) = sqlx::query_as(
            "SELECT COALESCE(SUM(size), 0), COUNT(*) FROM image_cache WHERE is_cover = 0",
        )
        .fetch_one(self.db.pool())
        .await?;

        Ok(CacheInfo {
            bytes: u64::try_from(bytes).unwrap_or(0),
            files: u64::try_from(files).unwrap_or(0),
        })
    }

    /// Returns total bytes and file count currently stored in the persistent shared cover store.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Db`] on SQLite query failure.
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "used by persistent cover store unit tests and diagnostics"
        )
    )]
    pub(crate) async fn cover_store_info(&self) -> Result<CacheInfo, AppError> {
        let (bytes, files): (i64, i64) = sqlx::query_as(
            "SELECT COALESCE(SUM(size), 0), COUNT(*) FROM image_cache WHERE is_cover = 1",
        )
        .fetch_one(self.db.pool())
        .await?;

        Ok(CacheInfo {
            bytes: u64::try_from(bytes).unwrap_or(0),
            files: u64::try_from(files).unwrap_or(0),
        })
    }

    /// Returns transient page cache usage grouped by source site hostname.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Db`] on SQLite failure.
    pub(crate) async fn list_by_source(&self) -> Result<Vec<CacheSourceStats>, AppError> {
        let rows: Vec<(String, i64, i64)> = sqlx::query_as(
            "SELECT COALESCE(NULLIF(TRIM(site), ''), 'unknown') AS src,
                    COALESCE(SUM(size), 0) AS total_bytes,
                    COUNT(*) AS file_count
             FROM image_cache
             WHERE is_cover = 0
             GROUP BY src
             ORDER BY total_bytes DESC",
        )
        .fetch_all(self.db.pool())
        .await?;

        Ok(rows
            .into_iter()
            .map(|(site, bytes, files)| CacheSourceStats {
                site,
                bytes: u64::try_from(bytes).unwrap_or(0),
                files: u64::try_from(files).unwrap_or(0),
            })
            .collect())
    }

    /// Returns transient page cache usage grouped by individual manga.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Db`] on SQLite failure.
    pub(crate) async fn list_by_manga(&self) -> Result<Vec<CacheMangaStats>, AppError> {
        let rows: Vec<(i64, String, String, i64, i64)> = sqlx::query_as(
            "SELECT c.manga_id,
                    COALESCE(m.title, 'Manga #' || c.manga_id) AS title,
                    COALESCE(m.site, c.site, 'unknown') AS site,
                    COALESCE(SUM(c.size), 0) AS total_bytes,
                    COUNT(*) AS file_count
             FROM image_cache c
             LEFT JOIN manga m ON m.id = c.manga_id
             WHERE c.manga_id IS NOT NULL AND c.is_cover = 0
             GROUP BY c.manga_id
             ORDER BY total_bytes DESC",
        )
        .fetch_all(self.db.pool())
        .await?;

        Ok(rows
            .into_iter()
            .map(|(manga_id, title, site, bytes, files)| CacheMangaStats {
                manga_id,
                title,
                site,
                bytes: u64::try_from(bytes).unwrap_or(0),
                files: u64::try_from(files).unwrap_or(0),
            })
            .collect())
    }

    /// Deletes transient page image files (`is_cover = 0`) belonging strictly to `site`, keeping
    /// persistent covers, library metadata, history, bookmarks, and settings intact.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Db`] on SQLite failure.
    pub(crate) async fn clear_by_source(&self, site: &str) -> Result<CacheInfo, AppError> {
        let keys: Vec<(String,)> = if site == "unknown" {
            sqlx::query_as(
                "SELECT key FROM image_cache
                 WHERE is_cover = 0 AND (site IS NULL OR TRIM(site) = '')",
            )
            .fetch_all(self.db.pool())
            .await?
        } else {
            sqlx::query_as("SELECT key FROM image_cache WHERE is_cover = 0 AND site = ?")
                .bind(site)
                .fetch_all(self.db.pool())
                .await?
        };

        for (key,) in &keys {
            self.index_mem.remove(key);
            self.remove_page_locations(key).await;
        }

        if site == "unknown" {
            sqlx::query(
                "DELETE FROM image_cache
                 WHERE is_cover = 0 AND (site IS NULL OR TRIM(site) = '')",
            )
            .execute(self.db.pool())
            .await?;
        } else {
            sqlx::query("DELETE FROM image_cache WHERE is_cover = 0 AND site = ?")
                .bind(site)
                .execute(self.db.pool())
                .await?;
        }

        if !keys.is_empty() {
            self.db.checkpoint_wal_truncate().await;
        }
        self.info().await
    }

    /// Deletes transient page cache files belonging strictly to `manga_id`.
    ///
    /// - If `manga_id` still exists in `manga` (cache management from Settings), its persistent
    ///   cover in `<data_dir>/covers/` is preserved intact.
    /// - If `manga_id` has already been permanently deleted/pruned from `manga`, any cover keys
    ///   owned by `manga_id` that are no longer referenced by any remaining manga or saved source
    ///   are also cleaned up.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Db`] on SQLite failure.
    pub(crate) async fn clear_by_manga(&self, manga_id: i64) -> Result<CacheInfo, AppError> {
        let page_keys: Vec<(String,)> =
            sqlx::query_as("SELECT key FROM image_cache WHERE manga_id = ? AND is_cover = 0")
                .bind(manga_id)
                .fetch_all(self.db.pool())
                .await?;

        for (key,) in &page_keys {
            self.index_mem.remove(key);
            self.remove_page_locations(key).await;
        }

        sqlx::query("DELETE FROM image_cache WHERE manga_id = ? AND is_cover = 0")
            .bind(manga_id)
            .execute(self.db.pool())
            .await?;

        let manga_exists: Option<(i64,)> =
            sqlx::query_as("SELECT 1 FROM manga WHERE id = ? LIMIT 1")
                .bind(manga_id)
                .fetch_optional(self.db.pool())
                .await?;

        let mut removed_any = !page_keys.is_empty();
        if manga_exists.is_none() {
            let cover_keys: Vec<(String,)> =
                sqlx::query_as("SELECT key FROM image_cache WHERE manga_id = ? AND is_cover = 1")
                    .bind(manga_id)
                    .fetch_all(self.db.pool())
                    .await?;
            for (key,) in cover_keys {
                if self.remove_cover_key_if_unreferenced(&key).await {
                    removed_any = true;
                }
            }
        }

        if removed_any {
            self.db.checkpoint_wal_truncate().await;
        }
        self.info().await
    }

    /// Removes the persistent cover file for `cover_url` **only** if no remaining `manga` row or
    /// `saved_source` icon references the same canonical cover SHA-256 key.
    ///
    /// Returns `true` when the cover file was unreferenced and removed.
    pub(crate) async fn remove_cover_if_unreferenced(&self, cover_url: &str) -> bool {
        let key = Self::key_for_url(cover_url);
        self.remove_cover_key_if_unreferenced(&key).await
    }

    async fn remove_cover_key_if_unreferenced(&self, key: &str) -> bool {
        let remaining_manga_covers: Vec<(i64, String)> =
            sqlx::query_as("SELECT id, cover_url FROM manga WHERE cover_url IS NOT NULL")
                .fetch_all(self.db.pool())
                .await
                .unwrap_or_default();

        if let Some((other_id, _)) = remaining_manga_covers
            .iter()
            .find(|(_, url)| Self::key_for_url(url) == key)
        {
            // Cover is still shared by another manga; keep file and re-associate ownership.
            let _ = sqlx::query("UPDATE image_cache SET manga_id = ? WHERE key = ?")
                .bind(*other_id)
                .bind(key)
                .execute(self.db.pool())
                .await;
            return false;
        }

        let remaining_source_icons: Vec<(String,)> =
            sqlx::query_as("SELECT icon_url FROM saved_source WHERE icon_url IS NOT NULL")
                .fetch_all(self.db.pool())
                .await
                .unwrap_or_default();

        if remaining_source_icons
            .iter()
            .any(|(url,)| Self::key_for_url(url) == key)
        {
            let _ = sqlx::query("UPDATE image_cache SET manga_id = NULL WHERE key = ?")
                .bind(key)
                .execute(self.db.pool())
                .await;
            return false;
        }

        self.index_mem.remove(key);
        self.remove_all_locations(key).await;
        let _ = sqlx::query("DELETE FROM image_cache WHERE key = ?")
            .bind(key)
            .execute(self.db.pool())
            .await;
        true
    }

    /// Deletes cached chapter page images (`is_cover = 0`) for a single finished chapter
    /// while preserving persistent covers and per-manga reader settings.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Db`] on SQLite failure.
    pub(crate) async fn clear_chapter_pages(
        &self,
        manga_id: i64,
        chapter_url: &str,
    ) -> Result<CacheInfo, AppError> {
        let keys: Vec<(String,)> = sqlx::query_as(
            "SELECT key FROM image_cache
             WHERE manga_id = ? AND is_cover = 0
               AND RTRIM(COALESCE(chapter_url, ''), '/') = RTRIM(?, '/')",
        )
        .bind(manga_id)
        .bind(chapter_url)
        .fetch_all(self.db.pool())
        .await?;

        for (key,) in &keys {
            self.index_mem.remove(key);
            self.remove_page_locations(key).await;
        }

        if !keys.is_empty() {
            sqlx::query(
                "DELETE FROM image_cache
                 WHERE manga_id = ? AND is_cover = 0
                   AND RTRIM(COALESCE(chapter_url, ''), '/') = RTRIM(?, '/')",
            )
            .bind(manga_id)
            .bind(chapter_url)
            .execute(self.db.pool())
            .await?;
            debug!(
                manga_id,
                removed = keys.len(),
                "purged finished chapter page cache"
            );
        }

        self.info().await
    }

    /// Deletes all transient chapter page images (`is_cover = 0`) for `manga_id` when exiting
    /// the reader or completing the manga, while preserving its persistent cover and settings.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Db`] on SQLite failure.
    pub(crate) async fn clear_manga_pages(&self, manga_id: i64) -> Result<CacheInfo, AppError> {
        let keys: Vec<(String,)> =
            sqlx::query_as("SELECT key FROM image_cache WHERE manga_id = ? AND is_cover = 0")
                .bind(manga_id)
                .fetch_all(self.db.pool())
                .await?;

        for (key,) in &keys {
            self.index_mem.remove(key);
            self.remove_page_locations(key).await;
        }

        if !keys.is_empty() {
            sqlx::query("DELETE FROM image_cache WHERE manga_id = ? AND is_cover = 0")
                .bind(manga_id)
                .execute(self.db.pool())
                .await?;
            self.db.checkpoint_wal_truncate().await;
            debug!(
                manga_id,
                removed = keys.len(),
                "purged manga chapter page cache while keeping persistent cover"
            );
        }

        self.info().await
    }

    /// Aggressively purges ALL transient chapter page images (`is_cover = 0`) across the app
    /// while preserving persistent covers in `<data_dir>/covers/` and all user/per-manga settings.
    ///
    /// Also sweeps any pre-migration flat shard directories outside `covers/` so upgrades
    /// immediately reclaim disk space, then truncates the SQLite WAL file.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Db`] or [`AppError::Internal`] if cleanup fails.
    pub(crate) async fn clear_all_chapter_pages(&self) -> Result<CacheInfo, AppError> {
        self.migrate_legacy_covers_dir().await;

        sqlx::query("DELETE FROM image_cache WHERE is_cover = 0")
            .execute(self.db.pool())
            .await?;
        self.index_mem.retain(|_, v| v.is_cover());

        let pages_dir = self.dir.join(PAGES_SUBDIR);
        if pages_dir.exists() {
            let _ = tokio::fs::remove_dir_all(&pages_dir).await;
        }
        tokio::fs::create_dir_all(&pages_dir)
            .await
            .map_err(|e| AppError::Internal {
                message: format!("failed to recreate page cache dir: {e}"),
            })?;

        // Sweep legacy flat shard folders (<cache_dir>/<hex>) left from before bucket separation,
        // never deleting persistent cover directories.
        if let Ok(mut entries) = tokio::fs::read_dir(&self.dir).await {
            while let Ok(Some(entry)) = entries.next_entry().await {
                let name = entry.file_name();
                let name_str = name.to_string_lossy();
                if name_str == COVERS_SUBDIR || name_str == PAGES_SUBDIR {
                    continue;
                }
                let path = entry.path();
                if path == self.covers_dir {
                    continue;
                }
                if path.is_dir() {
                    let _ = tokio::fs::remove_dir_all(&path).await;
                } else {
                    let _ = tokio::fs::remove_file(&path).await;
                }
            }
        }

        self.db.checkpoint_wal_truncate().await;
        self.info().await
    }

    /// Purges the transient chapter page cache (`<cache_dir>/pages/` and `is_cover = 0` rows)
    /// while keeping the persistent shared cover store (`<data_dir>/covers/`), manga metadata,
    /// and per-manga settings completely intact.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Db`] or [`AppError::Internal`] if cleanup fails.
    pub(crate) async fn clear(&self) -> Result<(), AppError> {
        let _ = self.clear_all_chapter_pages().await?;
        Ok(())
    }

    /// Evicts oldest **transient chapter pages** (`is_cover = 0`) until transient page disk usage
    /// is within `max_mb`. Persistent covers in `<data_dir>/covers/` are strictly immune to LRU
    /// eviction and are never counted against the transient page cache budget.
    pub(crate) async fn evict_if_needed(&self, max_mb: u32) {
        let limit_bytes = i64::from(max_mb).saturating_mul(1024 * 1024);
        let Ok((mut total,)) = sqlx::query_as::<_, (i64,)>(
            "SELECT COALESCE(SUM(size), 0) FROM image_cache WHERE is_cover = 0",
        )
        .fetch_one(self.db.pool())
        .await
        else {
            return;
        };

        if total <= limit_bytes {
            return;
        }

        let target = (limit_bytes * 9) / 10;
        let mut evicted_any = false;
        for _ in 0..50 {
            if total <= target {
                break;
            }
            let Ok(candidates) = sqlx::query_as::<_, (String, i64)>(
                "SELECT key, size FROM image_cache
                 WHERE is_cover = 0
                 ORDER BY last_access ASC
                 LIMIT 200",
            )
            .fetch_all(self.db.pool())
            .await
            else {
                return;
            };

            if candidates.is_empty() {
                break;
            }

            for (key, size) in candidates {
                if total <= target {
                    break;
                }
                self.index_mem.remove(&key);
                self.remove_page_locations(&key).await;
                let _ = sqlx::query("DELETE FROM image_cache WHERE key = ? AND is_cover = 0")
                    .bind(&key)
                    .execute(self.db.pool())
                    .await;
                total = total.saturating_sub(size);
                evicted_any = true;
                debug!(%key, "evicted lru transient chapter page");
            }
        }
        if evicted_any {
            self.db.checkpoint_wal_truncate().await;
        }
    }

    async fn load_or_fetch_index_entry(&self, key: &str) -> Option<CachedIndexEntry> {
        if let Some(hit) = self.index_mem.get(key) {
            return Some(hit.clone());
        }

        let row: Option<(String, i64, i64, i64, i64, i64)> = sqlx::query_as(
            "SELECT content_type, is_cover, last_access,
                    CASE WHEN site IS NOT NULL THEN 1 ELSE 0 END,
                    CASE WHEN manga_id IS NOT NULL THEN 1 ELSE 0 END,
                    CASE WHEN chapter_url IS NOT NULL THEN 1 ELSE 0 END
             FROM image_cache WHERE key = ?",
        )
        .bind(key)
        .fetch_optional(self.db.pool())
        .await
        .ok()?;

        let (ct, cov, last_acc, has_site, has_manga, has_chap) = row?;
        let mut flags = 0_u8;
        if cov != 0 {
            flags |= FLAG_COVER;
        }
        if has_site != 0 {
            flags |= FLAG_SITE;
        }
        if has_manga != 0 {
            flags |= FLAG_MANGA_ID;
        }
        if has_chap != 0 {
            flags |= FLAG_CHAPTER_URL;
        }
        let hydrated = CachedIndexEntry {
            content_type: ct,
            flags,
            last_touch_secs: last_acc,
        };
        self.index_mem.insert(key.to_owned(), hydrated.clone());
        Some(hydrated)
    }

    async fn upsert_index_row(
        &self,
        key: &str,
        content_type: &str,
        size: i64,
        now: i64,
        meta: &CacheEntryMeta<'_>,
    ) {
        let _ = sqlx::query(
            "INSERT INTO image_cache (key, content_type, size, last_access, site, manga_id, is_cover, chapter_url)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(key) DO UPDATE SET
                content_type = excluded.content_type,
                size = excluded.size,
                last_access = excluded.last_access,
                site = COALESCE(excluded.site, image_cache.site),
                manga_id = COALESCE(excluded.manga_id, image_cache.manga_id),
                is_cover = MAX(image_cache.is_cover, excluded.is_cover),
                chapter_url = CASE
                    WHEN MAX(image_cache.is_cover, excluded.is_cover) = 1 THEN NULL
                    ELSE COALESCE(excluded.chapter_url, image_cache.chapter_url)
                END",
        )
        .bind(key)
        .bind(content_type)
        .bind(size)
        .bind(now)
        .bind(meta.site)
        .bind(meta.manga_id)
        .bind(i64::from(meta.is_cover))
        .bind(meta.chapter_url)
        .execute(self.db.pool())
        .await;

        self.index_mem
            .entry(key.to_owned())
            .and_modify(|e| {
                content_type.clone_into(&mut e.content_type);
                e.apply_meta(meta, now);
            })
            .or_insert_with(|| {
                let mut e = CachedIndexEntry {
                    content_type: content_type.to_owned(),
                    flags: 0,
                    last_touch_secs: now,
                };
                e.apply_meta(meta, now);
                e
            });
    }

    fn cover_file_path(&self, key: &str) -> PathBuf {
        let prefix = key.get(0..2).unwrap_or("00");
        self.covers_dir.join(prefix).join(key)
    }

    fn legacy_covers_dir(&self) -> Option<PathBuf> {
        let legacy = self.dir.join(COVERS_SUBDIR);
        (legacy != self.covers_dir).then_some(legacy)
    }

    fn legacy_cover_file_path(&self, key: &str) -> Option<PathBuf> {
        let prefix = key.get(0..2).unwrap_or("00");
        self.legacy_covers_dir().map(|d| d.join(prefix).join(key))
    }

    fn page_file_path(&self, key: &str) -> PathBuf {
        let prefix = key.get(0..2).unwrap_or("00");
        self.dir.join(PAGES_SUBDIR).join(prefix).join(key)
    }

    fn legacy_flat_file_path(&self, key: &str) -> PathBuf {
        let prefix = key.get(0..2).unwrap_or("00");
        self.dir.join(prefix).join(key)
    }

    async fn read_or_migrate_cover_file(&self, key: &str) -> Option<Vec<u8>> {
        let primary = self.cover_file_path(key);
        if let Ok(data) = tokio::fs::read(&primary).await {
            return Some(data);
        }

        // Seamless fallback + migration from legacy `<cache_dir>/covers/<xx>/<sha256>`.
        if let Some(legacy_cover) = self.legacy_cover_file_path(key)
            && let Ok(data) = tokio::fs::read(&legacy_cover).await
        {
            if write_file_atomically(&primary, &data).await {
                let _ = tokio::fs::remove_file(&legacy_cover).await;
            }
            return Some(data);
        }

        // Promote a transient page or legacy flat shard into the persistent cover store.
        let page_path = self.page_file_path(key);
        if let Ok(data) = tokio::fs::read(&page_path).await {
            let _ = write_file_atomically(&primary, &data).await;
            return Some(data);
        }

        let flat_path = self.legacy_flat_file_path(key);
        if let Ok(data) = tokio::fs::read(&flat_path).await {
            if write_file_atomically(&primary, &data).await {
                let _ = tokio::fs::remove_file(&flat_path).await;
            }
            return Some(data);
        }

        None
    }

    async fn read_existing_page_file(&self, key: &str) -> Option<Vec<u8>> {
        let primary = self.page_file_path(key);
        if let Ok(data) = tokio::fs::read(&primary).await {
            return Some(data);
        }
        // Reuse persistent cover if the same image URL was already persisted as a cover.
        if let Ok(data) = tokio::fs::read(self.cover_file_path(key)).await {
            return Some(data);
        }
        if let Some(legacy_cover) = self.legacy_cover_file_path(key)
            && let Ok(data) = tokio::fs::read(legacy_cover).await
        {
            return Some(data);
        }
        tokio::fs::read(self.legacy_flat_file_path(key)).await.ok()
    }

    async fn remove_page_locations(&self, key: &str) {
        let _ = tokio::fs::remove_file(self.page_file_path(key)).await;
        let _ = tokio::fs::remove_file(self.legacy_flat_file_path(key)).await;
    }

    async fn remove_all_locations(&self, key: &str) {
        self.cover_ram.invalidate(key).await;
        let _ = tokio::fs::remove_file(self.page_file_path(key)).await;
        let _ = tokio::fs::remove_file(self.cover_file_path(key)).await;
        if let Some(legacy_cover) = self.legacy_cover_file_path(key) {
            let _ = tokio::fs::remove_file(legacy_cover).await;
        }
        let _ = tokio::fs::remove_file(self.legacy_flat_file_path(key)).await;
    }

    /// Seamlessly migrates existing cover files from `<cache_dir>/covers/<xx>/<sha256>` to
    /// `<data_dir>/covers/<xx>/<sha256>` when those directories are distinct.
    async fn migrate_legacy_covers_dir(&self) {
        let Some(legacy_root) = self.legacy_covers_dir() else {
            return;
        };
        if !tokio::fs::try_exists(&legacy_root).await.unwrap_or(false) {
            return;
        }

        let Ok(mut shard_entries) = tokio::fs::read_dir(&legacy_root).await else {
            return;
        };
        while let Ok(Some(shard)) = shard_entries.next_entry().await {
            let shard_path = shard.path();
            if !shard_path.is_dir() {
                continue;
            }
            let shard_name = shard.file_name();
            let dst_shard = self.covers_dir.join(&shard_name);
            let Ok(mut files) = tokio::fs::read_dir(&shard_path).await else {
                continue;
            };
            while let Ok(Some(file_entry)) = files.next_entry().await {
                let src_file = file_entry.path();
                if !src_file.is_file() {
                    continue;
                }
                let dst_file = dst_shard.join(file_entry.file_name());
                if tokio::fs::try_exists(&dst_file).await.unwrap_or(false) {
                    let _ = tokio::fs::remove_file(&src_file).await;
                    continue;
                }
                if tokio::fs::create_dir_all(&dst_shard).await.is_err() {
                    continue;
                }
                if tokio::fs::rename(&src_file, &dst_file).await.is_err()
                    && let Ok(bytes) = tokio::fs::read(&src_file).await
                    && write_file_atomically(&dst_file, &bytes).await
                {
                    let _ = tokio::fs::remove_file(&src_file).await;
                }
            }
            let _ = tokio::fs::remove_dir(&shard_path).await;
        }
        let _ = tokio::fs::remove_dir(&legacy_root).await;
    }
}

async fn write_file_atomically(path: &Path, bytes: &[u8]) -> bool {
    if let Some(parent) = path.parent()
        && let Err(err) = tokio::fs::create_dir_all(parent).await
    {
        warn!(%err, "failed to create image store shard dir");
        return false;
    }

    let nonce: u64 = rand::random();
    let tmp_path = path.with_extension(format!("tmp.{nonce:016x}"));
    if let Err(err) = tokio::fs::write(&tmp_path, bytes).await {
        let _ = tokio::fs::remove_file(&tmp_path).await;
        warn!(%err, "failed to write temp image file");
        return false;
    }
    if let Err(err) = tokio::fs::rename(&tmp_path, path).await {
        let _ = tokio::fs::remove_file(&tmp_path).await;
        warn!(%err, "failed to commit temp image file");
        return false;
    }
    true
}

/// Drop guard that releases a single-flight slot and wakes waiting callers.
#[derive(Debug)]
pub(crate) struct SingleFlightGuard {
    key: String,
    in_flight: Arc<DashMap<String, watch::Sender<bool>>>,
    tx: watch::Sender<bool>,
}

impl Drop for SingleFlightGuard {
    fn drop(&mut self) {
        self.in_flight.remove(&self.key);
        let _ = self.tx.send(true);
    }
}

#[cfg(test)]
mod tests {
    use mangacore::{ChapterRef, Manga, SiteId};
    use url::Url;

    use super::*;

    async fn seed_manga_and_cache(
        db: &Database,
        cache: &ImageCache,
        custom_json: &str,
    ) -> (i64, String, String, String) {
        let manga = Manga::minimal(
            Url::parse("https://example.org/manga/alpha").expect("valid url"),
            SiteId::generic("example.org"),
            "Alpha".to_owned(),
            Some(Url::parse("https://cdn.example.org/alpha-cover.jpg").expect("valid cover")),
            vec![
                ChapterRef::simple(
                    Url::parse("https://example.org/manga/alpha/ch-1").expect("valid ch1"),
                    "Ch 1".to_owned(),
                    Some(1.0),
                ),
                ChapterRef::simple(
                    Url::parse("https://example.org/manga/alpha/ch-2").expect("valid ch2"),
                    "Ch 2".to_owned(),
                    Some(2.0),
                ),
            ],
        );
        let manga_id = db.upsert_manga(&manga).await.expect("upsert manga");
        db.set_manga_reader_settings(manga_id, Some(custom_json))
            .await
            .expect("set reader settings");

        let cover_key = ImageCache::key_for_url("https://cdn.example.org/alpha-cover.jpg");
        let ch1_key = ImageCache::key_for_url("https://cdn.example.org/alpha-ch1-p1.jpg");
        let ch2_key = ImageCache::key_for_url("https://cdn.example.org/alpha-ch2-p1.jpg");

        let entries = [
            (&cover_key, b"COVER_BYTES".as_slice(), true, None),
            (
                &ch1_key,
                b"CH1_PAGE_BYTES".as_slice(),
                false,
                Some("https://example.org/manga/alpha/ch-1"),
            ),
            (
                &ch2_key,
                b"CH2_PAGE_BYTES".as_slice(),
                false,
                Some("https://example.org/manga/alpha/ch-2"),
            ),
        ];
        for (key, payload, is_cover, chapter_url) in entries {
            cache
                .put(
                    key,
                    &Bytes::copy_from_slice(payload),
                    "image/jpeg",
                    128,
                    CacheEntryMeta {
                        site: Some("example.org"),
                        manga_id: Some(manga_id),
                        is_cover,
                        chapter_url,
                    },
                )
                .await;
        }
        (manga_id, cover_key, ch1_key, ch2_key)
    }

    #[tokio::test]
    async fn single_flight_wakes_concurrent_waiter_without_hang() {
        let db = Database::open_in_memory().await.expect("in-memory db");
        let dir = std::env::temp_dir().join(format!("rtmaga_cache_test_{}", rand::random::<u64>()));
        let cache = ImageCache::new(dir.clone(), db).await.expect("init cache");

        let owner_guard = cache
            .begin_single_flight("img_key_1")
            .await
            .expect("first caller owns flight");

        let cache_waiter = cache.clone();
        let waiter =
            tokio::spawn(async move { cache_waiter.begin_single_flight("img_key_1").await });

        tokio::task::yield_now().await;
        drop(owner_guard);

        let outcome = tokio::time::timeout(Duration::from_secs(2), waiter)
            .await
            .expect("waiter must not hang")
            .expect("waiter task must succeed");
        assert!(
            outcome.is_none(),
            "second caller should wait and receive None"
        );

        let _ = tokio::fs::remove_dir_all(&dir).await;
    }

    #[test]
    fn canonical_cover_url_deduplication_matches_across_views() {
        let key_a = ImageCache::key_for_url("https://cdn.example.org/alpha-cover.jpg");
        let key_b = ImageCache::key_for_url("  https://CDN.EXAMPLE.ORG:443/alpha-cover.jpg#thumb ");
        assert_eq!(
            key_a, key_b,
            "canonical URL variations must map to identical cover SHA-256 key"
        );
    }

    #[tokio::test]
    async fn persistent_covers_survive_eviction_and_cache_clear() {
        let db = Database::open_in_memory().await.expect("in-memory db");
        let root = std::env::temp_dir().join(format!(
            "rtmaga_persistent_covers_{}",
            rand::random::<u64>()
        ));
        let cache_dir = root.join("cache").join("img");
        let expected_covers_dir = root.join("covers");
        let cache = ImageCache::new(cache_dir, db.clone())
            .await
            .expect("init cache");
        assert_eq!(
            cache.covers_dir(),
            expected_covers_dir.as_path(),
            "cache/img must resolve persistent cover store at <data_dir>/covers/"
        );

        let custom_json = r#"{"orientation":"horizontal"}"#;
        let (manga_id, cover_key, ch1_key, ch2_key) =
            seed_manga_and_cache(&db, &cache, custom_json).await;
        let cover_path = expected_covers_dir.join(&cover_key[0..2]).join(&cover_key);
        assert!(
            cover_path.is_file(),
            "cover must live in <data_dir>/covers/"
        );

        // 1. LRU eviction with `max_mb = 0` evicts pages while keeping covers 100% immune.
        cache.evict_if_needed(0).await;
        let ch1_gone = cache
            .get(&ch1_key, CacheEntryMeta::default())
            .await
            .is_none();
        let ch2_gone = cache
            .get(&ch2_key, CacheEntryMeta::default())
            .await
            .is_none();
        assert!(
            ch1_gone && ch2_gone,
            "LRU eviction must evict transient chapter pages"
        );
        let cover_hit = cache
            .get_cover(&cover_key, CacheEntryMeta::default())
            .await
            .is_some();
        assert!(
            cover_hit && cover_path.is_file(),
            "persistent cover must survive LRU eviction"
        );

        // 2. Re-add transient page and run full `clear()`: pages wiped, cover & settings survive.
        let page_meta = CacheEntryMeta {
            site: Some("example.org"),
            manga_id: Some(manga_id),
            is_cover: false,
            chapter_url: Some("https://example.org/manga/alpha/ch-1"),
        };
        cache
            .put(
                &ch1_key,
                &Bytes::from_static(b"TRANSIENT_PAGE"),
                "image/jpeg",
                128,
                page_meta,
            )
            .await;
        assert_eq!(
            cache.info().await.expect("info").files,
            1,
            "1 transient page"
        );

        cache.clear().await.expect("clear image cache");
        assert_eq!(
            cache.info().await.expect("info after clear").files,
            0,
            "pages cleared"
        );
        assert_eq!(
            cache.cover_store_info().await.expect("cover info").files,
            1,
            "cover kept"
        );
        let after_clear = cache
            .get_cover(&cover_key, CacheEntryMeta::default())
            .await
            .is_some();
        assert!(after_clear, "clear() must preserve persistent cover bytes");

        // 3. Clearing cache for an existing manga from Settings preserves its cover & settings.
        let _ = cache
            .clear_by_manga(manga_id)
            .await
            .expect("clear by manga");
        assert!(
            cover_path.is_file(),
            "clear_by_manga on existing manga keeps cover"
        );
        let reloaded = db
            .get_manga_by_id(manga_id)
            .await
            .expect("query")
            .expect("exists");
        assert_eq!(
            reloaded.reader_settings_json.as_deref(),
            Some(custom_json),
            "per-manga reader settings must survive all cache clears"
        );

        let _ = tokio::fs::remove_dir_all(&root).await;
    }

    #[tokio::test]
    async fn shared_cover_is_removed_only_when_last_referencing_manga_is_deleted() {
        let db = Database::open_in_memory().await.expect("in-memory db");
        let root =
            std::env::temp_dir().join(format!("rtmaga_shared_cover_ref_{}", rand::random::<u64>()));
        let cache_dir = root.join("cache").join("img");
        let cache = ImageCache::new(cache_dir, db.clone())
            .await
            .expect("init cache");

        let (alpha_id, cover_key, _, _) = seed_manga_and_cache(&db, &cache, "{}").await;
        let cover_path = root.join("covers").join(&cover_key[0..2]).join(&cover_key);
        assert!(cover_path.is_file(), "seeded cover must exist on disk");

        let manga_beta = Manga::minimal(
            Url::parse("https://example.org/manga/beta").expect("valid beta url"),
            SiteId::generic("example.org"),
            "Beta".to_owned(),
            Some(Url::parse("https://CDN.EXAMPLE.ORG/alpha-cover.jpg#dup").expect("valid cover")),
            vec![],
        );
        let beta_id = db.upsert_manga(&manga_beta).await.expect("upsert beta");

        db.delete_manga_permanently(alpha_id)
            .await
            .expect("delete alpha");
        assert!(
            !cache
                .remove_cover_if_unreferenced("https://cdn.example.org/alpha-cover.jpg")
                .await,
            "shared cover still referenced by Beta must NOT be deleted"
        );
        assert!(
            cover_path.is_file(),
            "shared cover file must remain while Beta references it"
        );

        db.delete_manga_permanently(beta_id)
            .await
            .expect("delete beta");
        assert!(
            cache
                .remove_cover_if_unreferenced("https://cdn.example.org/alpha-cover.jpg")
                .await,
            "unreferenced cover must be removed once last referencing manga is permanently deleted"
        );
        assert!(
            !cover_path.is_file(),
            "unreferenced cover file must be removed from disk"
        );

        let _ = tokio::fs::remove_dir_all(&root).await;
    }

    #[tokio::test]
    async fn migrates_legacy_cache_covers_dir_seamlessly_on_startup() {
        let db = Database::open_in_memory().await.expect("in-memory db");
        let root =
            std::env::temp_dir().join(format!("rtmaga_legacy_cover_mig_{}", rand::random::<u64>()));
        let cache_dir = root.join("cache").join("img");
        let cover_url = "https://cdn.example.org/legacy-cover.png";
        let key = ImageCache::key_for_url(cover_url);
        let legacy_path = cache_dir.join("covers").join(&key[0..2]).join(&key);

        tokio::fs::create_dir_all(legacy_path.parent().expect("parent"))
            .await
            .expect("create legacy dir");
        // Valid PNG magic header bytes so disk-hit MIME inference succeeds without a DB row.
        let png_bytes = [
            0x89_u8, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 0,
        ];
        tokio::fs::write(&legacy_path, png_bytes)
            .await
            .expect("write legacy cover");

        let cache = ImageCache::new(cache_dir, db).await.expect("init cache");
        let migrated_path = root.join("covers").join(&key[0..2]).join(&key);
        assert!(
            migrated_path.is_file(),
            "startup must migrate legacy <cache_dir>/covers/ file to <data_dir>/covers/"
        );
        assert!(
            !legacy_path.exists(),
            "migrated legacy cover file must be moved out of <cache_dir>/covers/"
        );

        let hit = cache
            .get_cover(&key, CacheEntryMeta::default())
            .await
            .expect("migrated cover must be readable even without pre-existing DB row");
        assert_eq!(
            hit.bytes.as_ref(),
            &png_bytes,
            "migrated cover bytes must match original bytes"
        );
        assert_eq!(
            hit.content_type, "image/png",
            "disk-recovered cover must infer PNG content type"
        );

        let _ = tokio::fs::remove_dir_all(&root).await;
    }
}
