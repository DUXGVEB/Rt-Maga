//! Disk-backed LRU image cache indexed in SQLite with atomic writes, single-flight dedup,
//! and strict separation between persistent cover thumbnails and transient chapter page images.

use std::path::PathBuf;
use std::sync::{Arc, LazyLock};

use bytes::Bytes;
use dashmap::DashMap;
use dashmap::mapref::entry::Entry;
use mangacore::HttpClient;
use serde::Serialize;
use sha2::{Digest, Sha256};
use tokio::sync::{Semaphore, watch};
use tracing::{debug, warn};
use url::Url;

use crate::error::AppError;
use crate::repo::{Database, now_unix_secs};

const FLAG_COVER: u8 = 1 << 0;
const FLAG_SITE: u8 = 1 << 1;
const FLAG_MANGA_ID: u8 = 1 << 2;
const FLAG_CHAPTER_URL: u8 = 1 << 3;

static PREFETCH_SEM: LazyLock<Semaphore> = LazyLock::new(|| Semaphore::new(4));

/// Subdirectory holding small persistent manga cover thumbnails (`is_cover = 1`).
const COVERS_SUBDIR: &str = "covers";

/// Subdirectory holding transient chapter page images (`is_cover = 0`), aggressively purged
/// after finishing a chapter/manga or exiting the application.
const PAGES_SUBDIR: &str = "pages";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CacheBucket {
    Cover,
    Page,
}

impl CacheBucket {
    const fn other(self) -> Self {
        match self {
            Self::Cover => Self::Page,
            Self::Page => Self::Cover,
        }
    }

    const fn subdir(self) -> &'static str {
        match self {
            Self::Cover => COVERS_SUBDIR,
            Self::Page => PAGES_SUBDIR,
        }
    }
}

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

/// Manages local image files under `<dir>/covers/<xx>/<sha256>` (persistent thumbnails) and
/// `<dir>/pages/<xx>/<sha256>` (transient chapter pages) with SQLite LRU eviction.
#[derive(Debug, Clone)]
pub(crate) struct ImageCache {
    dir: PathBuf,
    db: Database,
    in_flight: Arc<DashMap<String, watch::Sender<bool>>>,
    index_mem: Arc<DashMap<String, CachedIndexEntry>>,
}

impl ImageCache {
    /// Creates a new [`ImageCache`] rooted at `dir` and hydrates the in-memory hit index.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Internal`] if the cache directories cannot be created.
    pub(crate) async fn new(dir: PathBuf, db: Database) -> Result<Self, AppError> {
        tokio::fs::create_dir_all(dir.join(COVERS_SUBDIR))
            .await
            .map_err(|e| AppError::Internal {
                message: format!("failed to create cover cache dir: {e}"),
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

        Ok(Self {
            dir,
            db,
            in_flight: Arc::new(DashMap::new()),
            index_mem,
        })
    }

    /// Returns `true` when `url` is currently indexed in the local cache.
    pub(crate) fn has_cached_url(&self, url: &str) -> bool {
        let key = Self::key_for_url(url);
        self.index_mem.contains_key(&key)
    }

    /// Spawns a bounded background task to prefetch and persistently cache a cover thumbnail or
    /// site icon into `covers/` (`is_cover = 1`) when not already cached.
    pub(crate) fn spawn_cover_prefetch(
        &self,
        http: HttpClient,
        cover_url: Url,
        referer: Url,
        site: Option<String>,
        manga_id: Option<i64>,
        max_mb: u32,
    ) {
        if self.has_cached_url(cover_url.as_str()) {
            return;
        }
        let cache = self.clone();
        tokio::spawn(async move {
            let Ok(_permit) = PREFETCH_SEM.acquire().await else {
                return;
            };
            let key = Self::key_for_url(cover_url.as_str());
            if cache.index_mem.contains_key(&key) {
                return;
            }
            let Some(_guard) = cache.begin_single_flight(&key).await else {
                return;
            };
            if cache.index_mem.contains_key(&key) {
                return;
            }
            if let Ok(fetched) = http.fetch_image(&cover_url, Some(&referer)).await
                && !fetched.bytes.is_empty()
            {
                cache
                    .put(
                        &key,
                        &fetched.bytes,
                        &fetched.content_type,
                        max_mb,
                        CacheEntryMeta {
                            site: site.as_deref(),
                            manga_id,
                            is_cover: true,
                            chapter_url: None,
                        },
                    )
                    .await;
            }
        });
    }

    /// Computes a deterministic SHA-256 hex key for an image URL.
    pub(crate) fn key_for_url(url: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(url.as_bytes());
        let digest = hasher.finalize();
        let mut out = String::with_capacity(64);
        for byte in digest {
            use std::fmt::Write;
            let _ = write!(&mut out, "{byte:02x}");
        }
        out
    }

    /// Reads a cached image if present on disk, refreshes its LRU timestamp (at most hourly in
    /// SQLite to avoid write amplification while scrolling), and backfills missing metadata.
    pub(crate) async fn get(&self, key: &str, meta: CacheEntryMeta<'_>) -> Option<CachedImage> {
        let mut entry = if let Some(hit) = self.index_mem.get(key) {
            hit.clone()
        } else {
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
            hydrated
        };

        let bucket = if entry.is_cover() || meta.is_cover {
            CacheBucket::Cover
        } else {
            CacheBucket::Page
        };
        let data = self.read_existing_file(key, bucket).await;
        let Some(data) = data else {
            self.index_mem.remove(key);
            let _ = sqlx::query("DELETE FROM image_cache WHERE key = ?")
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

    /// Writes an image to disk atomically, updates the LRU table with ownership metadata,
    /// and evicts oldest entries (pages first, covers last) if the cache exceeds `max_mb`.
    pub(crate) async fn put(
        &self,
        key: &str,
        bytes: &Bytes,
        content_type: &str,
        max_mb: u32,
        meta: CacheEntryMeta<'_>,
    ) {
        let bucket = if meta.is_cover {
            CacheBucket::Cover
        } else {
            CacheBucket::Page
        };
        let path = self.file_path(key, bucket);
        if let Some(parent) = path.parent()
            && let Err(err) = tokio::fs::create_dir_all(parent).await
        {
            warn!(%err, "failed to create image cache shard dir");
            return;
        }

        let nonce: u64 = rand::random();
        let tmp_path = path.with_extension(format!("tmp.{nonce:016x}"));
        if let Err(err) = tokio::fs::write(&tmp_path, bytes).await {
            let _ = tokio::fs::remove_file(&tmp_path).await;
            warn!(%err, "failed to write temp cache file");
            return;
        }
        if let Err(err) = tokio::fs::rename(&tmp_path, &path).await {
            let _ = tokio::fs::remove_file(&tmp_path).await;
            warn!(%err, "failed to commit temp cache file");
            return;
        }

        let size = i64::try_from(bytes.len()).unwrap_or(i64::MAX);
        let now = now_unix_secs();

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
                chapter_url = COALESCE(excluded.chapter_url, image_cache.chapter_url)",
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
                e.apply_meta(&meta, now);
            })
            .or_insert_with(|| {
                let mut e = CachedIndexEntry {
                    content_type: content_type.to_owned(),
                    flags: 0,
                    last_touch_secs: now,
                };
                e.apply_meta(&meta, now);
                e
            });

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

    /// Returns total bytes and file count currently tracked by the disk cache.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Db`] on SQLite query failure.
    pub(crate) async fn info(&self) -> Result<CacheInfo, AppError> {
        let (bytes, files): (i64, i64) =
            sqlx::query_as("SELECT COALESCE(SUM(size), 0), COUNT(*) FROM image_cache")
                .fetch_one(self.db.pool())
                .await?;

        Ok(CacheInfo {
            bytes: u64::try_from(bytes).unwrap_or(0),
            files: u64::try_from(files).unwrap_or(0),
        })
    }

    /// Returns disk cache usage grouped by source site hostname.
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

    /// Returns disk cache usage grouped by individual manga.
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
             WHERE c.manga_id IS NOT NULL
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

    /// Deletes cached image files belonging strictly to `site` without touching any library,
    /// reading history, bookmarks, or per-manga/global user settings.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Db`] on SQLite failure.
    pub(crate) async fn clear_by_source(&self, site: &str) -> Result<CacheInfo, AppError> {
        let keys: Vec<(String,)> = if site == "unknown" {
            sqlx::query_as("SELECT key FROM image_cache WHERE site IS NULL OR TRIM(site) = ''")
                .fetch_all(self.db.pool())
                .await?
        } else {
            sqlx::query_as("SELECT key FROM image_cache WHERE site = ?")
                .bind(site)
                .fetch_all(self.db.pool())
                .await?
        };

        for (key,) in &keys {
            self.index_mem.remove(key);
            self.remove_all_locations(key).await;
        }

        if site == "unknown" {
            sqlx::query("DELETE FROM image_cache WHERE site IS NULL OR TRIM(site) = ''")
                .execute(self.db.pool())
                .await?;
        } else {
            sqlx::query("DELETE FROM image_cache WHERE site = ?")
                .bind(site)
                .execute(self.db.pool())
                .await?;
        }

        if !keys.is_empty() {
            self.db.checkpoint_wal_truncate().await;
        }
        self.info().await
    }

    /// Deletes all cached image files belonging strictly to `manga_id` without touching any
    /// library, reading history, bookmarks, or per-manga/global user settings.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Db`] on SQLite failure.
    pub(crate) async fn clear_by_manga(&self, manga_id: i64) -> Result<CacheInfo, AppError> {
        let keys: Vec<(String,)> = sqlx::query_as("SELECT key FROM image_cache WHERE manga_id = ?")
            .bind(manga_id)
            .fetch_all(self.db.pool())
            .await?;

        for (key,) in &keys {
            self.index_mem.remove(key);
            self.remove_all_locations(key).await;
        }

        sqlx::query("DELETE FROM image_cache WHERE manga_id = ?")
            .bind(manga_id)
            .execute(self.db.pool())
            .await?;

        if !keys.is_empty() {
            self.db.checkpoint_wal_truncate().await;
        }
        self.info().await
    }

    /// Deletes cached chapter page images (`is_cover = 0`) for a single finished chapter
    /// while preserving cover thumbnails and per-manga reader settings.
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
            let _ = tokio::fs::remove_file(self.file_path(key, CacheBucket::Page)).await;
            let _ = tokio::fs::remove_file(self.legacy_file_path(key)).await;
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
    /// the reader or completing the manga, while preserving its cover thumbnail and settings.
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
            let _ = tokio::fs::remove_file(self.file_path(key, CacheBucket::Page)).await;
            let _ = tokio::fs::remove_file(self.legacy_file_path(key)).await;
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
                "purged manga chapter page cache while keeping cover"
            );
        }

        self.info().await
    }

    /// Aggressively purges ALL transient chapter page images (`is_cover = 0`) across the app
    /// while preserving small cover thumbnails (`is_cover = 1`) and all user/per-manga settings.
    ///
    /// Also sweeps any pre-migration flat shard directories outside `covers/` so upgrades
    /// immediately reclaim disk space, then truncates the SQLite WAL file.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Db`] or [`AppError::Internal`] if cleanup fails.
    pub(crate) async fn clear_all_chapter_pages(&self) -> Result<CacheInfo, AppError> {
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

        // Sweep legacy flat shard folders (<dir>/<hex>) left from before cover/page separation.
        if let Ok(mut entries) = tokio::fs::read_dir(&self.dir).await {
            while let Ok(Some(entry)) = entries.next_entry().await {
                let name = entry.file_name();
                let name_str = name.to_string_lossy();
                if name_str == COVERS_SUBDIR || name_str == PAGES_SUBDIR {
                    continue;
                }
                let path = entry.path();
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

    /// Deletes all cached image files on disk (both covers and pages) and clears the SQLite
    /// `image_cache` index without touching manga metadata or per-manga settings.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Db`] or [`AppError::Internal`] if cleanup fails.
    pub(crate) async fn clear(&self) -> Result<(), AppError> {
        sqlx::query("DELETE FROM image_cache")
            .execute(self.db.pool())
            .await?;
        self.index_mem.clear();

        if self.dir.exists() {
            let _ = tokio::fs::remove_dir_all(&self.dir).await;
        }
        tokio::fs::create_dir_all(self.dir.join(COVERS_SUBDIR))
            .await
            .map_err(|e| AppError::Internal {
                message: format!("failed to recreate cover cache dir: {e}"),
            })?;
        tokio::fs::create_dir_all(self.dir.join(PAGES_SUBDIR))
            .await
            .map_err(|e| AppError::Internal {
                message: format!("failed to recreate page cache dir: {e}"),
            })?;
        self.db.checkpoint_wal_truncate().await;
        Ok(())
    }

    fn file_path(&self, key: &str, bucket: CacheBucket) -> PathBuf {
        let prefix = key.get(0..2).unwrap_or("00");
        self.dir.join(bucket.subdir()).join(prefix).join(key)
    }

    fn legacy_file_path(&self, key: &str) -> PathBuf {
        let prefix = key.get(0..2).unwrap_or("00");
        self.dir.join(prefix).join(key)
    }

    async fn read_existing_file(&self, key: &str, preferred: CacheBucket) -> Option<Vec<u8>> {
        let primary = self.file_path(key, preferred);
        if let Ok(data) = tokio::fs::read(&primary).await {
            return Some(data);
        }
        let secondary = self.file_path(key, preferred.other());
        if let Ok(data) = tokio::fs::read(&secondary).await {
            return Some(data);
        }
        tokio::fs::read(self.legacy_file_path(key)).await.ok()
    }

    async fn remove_all_locations(&self, key: &str) {
        let _ = tokio::fs::remove_file(self.file_path(key, CacheBucket::Page)).await;
        let _ = tokio::fs::remove_file(self.file_path(key, CacheBucket::Cover)).await;
        let _ = tokio::fs::remove_file(self.legacy_file_path(key)).await;
    }

    /// Evicts oldest cached images until total disk usage is within `max_mb`, always evicting
    /// transient chapter pages before small cover thumbnails.
    pub(crate) async fn evict_if_needed(&self, max_mb: u32) {
        let limit_bytes = i64::from(max_mb).saturating_mul(1024 * 1024);
        let Ok((mut total,)) =
            sqlx::query_as::<_, (i64,)>("SELECT COALESCE(SUM(size), 0) FROM image_cache")
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
            let Ok(candidates) = sqlx::query_as::<_, (String, i64, i64)>(
                "SELECT key, size, is_cover FROM image_cache
                 ORDER BY is_cover ASC, last_access ASC
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

            for (key, size, is_cover) in candidates {
                if total <= target {
                    break;
                }
                let bucket = if is_cover != 0 {
                    CacheBucket::Cover
                } else {
                    CacheBucket::Page
                };
                self.index_mem.remove(&key);
                let _ = tokio::fs::remove_file(self.file_path(&key, bucket)).await;
                let _ = tokio::fs::remove_file(self.legacy_file_path(&key)).await;
                let _ = sqlx::query("DELETE FROM image_cache WHERE key = ?")
                    .bind(&key)
                    .execute(self.db.pool())
                    .await;
                total = total.saturating_sub(size);
                evicted_any = true;
                debug!(%key, is_cover, "evicted lru cached image");
            }
        }
        if evicted_any {
            self.db.checkpoint_wal_truncate().await;
        }
    }
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
            Url::parse("https://example.org/manga/alpha").unwrap(),
            SiteId::generic("example.org"),
            "Alpha".to_owned(),
            Some(Url::parse("https://cdn.example.org/alpha-cover.jpg").unwrap()),
            vec![
                ChapterRef::simple(
                    Url::parse("https://example.org/manga/alpha/ch-1").unwrap(),
                    "Ch 1".to_owned(),
                    Some(1.0),
                ),
                ChapterRef::simple(
                    Url::parse("https://example.org/manga/alpha/ch-2").unwrap(),
                    "Ch 2".to_owned(),
                    Some(2.0),
                ),
            ],
        );
        let manga_id = db.upsert_manga(&manga).await.unwrap();
        db.set_manga_reader_settings(manga_id, Some(custom_json))
            .await
            .unwrap();

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
        let db = Database::open_in_memory().await.unwrap();
        let dir = std::env::temp_dir().join(format!("rtmaga_cache_test_{}", rand::random::<u64>()));
        let cache = ImageCache::new(dir.clone(), db).await.unwrap();

        let owner_guard = cache
            .begin_single_flight("img_key_1")
            .await
            .expect("first caller owns flight");

        let cache_waiter = cache.clone();
        let waiter =
            tokio::spawn(async move { cache_waiter.begin_single_flight("img_key_1").await });

        tokio::task::yield_now().await;
        drop(owner_guard);

        let outcome = tokio::time::timeout(std::time::Duration::from_secs(2), waiter)
            .await
            .expect("waiter must not hang")
            .expect("waiter task must succeed");
        assert!(
            outcome.is_none(),
            "second caller should wait and receive None"
        );

        let _ = tokio::fs::remove_dir_all(&dir).await;
    }

    #[tokio::test]
    async fn page_purges_and_full_cache_clear_preserve_covers_and_per_manga_settings() {
        let db = Database::open_in_memory().await.unwrap();
        let dir =
            std::env::temp_dir().join(format!("rtmaga_cache_purge_test_{}", rand::random::<u64>()));
        let cache = ImageCache::new(dir.clone(), db.clone()).await.unwrap();
        let custom_json = r#"{"orientation":"horizontal"}"#;
        let (manga_id, cover_key, ch1_key, ch2_key) =
            seed_manga_and_cache(&db, &cache, custom_json).await;
        let cover_meta = CacheEntryMeta {
            is_cover: true,
            ..CacheEntryMeta::default()
        };

        // 1. Clearing finished chapter 1 deletes only ch1 pages, keeping ch2 and cover.
        let after_ch1 = cache
            .clear_chapter_pages(manga_id, "https://example.org/manga/alpha/ch-1/")
            .await
            .unwrap();
        assert_eq!(after_ch1.files, 2, "only ch1 page must be purged");
        assert!(
            cache
                .get(&ch1_key, CacheEntryMeta::default())
                .await
                .is_none(),
            "ch1 page must be gone after chapter purge"
        );
        assert!(
            cache
                .get(&ch2_key, CacheEntryMeta::default())
                .await
                .is_some(),
            "ch2 page must remain cached"
        );
        assert!(
            cache.get(&cover_key, cover_meta.clone()).await.is_some(),
            "cover thumbnail must remain cached"
        );

        // 2. Clearing manga pages deletes remaining pages while keeping cover.
        let after_manga = cache.clear_manga_pages(manga_id).await.unwrap();
        assert_eq!(after_manga.files, 1, "only cover thumbnail should remain");
        assert!(
            cache
                .get(&ch2_key, CacheEntryMeta::default())
                .await
                .is_none(),
            "ch2 page must be purged on manga exit"
        );
        assert!(
            cache.get(&cover_key, cover_meta).await.is_some(),
            "cover thumbnail must survive manga page purge"
        );

        // 3. Re-add a transient page and verify global page sweep preserves covers.
        cache
            .put(
                &ch1_key,
                &Bytes::from_static(b"TRANSIENT"),
                "image/jpeg",
                128,
                CacheEntryMeta {
                    site: Some("example.org"),
                    manga_id: Some(manga_id),
                    is_cover: false,
                    chapter_url: Some("https://example.org/manga/alpha/ch-1"),
                },
            )
            .await;
        let after_sweep = cache.clear_all_chapter_pages().await.unwrap();
        assert_eq!(
            after_sweep.files, 1,
            "global page sweep must keep cover thumbnails"
        );

        // 4. Full manual cache clear must NEVER wipe per-manga reader settings.
        cache.clear().await.unwrap();
        assert_eq!(
            cache.info().await.unwrap().files,
            0,
            "full cache clear must empty image_cache"
        );
        let reloaded = db.get_manga_by_id(manga_id).await.unwrap().unwrap();
        assert_eq!(
            reloaded.reader_settings_json.as_deref(),
            Some(custom_json),
            "per-manga reader settings must survive all cache clears"
        );

        let _ = tokio::fs::remove_dir_all(&dir).await;
    }
}
