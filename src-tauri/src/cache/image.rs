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

    /// Reads a cached image if present on disk and refreshes its LRU timestamp.
    pub(crate) async fn get(&self, key: &str) -> Option<CachedImage> {
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

        let _ = sqlx::query("UPDATE image_cache SET last_access = ? WHERE key = ?")
            .bind(now_unix_secs())
            .bind(key)
            .execute(self.db.pool())
            .await;

        Some(CachedImage {
            bytes: Bytes::from(data),
            content_type,
        })
    }

    /// Writes an image to disk atomically, updates the LRU table, and evicts oldest entries if
    /// the cache exceeds `max_mb`.
    pub(crate) async fn put(&self, key: &str, bytes: &Bytes, content_type: &str, max_mb: u32) {
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
            "INSERT INTO image_cache (key, content_type, size, last_access)
             VALUES (?, ?, ?, ?)
             ON CONFLICT(key) DO UPDATE SET
                content_type = excluded.content_type,
                size = excluded.size,
                last_access = excluded.last_access",
        )
        .bind(key)
        .bind(content_type)
        .bind(size)
        .bind(now)
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
        let Ok(candidates) = sqlx::query_as::<_, (String, i64)>(
            "SELECT key, size FROM image_cache ORDER BY last_access ASC LIMIT 200",
        )
        .fetch_all(self.db.pool())
        .await
        else {
            return;
        };

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
