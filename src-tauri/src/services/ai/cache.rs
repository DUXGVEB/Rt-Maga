//! Bounded in-memory (`DashMap`) and disk-backed (`<data_dir>/cache/ai/`) cache for page-level AI
//! bubble detection and translation results, with total hard-purge on disable or reader exit.

use std::path::{Path, PathBuf};

use dashmap::DashMap;
use sha2::{Digest, Sha256};

use crate::error::AppError;
use crate::services::ai::{AiPageMode, PageAiResultView};
use crate::settings::TranslationModelTier;

/// Maximum number of page AI results kept in RAM simultaneously.
const MAX_RAM_ENTRIES: usize = 64;

/// Summary of purged AI cache items.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct AiCachePurgeStats {
    pub ram_entries_cleared: usize,
    pub disk_files_removed: usize,
}

/// Dual-layer RAM + disk cache scoped exclusively to `<data_dir>/cache/ai/`.
///
/// Never touches downloaded model weights under `ai_models/`.
#[derive(Debug)]
pub(crate) struct AiResultCache {
    cache_dir: PathBuf,
    ram: DashMap<String, PageAiResultView>,
}

impl AiResultCache {
    /// Creates an [`AiResultCache`] rooted at `<data_dir>/cache/ai/`.
    pub(crate) fn new(data_dir: &Path) -> Self {
        Self {
            cache_dir: data_dir.join("cache").join("ai"),
            ram: DashMap::new(),
        }
    }

    /// Returns the transient inference temp directory (`<data_dir>/cache/ai/tmp/`).
    pub(crate) fn temp_dir(&self) -> PathBuf {
        self.cache_dir.join("tmp")
    }

    /// Computes a stable, filesystem-safe SHA-256 cache key for a page request.
    pub(crate) fn cache_key(
        page_key: &str,
        tier: TranslationModelTier,
        target_lang: &str,
        mode: AiPageMode,
    ) -> String {
        let mut hasher = Sha256::new();
        hasher.update(page_key.as_bytes());
        hasher.update(b"|");
        hasher.update(match tier {
            TranslationModelTier::Lite => b"lite".as_slice(),
            TranslationModelTier::Qwen3Vl2B => b"qwen3_vl_2b".as_slice(),
        });
        hasher.update(b"|");
        hasher.update(target_lang.trim().to_ascii_lowercase().as_bytes());
        hasher.update(b"|");
        hasher.update(if mode.is_detect_only() { b"1" } else { b"0" });
        hex_lower(&hasher.finalize())
    }

    /// Looks up a cached page result in RAM first, then disk (`<data_dir>/cache/ai/<key>.json`).
    pub(crate) async fn get(&self, key: &str) -> Option<PageAiResultView> {
        if let Some(hit) = self.ram.get(key) {
            let mut result = hit.value().clone();
            result.from_cache = true;
            return Some(result);
        }

        let path = self.entry_path(key);
        let bytes = tokio::fs::read(&path).await.ok()?;
        let mut parsed: PageAiResultView = serde_json::from_slice(&bytes).ok()?;
        parsed.from_cache = true;
        self.insert_ram_bounded(key.to_owned(), parsed.clone());
        Some(parsed)
    }

    /// Stores a completed, non-cancelled page result into RAM and disk atomically.
    pub(crate) async fn put(&self, key: &str, result: &PageAiResultView) {
        if result.cancelled {
            return;
        }
        self.insert_ram_bounded(key.to_owned(), result.clone());

        if tokio::fs::create_dir_all(&self.cache_dir).await.is_err() {
            return;
        }
        let Ok(json) = serde_json::to_vec(result) else {
            return;
        };
        let dest = self.entry_path(key);
        let tmp = self.cache_dir.join(format!(".{key}.tmp"));
        if tokio::fs::write(&tmp, &json).await.is_ok() {
            if tokio::fs::rename(&tmp, &dest).await.is_err() {
                let _ = tokio::fs::remove_file(&dest).await;
                if tokio::fs::rename(&tmp, &dest).await.is_err() {
                    let _ = tokio::fs::remove_file(&tmp).await;
                }
            }
        } else {
            let _ = tokio::fs::remove_file(&tmp).await;
        }
    }

    /// Drops every in-memory cached translation/bubble map, shrinks backing memory, and removes
    /// ALL AI disk cache files & temp inference files under `<data_dir>/cache/ai/`.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Internal`] if removing `<data_dir>/cache/ai/` fails unexpectedly.
    pub(crate) async fn purge_all(&self) -> Result<AiCachePurgeStats, AppError> {
        let ram_entries_cleared = self.ram.len();
        self.ram.clear();
        self.ram.shrink_to_fit();

        let disk_files_removed = remove_dir_contents_recursive(&self.cache_dir).await?;
        Ok(AiCachePurgeStats {
            ram_entries_cleared,
            disk_files_removed,
        })
    }

    fn insert_ram_bounded(&self, key: String, value: PageAiResultView) {
        if self.ram.len() >= MAX_RAM_ENTRIES
            && let Some(first_key) = self.ram.iter().next().map(|entry| entry.key().clone())
        {
            let _ = self.ram.remove(&first_key);
        }
        self.ram.insert(key, value);
    }

    fn entry_path(&self, key: &str) -> PathBuf {
        self.cache_dir.join(format!("{key}.json"))
    }
}

async fn remove_dir_contents_recursive(dir: &Path) -> Result<usize, AppError> {
    if !tokio::fs::try_exists(dir).await.unwrap_or(false) {
        return Ok(0);
    }

    let dir_owned = dir.to_path_buf();
    tokio::task::spawn_blocking(move || remove_dir_contents_sync(&dir_owned))
        .await
        .map_err(|e| AppError::Internal {
            message: format!("AI cache purge worker failed: {e}"),
        })?
}

fn remove_dir_contents_sync(dir: &Path) -> Result<usize, AppError> {
    let mut removed = 0_usize;
    let entries = match std::fs::read_dir(dir) {
        Ok(rd) => rd,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(0),
        Err(err) => {
            return Err(AppError::Internal {
                message: format!("failed to read AI cache dir '{}': {err}", dir.display()),
            });
        }
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            removed = removed.saturating_add(remove_dir_contents_sync(&path)?);
            let _ = std::fs::remove_dir(&path);
        } else if std::fs::remove_file(&path).is_ok() {
            removed = removed.saturating_add(1);
        }
    }
    Ok(removed)
}

pub(crate) fn hex_lower(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        out.push(char::from(HEX[usize::from(b >> 4)]));
        out.push(char::from(HEX[usize::from(b & 0x0f)]));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::ai::TranslatedRegionView;

    fn sample_result(page_key: &str) -> PageAiResultView {
        PageAiResultView {
            page_key: page_key.to_owned(),
            tier_used: TranslationModelTier::Lite,
            target_lang: "en".to_owned(),
            detect_only: false,
            from_cache: false,
            cancelled: false,
            elapsed_ms: 42,
            image_width: 800,
            image_height: 1200,
            regions: vec![TranslatedRegionView {
                text: "Hello!".to_owned(),
                source_text: Some("こんにちは".to_owned()),
                bbox: [100, 100, 300, 220],
                confidence: 0.95,
            }],
            missing_nmt_packs: Vec::new(),
            error: None,
        }
    }

    #[tokio::test]
    async fn cache_round_trips_and_hard_purge_preserves_model_weights() {
        let unique = format!(
            "rtmaga_ai_cache_test_{}_{}",
            std::process::id(),
            crate::repo::now_unix_secs()
        );
        let root = std::env::temp_dir().join(unique);
        let models_dir = root.join("ai_models").join("tier2");
        std::fs::create_dir_all(&models_dir).expect("create sibling models dir");
        let sentinel_weight = models_dir.join("sentinel.gguf");
        std::fs::write(&sentinel_weight, b"keep-me").expect("write weight sentinel");

        let cache = AiResultCache::new(&root);
        let key = AiResultCache::cache_key(
            "page-1",
            TranslationModelTier::Lite,
            "en",
            AiPageMode::Translate,
        );
        cache.put(&key, &sample_result("page-1")).await;

        let hit = cache.get(&key).await.expect("RAM hit must succeed");
        assert!(hit.from_cache, "cached response must flag from_cache");
        assert_eq!(hit.regions.len(), 1, "regions preserved");

        // Fresh cache instance reading from disk only.
        let disk_only = AiResultCache::new(&root);
        let disk_hit = disk_only
            .get(&key)
            .await
            .expect("disk cache hit must hydrate RAM");
        assert!(disk_hit.from_cache, "disk hit flags from_cache");

        // Also place a temp inference image under cache/ai/tmp/.
        let tmp_dir = cache.temp_dir();
        std::fs::create_dir_all(&tmp_dir).expect("create tmp dir");
        let tmp_img = tmp_dir.join("job.jpg");
        std::fs::write(&tmp_img, b"jpg").expect("write temp image");

        let stats = cache.purge_all().await.expect("purge_all succeeds");
        assert_eq!(stats.ram_entries_cleared, 1, "RAM cleared");
        assert!(
            stats.disk_files_removed >= 2,
            "both json entry and temp jpg removed"
        );
        assert!(
            cache.get(&key).await.is_none(),
            "cache must be empty after purge_all"
        );
        assert!(
            sentinel_weight.is_file(),
            "hard purge must NEVER touch downloaded ai_models weights"
        );

        let _ = std::fs::remove_dir_all(&root);
    }
}
