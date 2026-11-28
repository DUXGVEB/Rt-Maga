//! Unified 2-Tier on-device AI engine for manga speech-bubble auto-zoom and translation overlay.
//!
//! - **Tier 1 (`Lite`)**: compact `PP-OCRv6_tiny` ONNX detection/recognition + on-device NMT
//!   (Android: `AiRuntimePlugin` ONNX Runtime XNNPACK/NNAPI + ML Kit Translate; Windows: `ort`
//!   DirectML/CPU + speech-bubble contour fallback).
//! - **Tier 2 (`Qwen3Vl2B`)**: optional user-downloaded `Qwen3-VL-2B-Instruct-IQ4_XS` + `mmproj`
//!   multimodal VLM (`llama.cpp` Vulkan / CPU fallback, `n_ctx=2048`, `flash_attn=true`,
//!   `q8_0` KV-cache, `use_mmap=true`, stateless per page).

pub(crate) mod cache;
pub(crate) mod downloader;
pub(crate) mod engine;
pub(crate) mod llama_tier2;
pub(crate) mod manifest;
#[cfg(any(not(target_os = "android"), test))]
pub(crate) mod nmt_opus;
pub(crate) mod onnx_tier1;
pub(crate) mod preprocess;

use serde::{Deserialize, Serialize};

pub(crate) use engine::AiEngine;

use crate::settings::TranslationModelTier;

/// Execution scope for a page-level AI inference request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AiPageMode {
    /// Speech-bubble bounding-box detection only (Bubble Auto-Zoom without translation).
    DetectOnly,
    /// Speech-bubble detection plus on-device translation into the target language.
    Translate,
}

impl AiPageMode {
    /// Returns `true` when this mode skips translation and only detects speech bubbles.
    pub(crate) const fn is_detect_only(self) -> bool {
        matches!(self, Self::DetectOnly)
    }
}

/// Installation / provisioning state of an AI model tier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub(crate) enum AiModelStatus {
    /// Ready for immediate local inference.
    Ready,
    /// Optional weights have not been downloaded yet.
    NotDownloaded,
    /// Weights are actively downloading.
    Downloading,
    /// Downloaded weights are undergoing SHA-256 verification or extraction.
    Verifying,
    /// Last provisioning or runtime check encountered an error.
    Error,
}

/// Summary card entry for one selectable AI model tier in Settings -> Accessibility.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AiModelEntryView {
    /// Logical tier identifier (`"lite"` or `"qwen3_vl_2b"`).
    pub tier: TranslationModelTier,
    /// Stable model pack identifier.
    pub id: String,
    /// Human-readable display label.
    pub display_name: String,
    /// Current provisioning state.
    pub status: AiModelStatus,
    /// Actual verified bytes currently present on disk for this tier.
    pub installed_bytes: u64,
    /// Expected full byte footprint for this tier on the active platform.
    pub required_bytes: u64,
    /// `true` when ready for immediate inference.
    pub ready: bool,
    /// `true` for the built-in Lite tier; `false` for optional downloaded tiers.
    pub bundled: bool,
    /// Active hardware acceleration summary (`"DirectML / CPU"`, `"XNNPACK / NNAPI"`, `"Vulkan"`, etc.).
    pub accelerator: String,
}

/// Summary card entry for one selectable offline Lite NMT language pack (Windows OPUS-MT or
/// Android ML Kit Translate).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AiNmtPackView {
    /// Stable pack identifier (`"ja-en"`, `"en-vi"` on Windows; `"ja"`, `"vi"` on Android).
    pub id: String,
    /// Human-readable display label (e.g. `"JA → EN"`, `"EN → VI"`, `"Japanese"`).
    pub display_name: String,
    /// Source language ISO code.
    pub source_lang: String,
    /// Target language ISO code.
    pub target_lang: String,
    /// Current provisioning state.
    pub status: AiModelStatus,
    /// Bytes currently installed on disk (or approximate footprint on Android ML Kit).
    pub installed_bytes: u64,
    /// Expected download footprint in bytes.
    pub required_bytes: u64,
    /// `true` when installed and ready for offline translation.
    pub ready: bool,
}

/// Complete live status snapshot of the on-device AI subsystem.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[expect(
    clippy::struct_excessive_bools,
    reason = "IPC status snapshot DTO serialized directly to the Svelte frontend"
)]
pub(crate) struct AiStatusView {
    /// Currently selected translation model tier.
    pub active_tier: TranslationModelTier,
    /// Runtime platform (`"windows"`, `"android"`, or `"other"`).
    pub platform: String,
    /// Resolved on-device storage root for downloaded AI model weights.
    pub storage_dir: String,
    /// `true` when Tier 1 (`Lite`) is available.
    pub tier1_ready: bool,
    /// `true` when Tier 2 (`Qwen3-VL-2B`) native runtime is supported on this build/device.
    pub tier2_supported: bool,
    /// `true` when Tier 2 (`Qwen3-VL-2B`) weights (and Windows sidecar if on Windows) are verified on disk.
    pub tier2_ready: bool,
    /// `true` while any AI model or NMT pack download is in progress.
    pub downloading: bool,
    /// `true` when Vulkan GPU acceleration is detected on this device.
    pub vulkan_supported: bool,
    /// `true` while native inference sessions or sidecar workers are currently loaded in memory.
    pub loaded_in_memory: bool,
    /// Number of currently queued or running page inference jobs.
    pub active_jobs: u32,
    /// Selectable model tier entries rendered in the Translation Model selector.
    pub models: Vec<AiModelEntryView>,
    /// Offline Lite NMT language packs managed in Settings -> Accessibility.
    pub nmt_packs: Vec<AiNmtPackView>,
}

/// Lifecycle phase of an active Tier 2 model download.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub(crate) enum AiDownloadStage {
    Starting,
    Downloading,
    Verifying,
    Extracting,
    Completed,
    Cancelled,
    Failed,
}

/// Live progress payload streamed via Tauri `Channel` and `"ai://download-progress"` events.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AiDownloadProgressEvent {
    pub stage: AiDownloadStage,
    pub asset_id: String,
    pub file_index: u32,
    pub file_count: u32,
    pub downloaded_bytes: u64,
    pub total_bytes: u64,
    pub percent: f32,
    pub speed_bytes_per_sec: u64,
    pub message: Option<String>,
}

/// Single speech-bubble bounding box and optional translated text on a manga page.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TranslatedRegionView {
    /// Translated text in `targetLang` (empty string when `detectOnly == true`).
    pub text: String,
    /// Optional raw OCR source text produced by Tier 1.
    pub source_text: Option<String>,
    /// Normalized `[x1, y1, x2, y2]` bounding box in `[0..=1000]` image coordinates.
    pub bbox: [u16; 4],
    /// Detection / recognition confidence in `0.0..=1.0`.
    pub confidence: f32,
}

/// Complete page-level AI bubble detection / translation result returned to the Reader overlay.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PageAiResultView {
    /// Stable cache identifier for the requested page.
    pub page_key: String,
    /// Tier that actually produced this result.
    pub tier_used: TranslationModelTier,
    /// Requested ISO target language code (`"en"`, `"vi"`, `"ja"`, etc.).
    pub target_lang: String,
    /// `true` when invoked solely for Bubble Auto-Zoom without translation.
    pub detect_only: bool,
    /// `true` when served directly from RAM or disk cache without re-running inference.
    pub from_cache: bool,
    /// `true` when aborted early due to fast page swipe or explicit cancellation.
    pub cancelled: bool,
    /// Wall-clock inference time in milliseconds.
    pub elapsed_ms: u64,
    /// Original decoded page width in pixels.
    pub image_width: u32,
    /// Original decoded page height in pixels.
    pub image_height: u32,
    /// Validated speech-bubble regions in normalized `[0..=1000]` coordinates.
    pub regions: Vec<TranslatedRegionView>,
    /// Offline NMT language packs that must be downloaded before Lite translation can complete.
    #[serde(default)]
    pub missing_nmt_packs: Vec<AiNmtPackView>,
    /// Non-fatal user-visible diagnostic message when translation could not complete.
    #[serde(default)]
    pub error: Option<String>,
}
