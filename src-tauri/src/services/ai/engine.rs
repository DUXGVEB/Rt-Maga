//! Unified 2-Tier on-device AI inference orchestrator with hot-switching, generation-based
//! cancellation, and immediate hard-unload + RAM/disk cache purge.

use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::time::Instant;

use sha2::{Digest, Sha256};
use tauri::ipc::Channel;
use tauri::{AppHandle, Emitter};
use tokio::sync::{OwnedSemaphorePermit, RwLock, Semaphore, watch};
use tracing::debug;

use crate::android_env::AiRuntimeCapabilities;
use crate::error::AppError;
use crate::services::ai::cache::{AiResultCache, hex_lower};
use crate::services::ai::downloader::{
    delete_tier2_assets, download_tier2_bundle, installed_tier2_bytes, resolve_storage_root,
};
#[cfg(not(target_os = "android"))]
use crate::services::ai::llama_tier2::GpuOffloadMode;
use crate::services::ai::llama_tier2::Tier2LlamaBackend;
use crate::services::ai::manifest::{
    SidecarTarget, TIER1_BUDGET_BYTES, tier1_ocr_assets, tier2_bundle_assets, total_bytes,
};
use crate::services::ai::onnx_tier1::Tier1OnnxBackend;
use crate::services::ai::preprocess::{PreprocessedPage, preprocess_image_bytes};
use crate::services::ai::{
    AiDownloadProgressEvent, AiModelEntryView, AiModelStatus, AiPageMode, AiStatusView,
    PageAiResultView, TranslatedRegionView,
};
use crate::settings::TranslationModelTier;
use crate::state::AppState;

/// Request scope passed to an [`InferenceBackend`] implementation.
#[derive(Debug)]
pub(crate) struct InferenceRequest<'a> {
    pub app: Option<&'a AppHandle>,
    pub storage_root: &'a Path,
    pub temp_dir: &'a Path,
    pub page: &'a PreprocessedPage,
    pub target_lang: &'a str,
    pub mode: AiPageMode,
    #[cfg(not(target_os = "android"))]
    pub offload: GpuOffloadMode,
}

/// Output produced by an [`InferenceBackend`] for a single preprocessed page.
#[derive(Debug)]
pub(crate) enum BackendOutput {
    /// Inference completed with zero or more normalized regions.
    Completed(Vec<TranslatedRegionView>),
    /// Inference was aborted early because a newer generation arrived.
    Cancelled,
}

/// Object-safe async backend trait allowing seamless hot-switching between Tier 1 (`Lite`) and
/// Tier 2 (`Qwen3Vl2B`) without restarting the application.
pub(crate) trait InferenceBackend: Send + Sync {
    /// Identifies which [`TranslationModelTier`] this backend serves.
    fn tier(&self) -> TranslationModelTier;

    /// Returns `true` while native runtime sessions or sidecar processes are loaded in memory.
    fn is_loaded(&self) -> bool;

    /// Runs page inference on an already downscaled (`<= 1024px`) manga page.
    fn infer_page<'a>(
        &'a self,
        req: &'a InferenceRequest<'a>,
        cancel_rx: watch::Receiver<u64>,
        expected_gen: u64,
    ) -> Pin<Box<dyn Future<Output = Result<BackendOutput, AppError>> + Send + 'a>>;

    /// Cancels any active inference work immediately.
    fn cancel_active<'a>(
        &'a self,
        app: Option<&'a AppHandle>,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>>;

    /// Hard-unloads native sessions / kills sidecar workers to reclaim RAM/VRAM immediately.
    fn hard_unload<'a>(
        &'a self,
        app: Option<&'a AppHandle>,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>>;
}

impl InferenceBackend for Tier1OnnxBackend {
    fn tier(&self) -> TranslationModelTier {
        TranslationModelTier::Lite
    }

    fn is_loaded(&self) -> bool {
        self.is_loaded()
    }

    fn infer_page<'a>(
        &'a self,
        req: &'a InferenceRequest<'a>,
        cancel_rx: watch::Receiver<u64>,
        expected_gen: u64,
    ) -> Pin<Box<dyn Future<Output = Result<BackendOutput, AppError>> + Send + 'a>> {
        Box::pin(async move {
            match self.infer_page(req, cancel_rx, expected_gen).await? {
                Some(regions) => Ok(BackendOutput::Completed(regions)),
                None => Ok(BackendOutput::Cancelled),
            }
        })
    }

    fn cancel_active<'a>(
        &'a self,
        app: Option<&'a AppHandle>,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
        self.cancel_active(app);
        Box::pin(async {})
    }

    fn hard_unload<'a>(
        &'a self,
        app: Option<&'a AppHandle>,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
        Box::pin(async move {
            self.hard_unload(app).await;
        })
    }
}

impl InferenceBackend for Tier2LlamaBackend {
    fn tier(&self) -> TranslationModelTier {
        TranslationModelTier::Qwen3Vl2B
    }

    fn is_loaded(&self) -> bool {
        self.is_loaded()
    }

    fn infer_page<'a>(
        &'a self,
        req: &'a InferenceRequest<'a>,
        cancel_rx: watch::Receiver<u64>,
        expected_gen: u64,
    ) -> Pin<Box<dyn Future<Output = Result<BackendOutput, AppError>> + Send + 'a>> {
        Box::pin(async move {
            match self.infer_page(req, cancel_rx, expected_gen).await? {
                Some(regions) => Ok(BackendOutput::Completed(regions)),
                None => Ok(BackendOutput::Cancelled),
            }
        })
    }

    fn cancel_active<'a>(
        &'a self,
        app: Option<&'a AppHandle>,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
        Box::pin(async move {
            self.cancel_active(app).await;
        })
    }

    fn hard_unload<'a>(
        &'a self,
        app: Option<&'a AppHandle>,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
        Box::pin(async move {
            self.hard_unload(app).await;
        })
    }
}

/// Central on-device AI subsystem owned by [`AppState`].
pub(crate) struct AiEngine {
    data_dir: PathBuf,
    active_tier: RwLock<TranslationModelTier>,
    tier1: Arc<dyn InferenceBackend>,
    tier2: Arc<dyn InferenceBackend>,
    cache: AiResultCache,
    job_generation: AtomicU64,
    job_cancel_tx: watch::Sender<u64>,
    active_jobs: AtomicU32,
    download_running: AtomicBool,
    download_cancel_tx: watch::Sender<bool>,
    inference_gate: Arc<Semaphore>,
}

impl std::fmt::Debug for AiEngine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AiEngine")
            .field("data_dir", &self.data_dir)
            .field(
                "job_generation",
                &self.job_generation.load(Ordering::Relaxed),
            )
            .field("active_jobs", &self.active_jobs.load(Ordering::Relaxed))
            .field(
                "download_running",
                &self.download_running.load(Ordering::Relaxed),
            )
            .finish_non_exhaustive()
    }
}

impl AiEngine {
    /// Constructs an idle [`AiEngine`] with zero models loaded in RAM.
    pub(crate) fn new(data_dir: &Path, initial_tier: TranslationModelTier) -> Self {
        let (job_cancel_tx, _job_rx) = watch::channel(0_u64);
        let (download_cancel_tx, _dl_rx) = watch::channel(false);
        Self {
            data_dir: data_dir.to_path_buf(),
            active_tier: RwLock::new(initial_tier),
            tier1: Arc::new(Tier1OnnxBackend::new()),
            tier2: Arc::new(Tier2LlamaBackend::new()),
            cache: AiResultCache::new(data_dir),
            job_generation: AtomicU64::new(0),
            job_cancel_tx,
            active_jobs: AtomicU32::new(0),
            download_running: AtomicBool::new(false),
            download_cancel_tx,
            inference_gate: Arc::new(Semaphore::new(1)),
        }
    }

    fn backend_for(&self, tier: TranslationModelTier) -> &dyn InferenceBackend {
        match tier {
            TranslationModelTier::Lite => self.tier1.as_ref(),
            TranslationModelTier::Qwen3Vl2B => self.tier2.as_ref(),
        }
    }

    /// Resolves the storage root for AI model weights.
    pub(crate) fn storage_root(&self, app: Option<&AppHandle>) -> PathBuf {
        resolve_storage_root(&self.data_dir, app)
    }

    /// Builds a live [`AiStatusView`] snapshot for Settings -> Accessibility and the Reader AI pill.
    pub(crate) async fn status(&self, app: Option<&AppHandle>) -> AiStatusView {
        let storage_root = self.storage_root(app);
        let caps = app.map_or_else(
            AiRuntimeCapabilities::default,
            crate::android_env::get_runtime_capabilities,
        );
        let tier2_installed = Tier2LlamaBackend::is_installed(&storage_root);
        let downloading = self.download_running.load(Ordering::Acquire);

        let mut active_tier = *self.active_tier.read().await;
        if active_tier == TranslationModelTier::Qwen3Vl2B && !tier2_installed {
            active_tier = TranslationModelTier::Lite;
        }

        let tier1_req = total_bytes(&tier1_ocr_assets()).min(TIER1_BUDGET_BYTES);
        let tier1_installed_bytes = Tier1OnnxBackend::installed_bytes(&storage_root, app);
        let sidecar_target = select_sidecar_target(&caps);
        let tier2_req = total_bytes(&tier2_bundle_assets(sidecar_target));
        let tier2_installed_bytes = installed_tier2_bytes(&storage_root);

        let tier2_status = if downloading {
            AiModelStatus::Downloading
        } else if tier2_installed {
            AiModelStatus::Ready
        } else {
            AiModelStatus::NotDownloaded
        };

        let tier1_accel = if cfg!(target_os = "android") {
            "XNNPACK / NNAPI".to_owned()
        } else if cfg!(target_os = "windows") {
            "DirectML / CPU".to_owned()
        } else {
            "CPU".to_owned()
        };

        let tier2_accel = if caps.vulkan_supported {
            "Vulkan (FP16 / Q8_0 KV)".to_owned()
        } else {
            "CPU (Q8_0 KV)".to_owned()
        };

        let models = vec![
            AiModelEntryView {
                tier: self.tier1.tier(),
                id: "lite_ocr_nmt".to_owned(),
                display_name: "Lite (Built-in)".to_owned(),
                status: AiModelStatus::Ready,
                installed_bytes: tier1_installed_bytes,
                required_bytes: tier1_req,
                ready: true,
                bundled: true,
                accelerator: tier1_accel,
            },
            AiModelEntryView {
                tier: self.tier2.tier(),
                id: "qwen3_vl_2b".to_owned(),
                display_name: "Qwen3-VL-2B (High Accuracy)".to_owned(),
                status: tier2_status,
                installed_bytes: tier2_installed_bytes,
                required_bytes: tier2_req,
                ready: tier2_installed,
                bundled: false,
                accelerator: tier2_accel,
            },
        ];

        AiStatusView {
            active_tier,
            platform: current_platform().to_owned(),
            storage_dir: storage_root.to_string_lossy().into_owned(),
            tier1_ready: true,
            tier2_ready: tier2_installed,
            downloading,
            vulkan_supported: caps.vulkan_supported,
            loaded_in_memory: self.tier1.is_loaded() || self.tier2.is_loaded(),
            active_jobs: self.active_jobs.load(Ordering::Relaxed),
            models,
        }
    }

    /// Synchronizes the active tier from persisted settings without failing when weights are absent.
    pub(crate) async fn sync_active_tier(&self, tier: TranslationModelTier) {
        *self.active_tier.write().await = tier;
    }

    /// Hot-switches the active AI backend tier without restarting the app, hard-unloading the
    /// previously active tier to keep RAM minimal.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::NotFound`] if `Qwen3Vl2B` is selected before downloading its weights.
    pub(crate) async fn set_tier(
        &self,
        app: Option<&AppHandle>,
        tier: TranslationModelTier,
    ) -> Result<AiStatusView, AppError> {
        let storage_root = self.storage_root(app);
        if tier == TranslationModelTier::Qwen3Vl2B
            && !Tier2LlamaBackend::is_installed(&storage_root)
        {
            return Err(AppError::NotFound {
                message: "Qwen3-VL-2B weights are not downloaded yet".to_owned(),
            });
        }

        let prev = {
            let mut guard = self.active_tier.write().await;
            let old = *guard;
            *guard = tier;
            old
        };

        if prev != tier {
            let _ = self.cancel_jobs(app).await;
            self.backend_for(prev).hard_unload(app).await;
        }

        Ok(self.status(app).await)
    }

    /// Downloads the verified Tier 2 `Qwen3-VL-2B` bundle (and Windows `llama.cpp` sidecar on
    /// Windows), streaming progress over `on_progress` and `"ai://download-progress"`.
    ///
    /// Auto-registers and hot-switches active tier to `Qwen3Vl2B` upon completion.
    ///
    /// # Errors
    ///
    /// Returns [`AppError`] if a download is already running, cancelled, or fails verification.
    pub(crate) async fn download_tier2(
        &self,
        app: &AppHandle,
        on_progress: Channel<AiDownloadProgressEvent>,
    ) -> Result<AiStatusView, AppError> {
        if self
            .download_running
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return Err(AppError::Internal {
                message: "AI model download is already in progress".to_owned(),
            });
        }

        let _ = self.download_cancel_tx.send_replace(false);
        let cancel_rx = self.download_cancel_tx.subscribe();
        let storage_root = self.storage_root(Some(app));
        let caps = crate::android_env::get_runtime_capabilities(app);
        let sidecar = select_sidecar_target(&caps);

        let app_clone = app.clone();
        let download_res = download_tier2_bundle(&storage_root, sidecar, cancel_rx, move |ev| {
            let _ = on_progress.send(ev.clone());
            let _ = app_clone.emit("ai://download-progress", &ev);
        })
        .await;

        self.download_running.store(false, Ordering::Release);
        download_res?;

        // Auto-register and hot-switch immediately without restart.
        *self.active_tier.write().await = TranslationModelTier::Qwen3Vl2B;
        self.tier1.hard_unload(Some(app)).await;
        Ok(self.status(Some(app)).await)
    }

    /// Cancels any active Tier 2 model download immediately and triggers `.part` rollback.
    pub(crate) async fn cancel_download(&self, app: Option<&AppHandle>) -> AiStatusView {
        let _ = self.download_cancel_tx.send_replace(true);
        self.status(app).await
    }

    /// Cancels downloads/jobs, hard-unloads Tier 2, removes Tier 2 files from disk, and switches
    /// active tier back to `Lite`.
    ///
    /// # Errors
    ///
    /// Returns [`AppError`] if removing downloaded Tier 2 directories fails.
    pub(crate) async fn delete_tier2(
        &self,
        app: Option<&AppHandle>,
    ) -> Result<AiStatusView, AppError> {
        let _ = self.download_cancel_tx.send_replace(true);
        let _ = self.cancel_jobs(app).await;
        self.tier2.hard_unload(app).await;

        let storage_root = self.storage_root(app);
        delete_tier2_assets(&storage_root).await?;
        *self.active_tier.write().await = TranslationModelTier::Lite;
        Ok(self.status(app).await)
    }

    /// Increments the monotonic generation counter and cancels all queued / in-flight inference
    /// jobs immediately (used on fast page swipe or user cancellation).
    pub(crate) async fn cancel_jobs(&self, app: Option<&AppHandle>) -> u64 {
        let next = self
            .job_generation
            .fetch_add(1, Ordering::AcqRel)
            .saturating_add(1);
        let _ = self.job_cancel_tx.send_replace(next);
        self.tier1.cancel_active(app).await;
        self.tier2.cancel_active(app).await;
        if let Some(handle) = app {
            crate::android_env::cancel_active_ai_jobs(handle);
        }
        next
    }

    /// Immediately cancels all active jobs, hard-unloads ALL native ONNX sessions / Windows
    /// `llama.cpp` sidecars / Android JNI contexts, clears + shrinks the in-memory `DashMap`
    /// cache, and deletes ALL AI disk cache files under `<data_dir>/cache/ai/`.
    ///
    /// Never touches downloaded Tier 2 model weights under `ai_models/`.
    ///
    /// # Errors
    ///
    /// Returns [`AppError`] if removing AI disk cache files fails.
    pub(crate) async fn hard_unload_and_purge(
        &self,
        app: Option<&AppHandle>,
    ) -> Result<AiStatusView, AppError> {
        let _ = self.cancel_jobs(app).await;
        self.tier1.hard_unload(app).await;
        self.tier2.hard_unload(app).await;
        if let Some(handle) = app {
            let _ = crate::android_env::hard_unload_all_ai(handle);
        }
        let _stats = self.cache.purge_all().await?;
        Ok(self.status(app).await)
    }

    /// Runs speech-bubble detection (`AiPageMode::DetectOnly`) or full on-device manga page
    /// translation (`AiPageMode::Translate`) for a signed `img://` `page_token`.
    ///
    /// # Errors
    ///
    /// Returns [`AppError`] if resolving or decoding the page image fails, or the active backend
    /// returns an unrecoverable inference error.
    pub(crate) async fn translate_page(
        &self,
        app: Option<&AppHandle>,
        state: &AppState,
        page_token: &str,
        target_lang: &str,
        mode: AiPageMode,
        client_generation: Option<u64>,
    ) -> Result<PageAiResultView, AppError> {
        let started = Instant::now();
        let normalized_lang = normalize_target_lang(target_lang);
        let page_key = derive_page_key(page_token);
        let storage_root = self.storage_root(app);

        let selected_tier = self.resolve_effective_tier(&storage_root, mode).await;
        let expected_gen = self.capture_request_generation(client_generation);

        let cache_key = AiResultCache::cache_key(&page_key, selected_tier, &normalized_lang, mode);
        if let Some(hit) = self.cache.get(&cache_key).await {
            return Ok(hit);
        }

        let (raw_bytes, _mime, _verified) =
            crate::image_proxy::resolve_image_bytes(state, page_token).await?;

        self.infer_raw_bytes(
            app,
            page_key,
            raw_bytes,
            normalized_lang,
            mode,
            expected_gen,
            started,
        )
        .await
    }

    /// Captures the active server-side generation when a translation request begins.
    ///
    /// A stale `client_generation` (for example after an OFF -> ON toggle invokes
    /// `hard_unload_and_purge()`, bumping `job_generation`) must not reject a fresh request.
    /// In-flight cancellation still triggers whenever `self.job_generation` advances *after*
    /// this generation snapshot is captured.
    fn capture_request_generation(&self, _client_generation: Option<u64>) -> u64 {
        self.job_generation.load(Ordering::Acquire)
    }

    /// Core inference path operating directly on raw image bytes (also exercised by unit tests).
    #[expect(
        clippy::too_many_arguments,
        reason = "internal page inference pipeline carrying cache key, bytes, target language, mode, and generation token"
    )]
    pub(crate) async fn infer_raw_bytes(
        &self,
        app: Option<&AppHandle>,
        page_key: String,
        raw_bytes: Vec<u8>,
        target_lang: String,
        mode: AiPageMode,
        expected_gen: u64,
        started: Instant,
    ) -> Result<PageAiResultView, AppError> {
        let storage_root = self.storage_root(app);
        let effective_tier = self.resolve_effective_tier(&storage_root, mode).await;
        let cache_key = AiResultCache::cache_key(&page_key, effective_tier, &target_lang, mode);

        if let Some(hit) = self.cache.get(&cache_key).await {
            return Ok(hit);
        }
        if self.job_generation.load(Ordering::Acquire) != expected_gen {
            return Ok(cancelled_result(
                page_key,
                effective_tier,
                target_lang,
                mode,
                started,
            ));
        }

        let preprocessed = tokio::task::spawn_blocking(move || preprocess_image_bytes(&raw_bytes))
            .await
            .map_err(|e| AppError::Internal {
                message: format!("AI image preprocessing task failed: {e}"),
            })??;

        let Some(permit) = self.acquire_inference_permit(expected_gen).await? else {
            return Ok(cancelled_result(
                page_key,
                effective_tier,
                target_lang,
                mode,
                started,
            ));
        };

        self.active_jobs.fetch_add(1, Ordering::AcqRel);
        let _active_guard = ActiveJobGuard(&self.active_jobs);

        #[cfg(not(target_os = "android"))]
        let offload = {
            let caps = app.map_or_else(
                AiRuntimeCapabilities::default,
                crate::android_env::get_runtime_capabilities,
            );
            if caps.vulkan_supported {
                GpuOffloadMode::VulkanFull
            } else {
                GpuOffloadMode::CpuFallback
            }
        };
        let temp_dir = self.cache.temp_dir();
        let req = InferenceRequest {
            app,
            storage_root: &storage_root,
            temp_dir: &temp_dir,
            page: &preprocessed,
            target_lang: &target_lang,
            mode,
            #[cfg(not(target_os = "android"))]
            offload,
        };

        let backend = self.backend_for(effective_tier);
        debug!(tier = ?backend.tier(), ?mode, "dispatching page AI inference");
        let backend_out = backend
            .infer_page(&req, self.job_cancel_tx.subscribe(), expected_gen)
            .await;

        drop(permit);

        match backend_out? {
            BackendOutput::Cancelled => Ok(cancelled_result(
                page_key,
                effective_tier,
                target_lang,
                mode,
                started,
            )),
            BackendOutput::Completed(regions) => {
                let elapsed_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
                let result = PageAiResultView {
                    page_key,
                    tier_used: effective_tier,
                    target_lang,
                    detect_only: mode.is_detect_only(),
                    from_cache: false,
                    cancelled: false,
                    elapsed_ms,
                    image_width: preprocessed.original_width,
                    image_height: preprocessed.original_height,
                    regions,
                };
                self.cache.put(&cache_key, &result).await;
                Ok(result)
            }
        }
    }

    async fn acquire_inference_permit(
        &self,
        expected_gen: u64,
    ) -> Result<Option<OwnedSemaphorePermit>, AppError> {
        let mut cancel_rx = self.job_cancel_tx.subscribe();
        let permit = tokio::select! {
            acq = Arc::clone(&self.inference_gate).acquire_owned() => {
                acq.map_err(|_| AppError::Internal {
                    message: "AI inference semaphore closed".to_owned(),
                })?
            }
            () = wait_until_generation_changes(&mut cancel_rx, expected_gen) => {
                return Ok(None);
            }
        };
        if self.job_generation.load(Ordering::Acquire) != expected_gen {
            drop(permit);
            return Ok(None);
        }
        Ok(Some(permit))
    }

    async fn resolve_effective_tier(
        &self,
        storage_root: &Path,
        mode: AiPageMode,
    ) -> TranslationModelTier {
        if mode.is_detect_only() {
            return TranslationModelTier::Lite;
        }
        let requested = *self.active_tier.read().await;
        if requested == TranslationModelTier::Qwen3Vl2B
            && Tier2LlamaBackend::is_installed(storage_root)
        {
            TranslationModelTier::Qwen3Vl2B
        } else {
            TranslationModelTier::Lite
        }
    }
}

struct ActiveJobGuard<'a>(&'a AtomicU32);

impl Drop for ActiveJobGuard<'_> {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
    }
}

async fn wait_until_generation_changes(cancel_rx: &mut watch::Receiver<u64>, expected_gen: u64) {
    loop {
        if *cancel_rx.borrow() != expected_gen {
            return;
        }
        if cancel_rx.changed().await.is_err() {
            return;
        }
    }
}

fn cancelled_result(
    page_key: String,
    tier_used: TranslationModelTier,
    target_lang: String,
    mode: AiPageMode,
    started: Instant,
) -> PageAiResultView {
    PageAiResultView {
        page_key,
        tier_used,
        target_lang,
        detect_only: mode.is_detect_only(),
        from_cache: false,
        cancelled: true,
        elapsed_ms: u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
        image_width: 0,
        image_height: 0,
        regions: Vec::new(),
    }
}

fn derive_page_key(page_token: &str) -> String {
    let digest = Sha256::digest(page_token.as_bytes());
    let hex = hex_lower(&digest);
    hex.chars().take(24).collect()
}

fn normalize_target_lang(raw: &str) -> String {
    let trimmed = raw.trim().to_ascii_lowercase();
    match trimmed.as_str() {
        "vi" | "zh" | "ja" | "ru" | "ko" | "th" | "en" => trimmed,
        _ => "en".to_owned(),
    }
}

const fn select_sidecar_target(caps: &AiRuntimeCapabilities) -> SidecarTarget {
    if cfg!(target_os = "windows") {
        if caps.vulkan_supported {
            SidecarTarget::WindowsVulkan
        } else {
            SidecarTarget::WindowsCpu
        }
    } else {
        SidecarTarget::None
    }
}

const fn current_platform() -> &'static str {
    if cfg!(target_os = "android") {
        "android"
    } else if cfg!(target_os = "windows") {
        "windows"
    } else {
        "other"
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use image::{DynamicImage, ImageBuffer, ImageFormat, Rgb};

    use super::*;

    fn synthetic_manga_page_png() -> Vec<u8> {
        // Dark gray panel background with one bright white speech bubble containing dark text strokes.
        let mut img = ImageBuffer::from_pixel(400, 600, Rgb([60_u8, 60, 60]));
        for y in 80..220 {
            for x in 180..360 {
                img.put_pixel(x, y, Rgb([252_u8, 252, 252]));
            }
        }
        // Dark ink strokes inside the bubble so speech-bubble filter distinguishes it from blank margin.
        for y in 110..190 {
            if y % 6 == 0 {
                for x in 210..330 {
                    img.put_pixel(x, y, Rgb([20_u8, 20, 20]));
                }
            }
        }
        let mut out = Cursor::new(Vec::new());
        DynamicImage::ImageRgb8(img)
            .write_to(&mut out, ImageFormat::Png)
            .expect("encode synthetic manga page");
        out.into_inner()
    }

    #[tokio::test]
    async fn engine_detects_bubble_caches_cancels_and_hard_purges() {
        let root = std::env::temp_dir().join(format!(
            "rtmaga_engine_test_{}_{}",
            std::process::id(),
            crate::repo::now_unix_secs()
        ));
        std::fs::create_dir_all(&root).expect("create test data_dir");

        let engine = AiEngine::new(&root, TranslationModelTier::Lite);
        let status = engine.status(None).await;
        assert_eq!(
            status.active_tier,
            TranslationModelTier::Lite,
            "starts Lite"
        );
        assert!(status.tier1_ready, "Tier 1 always ready");
        assert!(!status.tier2_ready, "Tier 2 not downloaded yet");
        assert!(!status.loaded_in_memory, "idle engine uses zero model RAM");

        // Selecting Tier 2 before download must return an explicit NotFound error.
        let switch_err = engine
            .set_tier(None, TranslationModelTier::Qwen3Vl2B)
            .await
            .expect_err("cannot switch to undownloaded Tier 2");
        assert!(
            matches!(switch_err, AppError::NotFound { .. }),
            "expected NotFound when Tier 2 weights are missing"
        );

        let page_bytes = synthetic_manga_page_png();
        let gen0 = engine.job_generation.load(Ordering::Acquire);
        let res = engine
            .infer_raw_bytes(
                None,
                "page-bubble-1".to_owned(),
                page_bytes.clone(),
                "en".to_owned(),
                AiPageMode::DetectOnly,
                gen0,
                Instant::now(),
            )
            .await
            .expect("detect_only bubble inference succeeds");
        assert!(!res.cancelled, "fresh job not cancelled");
        assert!(!res.from_cache, "first run computes fresh");
        assert!(
            !res.regions.is_empty(),
            "synthetic speech bubble must be detected by Tier 1 fallback"
        );

        // Second identical request hits cache immediately.
        let cached = engine
            .infer_raw_bytes(
                None,
                "page-bubble-1".to_owned(),
                page_bytes.clone(),
                "en".to_owned(),
                AiPageMode::DetectOnly,
                gen0,
                Instant::now(),
            )
            .await
            .expect("cached lookup succeeds");
        assert!(cached.from_cache, "second request served from cache");

        // Cancelling jobs bumps generation so stale requests abort immediately.
        let gen1 = engine.cancel_jobs(None).await;
        assert!(gen1 > gen0, "generation monotonically increases");
        let stale = engine
            .infer_raw_bytes(
                None,
                "page-bubble-2".to_owned(),
                page_bytes,
                "en".to_owned(),
                AiPageMode::DetectOnly,
                gen0,
                Instant::now(),
            )
            .await
            .expect("stale job returns cancelled result cleanly");
        assert!(stale.cancelled, "stale generation aborts immediately");

        // Hard-unload & purge clears RAM + disk cache under <data_dir>/cache/ai/.
        let after_purge = engine
            .hard_unload_and_purge(None)
            .await
            .expect("hard_unload_and_purge succeeds");
        assert!(
            !after_purge.loaded_in_memory,
            "loaded_in_memory is false after hard unload"
        );
        let ai_cache_dir = root.join("cache").join("ai");
        let leftover_files = std::fs::read_dir(&ai_cache_dir).map_or(0, Iterator::count);
        assert_eq!(leftover_files, 0, "all AI disk cache files purged");

        let _ = std::fs::remove_dir_all(&root);
    }

    #[tokio::test]
    async fn hard_unload_then_translate_with_older_client_generation_succeeds() {
        let root = std::env::temp_dir().join(format!(
            "rtmaga_engine_toggle_test_{}_{}",
            std::process::id(),
            crate::repo::now_unix_secs()
        ));
        std::fs::create_dir_all(&root).expect("create test data_dir");

        let engine = AiEngine::new(&root, TranslationModelTier::Lite);
        let stale_client_gen = engine.job_generation.load(Ordering::Acquire);

        // Simulate OFF -> ON toggle calling hard_unload_and_purge (even multiple times),
        // advancing server job_generation ahead of the client's cached counter.
        let _ = engine
            .hard_unload_and_purge(None)
            .await
            .expect("first hard_unload_and_purge succeeds");
        let _ = engine
            .hard_unload_and_purge(None)
            .await
            .expect("second hard_unload_and_purge succeeds");
        let server_gen_after_purge = engine.job_generation.load(Ordering::Acquire);
        assert!(
            server_gen_after_purge > stale_client_gen,
            "hard_unload_and_purge must advance server job_generation"
        );

        let page_bytes = synthetic_manga_page_png();
        let expected_gen = engine.capture_request_generation(Some(stale_client_gen));
        assert_eq!(
            expected_gen, server_gen_after_purge,
            "fresh request must capture current server generation despite older client_generation"
        );

        let res = engine
            .infer_raw_bytes(
                None,
                "page-after-toggle".to_owned(),
                page_bytes.clone(),
                "en".to_owned(),
                AiPageMode::DetectOnly,
                expected_gen,
                Instant::now(),
            )
            .await
            .expect("translation after hard_unload_and_purge succeeds");
        assert!(
            !res.cancelled,
            "fresh translation with older client_generation must NOT be cancelled"
        );
        assert!(
            !res.regions.is_empty(),
            "detected speech bubble must be returned after OFF -> ON toggle"
        );

        // If generation advances *after* request start, in-flight cancellation still triggers.
        let _ = engine.cancel_jobs(None).await;
        let cancelled = engine
            .infer_raw_bytes(
                None,
                "page-cancelled-midflight".to_owned(),
                page_bytes,
                "en".to_owned(),
                AiPageMode::DetectOnly,
                expected_gen,
                Instant::now(),
            )
            .await
            .expect("post-start cancellation returns cancelled result cleanly");
        assert!(
            cancelled.cancelled,
            "generation bump after request start must cancel in-flight job"
        );

        let _ = std::fs::remove_dir_all(&root);
    }
}
