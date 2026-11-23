//! Disk-backed LRU image cache indexed in SQLite with atomic writes and single-flight dedup.

use std::path::PathBuf;
use std::sync::Arc;

use bytes::Bytes;
use dashmap::DashMap;
use dashmap::mapref::entry::Entry;
use serde::Serialize;
use sha2::{Digest, Sha256};
use tokio::sync::watch;
use tracing::{debug, warn};

use crate::error::AppError;
use crate::repo::{Database, now_unix_secs};

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
}

/// A cached image loaded from disk.
#[derive(Debug, Clone)]
pub(crate) struct CachedImage {
    /// Raw image bytes.
    pub bytes: Bytes,
    /// MIME content type.
    pub content_type: String,
}

/// Manages local image files under `<dir>/<xx>/<sha256>` with SQLite LRU eviction.
#[derive(Debug, Clone)]
pub(crate) struct ImageCache {
    dir: PathBuf,
    db: Database,
    in_flight: Arc<DashMap<String, watch::Sender<bool>>>,
}

impl ImageCache {
    /// Creates a new [`ImageCache`] rooted at `dir`.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Internal`] if the cache directory cannot be created.
    pub(crate) async fn new(dir: PathBuf, db: Database) -> Result<Self, AppError> {
        tokio::fs::create_dir_all(&dir)
            .await
            .map_err(|e| AppError::Internal {
                message: format!("failed to create image cache dir: {e}"),
            })?;
        Ok(Self {
            dir,
            db,
            in_flight: Arc::new(DashMap::new()),
        })
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

    /// Reads a cached image if present on disk, refreshes its LRU timestamp, and backfills
    /// missing `site` / `manga_id` ownership metadata.
    pub(crate) async fn get(&self, key: &str, meta: CacheEntryMeta<'_>) -> Option<CachedImage> {
        let row: Option<(String,)> =
            sqlx::query_as("SELECT content_type FROM image_cache WHERE key = ?")
                .bind(key)
                .fetch_optional(self.db.pool())
                .await
                .ok()?;

        let (content_type,) = row?;
        let path = self.file_path(key);
        let Ok(data) = tokio::fs::read(&path).await else {
            let _ = sqlx::query("DELETE FROM image_cache WHERE key = ?")
                .bind(key)
                .execute(self.db.pool())
                .await;
            return None;
        };

        let _ = sqlx::query(
            "UPDATE image_cache
             SET last_access = ?,
                 site = COALESCE(site, ?),
                 manga_id = COALESCE(manga_id, ?)
             WHERE key = ?",
        )
        .bind(now_unix_secs())
        .bind(meta.site)
        .bind(meta.manga_id)
        .bind(key)
        .execute(self.db.pool())
        .await;

        Some(CachedImage {
            bytes: Bytes::from(data),
            content_type,
        })
    }

    /// Writes an image to disk atomically, updates the LRU table with optional source/manga
    /// ownership metadata, and evicts oldest entries if the cache exceeds `max_mb`.
    pub(crate) async fn put(
        &self,
        key: &str,
        bytes: &Bytes,
        content_type: &str,
        max_mb: u32,
        meta: CacheEntryMeta<'_>,
    ) {
        let path = self.file_path(key);
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
            "INSERT INTO image_cache (key, content_type, size, last_access, site, manga_id)
             VALUES (?, ?, ?, ?, ?, ?)
             ON CONFLICT(key) DO UPDATE SET
                content_type = excluded.content_type,
                size = excluded.size,
                last_access = excluded.last_access,
                site = COALESCE(excluded.site, image_cache.site),
                manga_id = COALESCE(excluded.manga_id, image_cache.manga_id)",
        )
        .bind(key)
        .bind(content_type)
        .bind(size)
        .bind(now)
        .bind(meta.site)
        .bind(meta.manga_id)
        .execute(self.db.pool())
        .await;

        self.evict_if_needed(max_mb).await;
    }

    /// Coordinates concurrent requests for the same key so only one network fetch executes.
    ///
    /// Uses an atomic `DashMap::entry` paired with a latched `watch` channel so waiters never
    /// miss a wakeup even if the fetch completes before `.await`.
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
    /// reading history, bookmarks, or user settings.
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
            let _ = tokio::fs::remove_file(self.file_path(key)).await;
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

        self.info().await
    }

    /// Deletes cached image files belonging strictly to `manga_id` without touching any library,
    /// reading history, bookmarks, or user settings.
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
            let _ = tokio::fs::remove_file(self.file_path(key)).await;
        }

        sqlx::query("DELETE FROM image_cache WHERE manga_id = ?")
            .bind(manga_id)
            .execute(self.db.pool())
            .await?;

        self.info().await
    }

    /// Deletes all cached image files on disk and clears the SQLite index.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Db`] or [`AppError::Internal`] if cleanup fails.
    pub(crate) async fn clear(&self) -> Result<(), AppError> {
        sqlx::query("DELETE FROM image_cache")
            .execute(self.db.pool())
            .await?;

        if self.dir.exists() {
            let _ = tokio::fs::remove_dir_all(&self.dir).await;
        }
        tokio::fs::create_dir_all(&self.dir)
            .await
            .map_err(|e| AppError::Internal {
                message: format!("failed to recreate image cache dir: {e}"),
            })?;
        Ok(())
    }

    fn file_path(&self, key: &str) -> PathBuf {
        let prefix = key.get(0..2).unwrap_or("00");
        self.dir.join(prefix).join(key)
    }

    /// Evicts oldest cached images until total disk usage is within `max_mb`.
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
        for _ in 0..50 {
            if total <= target {
                break;
            }
            let Ok(candidates) = sqlx::query_as::<_, (String, i64)>(
                "SELECT key, size FROM image_cache ORDER BY last_access ASC LIMIT 200",
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
                let path = self.file_path(&key);
                let _ = tokio::fs::remove_file(&path).await;
                let _ = sqlx::query("DELETE FROM image_cache WHERE key = ?")
                    .bind(&key)
                    .execute(self.db.pool())
                    .await;
                total = total.saturating_sub(size);
                debug!(%key, "evicted lru cached image");
            }
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
    use super::*;

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
}
