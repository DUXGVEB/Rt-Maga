//! Resumable HTTP `Range` downloader with streaming SHA-256 verification, exponential backoff,
//! cancellation rollback, and zip-slip-safe extraction for Windows `llama.cpp` sidecars.

use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Component, Path, PathBuf};
use std::time::{Duration, Instant};

use crc32fast::Hasher as Crc32Hasher;
use flate2::read::DeflateDecoder;
use reqwest::header::{CONTENT_LENGTH, CONTENT_RANGE, RANGE};
use reqwest::{Client, StatusCode};
use sha2::{Digest, Sha256};
use tauri::AppHandle;
use tokio::io::{AsyncSeekExt, AsyncWriteExt};
use tokio::sync::watch;

use crate::error::AppError;
use crate::services::ai::cache::hex_lower;
#[cfg(any(not(target_os = "android"), test))]
use crate::services::ai::manifest::opus_mt_pack;
use crate::services::ai::manifest::{
    AiArchiveFormat, ModelAssetSpec, SidecarTarget, TIER2_VLM_GGUF, TIER2_VLM_MMPROJ,
    WIN_LLAMA_SIDECAR_DIR, is_allowed_manifest_url, is_valid_sha256_hex, tier2_bundle_assets,
    total_bytes,
};
use crate::services::ai::{AiDownloadProgressEvent, AiDownloadStage};
use mangacore::ProxyConfig;

/// Exponential retry backoff delays for transient transport errors (3 attempts total).
const RETRY_BACKOFF_MS: [u64; 3] = [500, 1500, 3500];

/// Minimum interval between progress updates emitted to the UI channel.
const PROGRESS_EMIT_INTERVAL: Duration = Duration::from_millis(120);

/// Maximum size of any single extracted Windows sidecar binary/DLL (`128 MiB`).
const MAX_SIDECAR_ENTRY_BYTES: u64 = 128 * 1024 * 1024;

/// Maximum ZIP EOCD search window (`64 KiB + 22 bytes`).
const EOCD_SEARCH_WINDOW: u64 = 65_557;

/// Resolves the platform-appropriate on-device AI model storage root.
///
/// On Android, queries `AiRuntimePlugin.getAiStorageDir` (`getExternalFilesDir("ai_models")`)
/// with a safe fallback to `<data_dir>/ai_models`. On Windows/desktop, uses `<data_dir>/ai_models`.
pub(crate) fn resolve_storage_root(data_dir: &Path, app: Option<&AppHandle>) -> PathBuf {
    #[cfg(target_os = "android")]
    if let Some(handle) = app
        && let Some(ext_dir) = crate::android_env::get_ai_storage_dir(handle)
    {
        return ext_dir;
    }

    #[cfg(not(target_os = "android"))]
    let _ = app;

    data_dir.join("ai_models")
}

/// Returns the expected absolute path of a manifest asset inside `storage_root`.
pub(crate) fn asset_destination(storage_root: &Path, spec: &ModelAssetSpec) -> PathBuf {
    storage_root.join(spec.rel_path)
}

/// Returns the expected extracted Windows `llama-mtmd-cli.exe` path inside `storage_root`.
#[cfg(any(not(target_os = "android"), test))]
pub(crate) fn win_llama_mtmd_cli_path(storage_root: &Path) -> PathBuf {
    storage_root
        .join(WIN_LLAMA_SIDECAR_DIR)
        .join("llama-mtmd-cli.exe")
}

/// Returns the expected extracted Windows `llama-server.exe` path inside `storage_root`.
pub(crate) fn win_llama_server_path(storage_root: &Path) -> PathBuf {
    storage_root
        .join(WIN_LLAMA_SIDECAR_DIR)
        .join("llama-server.exe")
}

/// Returns `true` when Tier 2 weights (and Windows `llama-server.exe` on Windows) are present
/// with exact expected sizes on disk.
pub(crate) fn is_tier2_installed(storage_root: &Path) -> bool {
    let gguf_ok = file_matches_size(
        &asset_destination(storage_root, &TIER2_VLM_GGUF),
        TIER2_VLM_GGUF.expected_bytes,
    );
    let mmproj_ok = file_matches_size(
        &asset_destination(storage_root, &TIER2_VLM_MMPROJ),
        TIER2_VLM_MMPROJ.expected_bytes,
    );
    if !gguf_ok || !mmproj_ok {
        return false;
    }

    #[cfg(target_os = "windows")]
    {
        win_llama_server_path(storage_root).is_file()
            || win_llama_mtmd_cli_path(storage_root).is_file()
    }

    #[cfg(not(target_os = "windows"))]
    {
        true
    }
}

/// Sums the on-disk bytes currently installed for Tier 2 (`tier2/` weights + Windows sidecar dir).
pub(crate) fn installed_tier2_bytes(storage_root: &Path) -> u64 {
    let mut sum = 0_u64;
    sum = sum.saturating_add(dir_size_sync(&storage_root.join("tier2")));
    #[cfg(target_os = "windows")]
    {
        sum = sum.saturating_add(dir_size_sync(&storage_root.join(WIN_LLAMA_SIDECAR_DIR)));
    }
    sum
}

/// Deletes all downloaded Tier 2 model weights, partial files, and Windows sidecar binaries.
///
/// # Errors
///
/// Returns [`AppError::Internal`] if removing downloaded Tier 2 directories fails.
pub(crate) async fn delete_tier2_assets(storage_root: &Path) -> Result<(), AppError> {
    let root = storage_root.to_path_buf();
    tokio::task::spawn_blocking(move || {
        for sub in ["tier2", "downloads", WIN_LLAMA_SIDECAR_DIR] {
            let path = root.join(sub);
            if path.exists() {
                std::fs::remove_dir_all(&path).map_err(|e| AppError::Internal {
                    message: format!(
                        "failed to remove Tier 2 directory '{}': {e}",
                        path.display()
                    ),
                })?;
            }
        }
        let staging = root.join(format!("{WIN_LLAMA_SIDECAR_DIR}.extracting"));
        if staging.exists() {
            let _ = std::fs::remove_dir_all(&staging);
        }
        Ok(())
    })
    .await
    .map_err(|e| AppError::Internal {
        message: format!("Tier 2 deletion worker failed: {e}"),
    })?
}

/// Downloads, SHA-256 verifies, and provisions the entire Tier 2 bundle (`Qwen3-VL-2B-Instruct`
/// GGUF + `mmproj`, plus Windows `llama.cpp` `b12808` sidecar on Windows).
///
/// # Errors
///
/// Returns [`AppError`] on network failure, cancellation, checksum mismatch, or extraction failure.
pub(crate) async fn download_tier2_bundle<F>(
    storage_root: &Path,
    sidecar: SidecarTarget,
    proxy: Option<&ProxyConfig>,
    cancel_rx: watch::Receiver<bool>,
    mut on_progress: F,
) -> Result<(), AppError>
where
    F: FnMut(AiDownloadProgressEvent) + Send,
{
    let assets = tier2_bundle_assets(sidecar);
    let bundle_total = total_bytes(&assets);
    let file_count = u32::try_from(assets.len()).unwrap_or(u32::MAX);

    let client = build_download_client(proxy)?;
    let mut completed_prefix_bytes = 0_u64;
    let mut emit = |stage, id: &str, idx: u32, bytes: u64, speed: u64, msg: Option<String>| {
        on_progress(make_progress(
            stage,
            id,
            idx,
            file_count,
            bytes,
            bundle_total,
            speed,
            msg,
        ));
    };

    for (idx, asset) in assets.iter().enumerate() {
        let file_index = u32::try_from(idx.saturating_add(1)).unwrap_or(file_count);

        if *cancel_rx.borrow() {
            emit(
                AiDownloadStage::Cancelled,
                asset.id,
                file_index,
                completed_prefix_bytes,
                0,
                Some("Download cancelled".to_owned()),
            );
            return Err(AppError::Internal {
                message: "AI model download cancelled".to_owned(),
            });
        }

        // Skip re-downloading if already fully present and verified on disk.
        let dest = asset_destination(storage_root, asset);
        if is_asset_already_installed(storage_root, asset, &dest).await {
            completed_prefix_bytes = completed_prefix_bytes.saturating_add(asset.expected_bytes);
            emit(
                AiDownloadStage::Downloading,
                asset.id,
                file_index,
                completed_prefix_bytes,
                0,
                None,
            );
            continue;
        }

        let prefix = completed_prefix_bytes;
        let res = download_verified_asset(
            &client,
            storage_root,
            asset,
            cancel_rx.clone(),
            |stage, file_bytes, speed, msg| {
                let agg = prefix.saturating_add(file_bytes.min(asset.expected_bytes));
                emit(stage, asset.id, file_index, agg, speed, msg);
            },
        )
        .await;

        if let Err(err) = res {
            let stage = if *cancel_rx.borrow() {
                AiDownloadStage::Cancelled
            } else {
                AiDownloadStage::Failed
            };
            emit(
                stage,
                asset.id,
                file_index,
                completed_prefix_bytes,
                0,
                Some(err.to_string()),
            );
            return Err(err);
        }

        if asset.archive == AiArchiveFormat::ZipArchive {
            emit(
                AiDownloadStage::Extracting,
                asset.id,
                file_index,
                completed_prefix_bytes.saturating_add(asset.expected_bytes),
                0,
                Some("Extracting llama.cpp sidecar".to_owned()),
            );
            finalize_sidecar_archive(storage_root, &dest).await?;
        }

        completed_prefix_bytes = completed_prefix_bytes.saturating_add(asset.expected_bytes);
    }

    emit(
        AiDownloadStage::Completed,
        TIER2_VLM_GGUF.id,
        file_count,
        bundle_total,
        0,
        Some("Qwen3-VL-2B ready".to_owned()),
    );
    Ok(())
}

/// Downloads and SHA-256 verifies one or more Windows offline OPUS-MT translation packs.
///
/// # Errors
///
/// Returns [`AppError`] on unknown pack ID, cancellation, network failure, or checksum mismatch.
#[cfg(any(not(target_os = "android"), test))]
pub(crate) async fn download_opus_nmt_packs<F>(
    storage_root: &Path,
    pack_ids: &[String],
    proxy: Option<&ProxyConfig>,
    cancel_rx: watch::Receiver<bool>,
    mut on_progress: F,
) -> Result<(), AppError>
where
    F: FnMut(AiDownloadProgressEvent) + Send,
{
    let mut assets = Vec::new();
    for id in pack_ids {
        let Some(pack_assets) = opus_mt_pack(id.as_str()) else {
            return Err(AppError::NotFound {
                message: format!("unknown offline OPUS-MT pack '{id}'"),
            });
        };
        assets.extend_from_slice(pack_assets);
    }
    if assets.is_empty() {
        return Ok(());
    }

    let bundle_total = total_bytes(&assets);
    let file_count = u32::try_from(assets.len()).unwrap_or(u32::MAX);
    let client = build_download_client(proxy)?;
    let mut completed_prefix_bytes = 0_u64;
    let mut emit = |stage, id: &str, idx: u32, bytes: u64, speed: u64, msg: Option<String>| {
        on_progress(make_progress(
            stage,
            id,
            idx,
            file_count,
            bytes,
            bundle_total,
            speed,
            msg,
        ));
    };

    for (idx, asset) in assets.iter().enumerate() {
        let file_index = u32::try_from(idx.saturating_add(1)).unwrap_or(file_count);
        if *cancel_rx.borrow() {
            emit(
                AiDownloadStage::Cancelled,
                asset.id,
                file_index,
                completed_prefix_bytes,
                0,
                Some("Download cancelled".to_owned()),
            );
            return Err(AppError::Internal {
                message: "NMT pack download cancelled".to_owned(),
            });
        }

        let dest = asset_destination(storage_root, asset);
        if is_asset_already_installed(storage_root, asset, &dest).await {
            completed_prefix_bytes = completed_prefix_bytes.saturating_add(asset.expected_bytes);
            emit(
                AiDownloadStage::Downloading,
                asset.id,
                file_index,
                completed_prefix_bytes,
                0,
                None,
            );
            continue;
        }

        let prefix = completed_prefix_bytes;
        let res = download_verified_asset(
            &client,
            storage_root,
            asset,
            cancel_rx.clone(),
            |stage, file_bytes, speed, msg| {
                let agg = prefix.saturating_add(file_bytes.min(asset.expected_bytes));
                emit(stage, asset.id, file_index, agg, speed, msg);
            },
        )
        .await;

        if let Err(err) = res {
            let stage = if *cancel_rx.borrow() {
                AiDownloadStage::Cancelled
            } else {
                AiDownloadStage::Failed
            };
            emit(
                stage,
                asset.id,
                file_index,
                completed_prefix_bytes,
                0,
                Some(err.to_string()),
            );
            return Err(err);
        }

        completed_prefix_bytes = completed_prefix_bytes.saturating_add(asset.expected_bytes);
    }

    let final_id = pack_ids.first().map_or("nmt", String::as_str);
    emit(
        AiDownloadStage::Completed,
        final_id,
        file_count,
        bundle_total,
        0,
        Some("Offline translation pack ready".to_owned()),
    );
    Ok(())
}

/// Deletes a downloaded Windows OPUS-MT pack directory from `<storage_root>/nmt/<pack_id>`.
///
/// # Errors
///
/// Returns [`AppError`] if `pack_id` is invalid or removal fails.
#[cfg(any(not(target_os = "android"), test))]
pub(crate) async fn delete_opus_nmt_pack(
    storage_root: &Path,
    pack_id: &str,
) -> Result<(), AppError> {
    if opus_mt_pack(pack_id).is_none() {
        return Err(AppError::NotFound {
            message: format!("unknown offline OPUS-MT pack '{pack_id}'"),
        });
    }
    let dir = storage_root.join("nmt").join(pack_id);
    tokio::task::spawn_blocking(move || {
        if dir.exists() {
            std::fs::remove_dir_all(&dir).map_err(|e| AppError::Internal {
                message: format!("failed to remove OPUS-MT pack '{}': {e}", dir.display()),
            })?;
        }
        Ok(())
    })
    .await
    .map_err(|e| AppError::Internal {
        message: format!("OPUS-MT pack deletion worker failed: {e}"),
    })?
}

async fn is_asset_already_installed(
    storage_root: &Path,
    asset: &ModelAssetSpec,
    dest: &Path,
) -> bool {
    match asset.archive {
        AiArchiveFormat::RawFile => {
            file_matches_size(dest, asset.expected_bytes)
                && verify_file_sha256(dest, asset.expected_bytes, asset.sha256_hex)
                    .await
                    .is_ok()
        }
        AiArchiveFormat::ZipArchive => win_llama_server_path(storage_root).is_file(),
    }
}

async fn finalize_sidecar_archive(storage_root: &Path, zip_dest: &Path) -> Result<(), AppError> {
    let zip_path = zip_dest.to_path_buf();
    let target_dir = storage_root.join(WIN_LLAMA_SIDECAR_DIR);
    let extract_res =
        tokio::task::spawn_blocking(move || extract_llama_sidecar_zip(&zip_path, &target_dir))
            .await
            .map_err(|e| AppError::Internal {
                message: format!("sidecar extraction worker panicked: {e}"),
            })?;

    // Remove zip archive after extraction (or rollback failure) to reclaim disk space.
    let _ = tokio::fs::remove_file(zip_dest).await;
    extract_res
}

/// Downloads a single [`ModelAssetSpec`] using resumable HTTP `Range` requests into a `.part`
/// file, verifies exact byte length + SHA-256 digest, and atomically promotes it to `dest`.
///
/// Rolls back (deletes `.part`) on checksum mismatch, cancellation, or unrecoverable error.
///
/// # Errors
///
/// Returns [`AppError`] on invalid URL, network error, cancellation, or verification mismatch.
pub(crate) async fn download_verified_asset<F>(
    client: &Client,
    storage_root: &Path,
    spec: &ModelAssetSpec,
    cancel_rx: watch::Receiver<bool>,
    mut on_event: F,
) -> Result<PathBuf, AppError>
where
    F: FnMut(AiDownloadStage, u64, u64, Option<String>),
{
    if !is_allowed_manifest_url(spec.url) && !is_loopback_test_url(spec.url) {
        return Err(AppError::InvalidUrl {
            message: format!("refused non-allowlisted model download URL: {}", spec.url),
        });
    }
    if !is_valid_sha256_hex(spec.sha256_hex) {
        return Err(AppError::CorruptedData {
            message: format!("manifest SHA-256 for {} is malformed", spec.id),
        });
    }

    let dest = asset_destination(storage_root, spec);
    if let Some(parent) = dest.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| AppError::Internal {
                message: format!(
                    "failed to prepare model directory '{}': {e}",
                    parent.display()
                ),
            })?;
    }

    let part_path = part_path_for(&dest);
    on_event(AiDownloadStage::Starting, 0, 0, None);

    let mut last_err: Option<AppError> = None;
    for attempt in 0..=RETRY_BACKOFF_MS.len() {
        if *cancel_rx.borrow() {
            let _ = tokio::fs::remove_file(&part_path).await;
            return Err(AppError::Internal {
                message: "download cancelled".to_owned(),
            });
        }

        match stream_into_part(client, spec, &part_path, cancel_rx.clone(), &mut on_event).await {
            Ok(()) => {
                last_err = None;
                break;
            }
            Err(DownloadStepError::Cancelled) => {
                let _ = tokio::fs::remove_file(&part_path).await;
                return Err(AppError::Internal {
                    message: "download cancelled".to_owned(),
                });
            }
            Err(DownloadStepError::Fatal(err)) => {
                let _ = tokio::fs::remove_file(&part_path).await;
                return Err(err);
            }
            Err(DownloadStepError::Retryable(err)) => {
                last_err = Some(err);
                if let Some(&delay_ms) = RETRY_BACKOFF_MS.get(attempt) {
                    let mut wait_rx = cancel_rx.clone();
                    tokio::select! {
                        () = tokio::time::sleep(Duration::from_millis(delay_ms)) => {}
                        _ = wait_rx.changed() => {
                            if *wait_rx.borrow() {
                                let _ = tokio::fs::remove_file(&part_path).await;
                                return Err(AppError::Internal {
                                    message: "download cancelled during retry backoff".to_owned(),
                                });
                            }
                        }
                    }
                }
            }
        }
    }

    if let Some(err) = last_err {
        let _ = tokio::fs::remove_file(&part_path).await;
        return Err(err);
    }

    if *cancel_rx.borrow() {
        let _ = tokio::fs::remove_file(&part_path).await;
        return Err(AppError::Internal {
            message: "download cancelled before verification".to_owned(),
        });
    }

    on_event(
        AiDownloadStage::Verifying,
        spec.expected_bytes,
        0,
        Some(format!("Verifying SHA-256 for {}", spec.id)),
    );

    if let Err(err) = verify_file_sha256(&part_path, spec.expected_bytes, spec.sha256_hex).await {
        let _ = tokio::fs::remove_file(&part_path).await;
        return Err(err);
    }

    atomic_replace_file(&part_path, &dest)
        .await
        .inspect_err(|_| {
            let _ = std::fs::remove_file(&part_path);
        })?;

    Ok(dest)
}

enum DownloadStepError {
    Cancelled,
    Retryable(AppError),
    Fatal(AppError),
}

#[expect(
    clippy::too_many_lines,
    reason = "handles resumable HTTP Range negotiation, chunked streaming, cancellation, and speed tracking"
)]
async fn stream_into_part<F>(
    client: &Client,
    spec: &ModelAssetSpec,
    part_path: &Path,
    mut cancel_rx: watch::Receiver<bool>,
    on_event: &mut F,
) -> Result<(), DownloadStepError>
where
    F: FnMut(AiDownloadStage, u64, u64, Option<String>),
{
    let mut existing_len = tokio::fs::metadata(part_path)
        .await
        .map_or(0_u64, |m| m.len());

    if existing_len > spec.expected_bytes {
        let _ = tokio::fs::remove_file(part_path).await;
        existing_len = 0;
    } else if existing_len == spec.expected_bytes {
        on_event(AiDownloadStage::Downloading, existing_len, 0, None);
        return Ok(());
    }

    let mut req = client.get(spec.url);
    if existing_len > 0 {
        req = req.header(RANGE, format!("bytes={existing_len}-"));
    }

    let mut response = tokio::select! {
        res = req.send() => res.map_err(|e| DownloadStepError::Retryable(AppError::Network {
            message: format!("download request failed for {}: {e}", spec.id),
        }))?,
        _ = cancel_rx.changed() => {
            if *cancel_rx.borrow() {
                return Err(DownloadStepError::Cancelled);
            }
            return Err(DownloadStepError::Retryable(AppError::Network {
                message: "cancel channel closed unexpectedly".to_owned(),
            }));
        }
    };

    let status = response.status();
    if status == StatusCode::RANGE_NOT_SATISFIABLE {
        let _ = tokio::fs::remove_file(part_path).await;
        return Err(DownloadStepError::Retryable(AppError::HttpError {
            status: status.as_u16(),
            message: format!("range reset required for {}", spec.id),
        }));
    }

    if status.is_server_error() || status == StatusCode::TOO_MANY_REQUESTS {
        return Err(DownloadStepError::Retryable(AppError::HttpError {
            status: status.as_u16(),
            message: format!("upstream returned HTTP {} for {}", status.as_u16(), spec.id),
        }));
    }

    if status != StatusCode::OK && status != StatusCode::PARTIAL_CONTENT {
        return Err(DownloadStepError::Fatal(AppError::HttpError {
            status: status.as_u16(),
            message: format!(
                "unexpected HTTP {} downloading {}",
                status.as_u16(),
                spec.id
            ),
        }));
    }

    let transfer = if status == StatusCode::PARTIAL_CONTENT && existing_len > 0 {
        TransferMode::Resumed {
            offset: existing_len,
        }
    } else {
        TransferMode::Full
    };
    validate_response_lengths(&response, spec, transfer)?;

    let mut file = match transfer {
        TransferMode::Resumed { offset } => {
            let mut f = tokio::fs::OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(false)
                .open(part_path)
                .await
                .map_err(|e| {
                    DownloadStepError::Fatal(AppError::Internal {
                        message: format!("failed to reopen partial file: {e}"),
                    })
                })?;
            f.seek(SeekFrom::Start(offset)).await.map_err(|e| {
                DownloadStepError::Fatal(AppError::Internal {
                    message: format!("failed to seek partial file: {e}"),
                })
            })?;
            f
        }
        TransferMode::Full => {
            existing_len = 0;
            tokio::fs::OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(true)
                .open(part_path)
                .await
                .map_err(|e| {
                    DownloadStepError::Fatal(AppError::Internal {
                        message: format!("failed to create partial file: {e}"),
                    })
                })?
        }
    };

    let mut downloaded = existing_len;
    let started_at = Instant::now();
    let mut session_bytes = 0_u64;
    let mut last_emit = Instant::now();
    on_event(AiDownloadStage::Downloading, downloaded, 0, None);

    loop {
        if *cancel_rx.borrow() {
            return Err(DownloadStepError::Cancelled);
        }

        let maybe_chunk = tokio::select! {
            chunk_res = response.chunk() => chunk_res.map_err(|e| DownloadStepError::Retryable(AppError::Network {
                message: format!("stream interrupted while downloading {}: {e}", spec.id),
            }))?,
            _ = cancel_rx.changed() => {
                if *cancel_rx.borrow() {
                    return Err(DownloadStepError::Cancelled);
                }
                continue;
            }
        };

        let Some(chunk) = maybe_chunk else {
            break;
        };

        let chunk_len = u64::try_from(chunk.len()).unwrap_or(u64::MAX);
        if downloaded.saturating_add(chunk_len) > spec.expected_bytes {
            return Err(DownloadStepError::Fatal(AppError::CorruptedData {
                message: format!(
                    "download for {} exceeded expected size ({} B)",
                    spec.id, spec.expected_bytes
                ),
            }));
        }

        file.write_all(&chunk).await.map_err(|e| {
            DownloadStepError::Fatal(AppError::Internal {
                message: format!("failed to write chunk for {}: {e}", spec.id),
            })
        })?;
        downloaded = downloaded.saturating_add(chunk_len);
        session_bytes = session_bytes.saturating_add(chunk_len);

        if last_emit.elapsed() >= PROGRESS_EMIT_INTERVAL {
            let speed = compute_speed_bps(session_bytes, started_at.elapsed());
            on_event(AiDownloadStage::Downloading, downloaded, speed, None);
            last_emit = Instant::now();
        }
    }

    file.flush().await.map_err(|e| {
        DownloadStepError::Fatal(AppError::Internal {
            message: format!("failed to flush partial file for {}: {e}", spec.id),
        })
    })?;

    if downloaded != spec.expected_bytes {
        return Err(DownloadStepError::Retryable(AppError::CorruptedData {
            message: format!(
                "incomplete download for {}: received {downloaded} / {} B",
                spec.id, spec.expected_bytes
            ),
        }));
    }

    let speed = compute_speed_bps(session_bytes, started_at.elapsed());
    on_event(AiDownloadStage::Downloading, downloaded, speed, None);
    Ok(())
}

#[derive(Debug, Clone, Copy)]
enum TransferMode {
    Full,
    Resumed { offset: u64 },
}

fn validate_response_lengths(
    response: &reqwest::Response,
    spec: &ModelAssetSpec,
    transfer: TransferMode,
) -> Result<(), DownloadStepError> {
    if let Some(cl_val) = response.headers().get(CONTENT_LENGTH)
        && let Ok(cl_str) = cl_val.to_str()
        && let Ok(cl) = cl_str.trim().parse::<u64>()
    {
        let expected_cl = match transfer {
            TransferMode::Resumed { offset } => spec.expected_bytes.saturating_sub(offset),
            TransferMode::Full => spec.expected_bytes,
        };
        if cl != expected_cl {
            return Err(DownloadStepError::Fatal(AppError::CorruptedData {
                message: format!(
                    "Content-Length mismatch for {}: expected {expected_cl} B, server reported {cl} B",
                    spec.id
                ),
            }));
        }
    }

    if matches!(transfer, TransferMode::Resumed { .. })
        && let Some(cr_val) = response.headers().get(CONTENT_RANGE)
        && let Ok(cr_str) = cr_val.to_str()
        && let Some((_, total_str)) = cr_str.rsplit_once('/')
        && let Ok(total) = total_str.trim().parse::<u64>()
        && total != spec.expected_bytes
    {
        return Err(DownloadStepError::Fatal(AppError::CorruptedData {
            message: format!(
                "Content-Range total mismatch for {}: expected {} B, got {total} B",
                spec.id, spec.expected_bytes
            ),
        }));
    }

    Ok(())
}

/// Streams a file on disk through SHA-256 and verifies exact byte length and lowercase hex digest.
///
/// # Errors
///
/// Returns [`AppError::CorruptedData`] if size or SHA-256 does not match.
pub(crate) async fn verify_file_sha256(
    path: &Path,
    expected_bytes: u64,
    expected_sha256_hex: &str,
) -> Result<(), AppError> {
    let path_owned = path.to_path_buf();
    let expected_hex = expected_sha256_hex.to_ascii_lowercase();
    tokio::task::spawn_blocking(move || {
        let mut file = std::fs::File::open(&path_owned).map_err(|e| AppError::CorruptedData {
            message: format!("failed to open file for SHA-256 verification: {e}"),
        })?;
        let mut hasher = Sha256::new();
        let mut buf = vec![0_u8; 64 * 1024];
        let mut total_read = 0_u64;
        loop {
            let n = file.read(&mut buf).map_err(|e| AppError::CorruptedData {
                message: format!("failed reading file during SHA-256 check: {e}"),
            })?;
            if n == 0 {
                break;
            }
            hasher.update(&buf[..n]);
            total_read = total_read.saturating_add(u64::try_from(n).unwrap_or(0));
        }

        if total_read != expected_bytes {
            return Err(AppError::CorruptedData {
                message: format!(
                    "verified size mismatch: expected {expected_bytes} B, got {total_read} B"
                ),
            });
        }

        let actual_hex = hex_lower(&hasher.finalize());
        if actual_hex != expected_hex {
            return Err(AppError::CorruptedData {
                message: format!(
                    "SHA-256 digest mismatch: expected {expected_hex}, computed {actual_hex}"
                ),
            });
        }
        Ok(())
    })
    .await
    .map_err(|e| AppError::Internal {
        message: format!("SHA-256 verification worker failed: {e}"),
    })?
}

/// Safely extracts `llama-mtmd-cli.exe`, `llama-server.exe`, and `*.dll` from a verified
/// `llama.cpp` Windows ZIP release with strict zip-slip protection and atomic directory promotion.
///
/// # Errors
///
/// Returns [`AppError::CorruptedData`] if the ZIP is invalid, attempts path traversal, fails
/// CRC-32 verification, or lacks `llama-mtmd-cli.exe`.
pub(crate) fn extract_llama_sidecar_zip(
    zip_path: &Path,
    target_dir: &Path,
) -> Result<(), AppError> {
    let mut file = std::fs::File::open(zip_path).map_err(|e| AppError::CorruptedData {
        message: format!("failed to open sidecar zip '{}': {e}", zip_path.display()),
    })?;
    let file_size = file
        .metadata()
        .map_err(|e| AppError::CorruptedData {
            message: format!("failed to stat sidecar zip: {e}"),
        })?
        .len();

    let entries = read_zip_entries(&mut file, file_size)?;

    let staging_dir = target_dir.with_extension("extracting");
    if staging_dir.exists() {
        let _ = std::fs::remove_dir_all(&staging_dir);
    }
    std::fs::create_dir_all(&staging_dir).map_err(|e| AppError::Internal {
        message: format!(
            "failed to create staging dir '{}': {e}",
            staging_dir.display()
        ),
    })?;

    let mut extracted_cli = false;
    let extract_result: Result<(), AppError> = (|| {
        for entry in &entries {
            let safe_file_name = validate_sidecar_zip_entry_name(&entry.name)?;
            let Some(file_name) = safe_file_name else {
                continue;
            };
            if !is_allowed_sidecar_binary(&file_name) {
                continue;
            }
            if entry.uncompressed_size > MAX_SIDECAR_ENTRY_BYTES
                || entry.compressed_size > MAX_SIDECAR_ENTRY_BYTES
            {
                return Err(AppError::CorruptedData {
                    message: format!("sidecar entry '{file_name}' exceeds safety size limit"),
                });
            }

            let payload = read_and_verify_zip_entry(&mut file, entry)?;
            let out_path = staging_dir.join(&file_name);
            let mut out_file =
                std::fs::File::create(&out_path).map_err(|e| AppError::Internal {
                    message: format!(
                        "failed to create sidecar file '{}': {e}",
                        out_path.display()
                    ),
                })?;
            out_file
                .write_all(&payload)
                .map_err(|e| AppError::Internal {
                    message: format!("failed to write sidecar file '{}': {e}", out_path.display()),
                })?;

            if file_name.eq_ignore_ascii_case("llama-mtmd-cli.exe")
                || file_name.eq_ignore_ascii_case("llama-server.exe")
            {
                extracted_cli = true;
            }
        }

        if !extracted_cli {
            return Err(AppError::CorruptedData {
                message: "llama.cpp archive did not contain llama-server.exe or llama-mtmd-cli.exe"
                    .to_owned(),
            });
        }
        Ok(())
    })();

    if let Err(err) = extract_result {
        let _ = std::fs::remove_dir_all(&staging_dir);
        return Err(err);
    }

    if let Some(parent) = target_dir.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            let _ = std::fs::remove_dir_all(&staging_dir);
            AppError::Internal {
                message: format!("failed to create sidecar parent dir: {e}"),
            }
        })?;
    }
    if target_dir.exists() {
        let _ = std::fs::remove_dir_all(target_dir);
    }
    std::fs::rename(&staging_dir, target_dir).map_err(|e| {
        let _ = std::fs::remove_dir_all(&staging_dir);
        AppError::Internal {
            message: format!("failed to finalize sidecar directory: {e}"),
        }
    })?;
    Ok(())
}

/// Validates a ZIP entry path against zip-slip traversal attacks and returns its flat basename
/// (or `None` for directory entries).
fn validate_sidecar_zip_entry_name(raw_name: &str) -> Result<Option<String>, AppError> {
    if raw_name.contains('\0') {
        return Err(AppError::CorruptedData {
            message: "ZIP archive contains NUL byte in entry name".to_owned(),
        });
    }
    let normalized = raw_name.replace('\\', "/");
    if normalized.starts_with('/') || normalized.contains(':') {
        return Err(AppError::CorruptedData {
            message: format!("ZIP entry uses forbidden absolute path: '{raw_name}'"),
        });
    }
    for comp in Path::new(&normalized).components() {
        match comp {
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(AppError::CorruptedData {
                    message: format!("ZIP archive attempted zip-slip path traversal: '{raw_name}'"),
                });
            }
            Component::CurDir | Component::Normal(_) => {}
        }
    }
    if normalized.ends_with('/') {
        return Ok(None);
    }
    let base = normalized.rsplit('/').next().unwrap_or("");
    if base.is_empty() || base == "." || base == ".." {
        return Ok(None);
    }
    Ok(Some(base.to_owned()))
}

fn is_allowed_sidecar_binary(file_name: &str) -> bool {
    let lower = file_name.to_ascii_lowercase();
    lower == "llama-mtmd-cli.exe"
        || lower == "llama-server.exe"
        || std::path::Path::new(&lower)
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("dll"))
}

#[derive(Debug)]
struct SidecarZipEntry {
    name: String,
    method: u16,
    local_header_offset: u64,
    compressed_size: u64,
    uncompressed_size: u64,
    crc32: u32,
}

fn read_zip_entries(
    file: &mut std::fs::File,
    file_size: u64,
) -> Result<Vec<SidecarZipEntry>, AppError> {
    if file_size < 22 {
        return Err(AppError::CorruptedData {
            message: "truncated ZIP archive".to_owned(),
        });
    }
    let tail_size = usize::try_from(file_size.min(EOCD_SEARCH_WINDOW)).unwrap_or(22);
    let tail_start = file_size.saturating_sub(u64::try_from(tail_size).unwrap_or(0));
    file.seek(SeekFrom::Start(tail_start))
        .map_err(|e| AppError::CorruptedData {
            message: format!("failed seeking ZIP EOCD: {e}"),
        })?;
    let mut tail = vec![0_u8; tail_size];
    file.read_exact(&mut tail)
        .map_err(|e| AppError::CorruptedData {
            message: format!("failed reading ZIP EOCD: {e}"),
        })?;

    let eocd_pos = (0..=tail.len().saturating_sub(22))
        .rev()
        .find(|&i| tail[i..i + 4] == [0x50, 0x4B, 0x05, 0x06])
        .ok_or_else(|| AppError::CorruptedData {
            message: "ZIP End of Central Directory marker missing".to_owned(),
        })?;

    let cd_size = u64::from(u32::from_le_bytes([
        tail[eocd_pos + 12],
        tail[eocd_pos + 13],
        tail[eocd_pos + 14],
        tail[eocd_pos + 15],
    ]));
    let cd_offset = u64::from(u32::from_le_bytes([
        tail[eocd_pos + 16],
        tail[eocd_pos + 17],
        tail[eocd_pos + 18],
        tail[eocd_pos + 19],
    ]));
    if cd_size > 16 * 1024 * 1024 || cd_offset.saturating_add(cd_size) > file_size {
        return Err(AppError::CorruptedData {
            message: "ZIP Central Directory bounds invalid".to_owned(),
        });
    }

    file.seek(SeekFrom::Start(cd_offset))
        .map_err(|e| AppError::CorruptedData {
            message: format!("failed seeking ZIP Central Directory: {e}"),
        })?;
    let cd_len = usize::try_from(cd_size).unwrap_or(0);
    let mut cd = vec![0_u8; cd_len];
    file.read_exact(&mut cd)
        .map_err(|e| AppError::CorruptedData {
            message: format!("failed reading ZIP Central Directory: {e}"),
        })?;

    Ok(parse_zip_central_directory(&cd))
}

fn parse_zip_central_directory(cd: &[u8]) -> Vec<SidecarZipEntry> {
    let mut out = Vec::new();
    let mut cursor = 0_usize;
    while cursor + 46 <= cd.len() {
        if cd[cursor..cursor + 4] != [0x50, 0x4B, 0x01, 0x02] {
            break;
        }
        let method = u16::from_le_bytes([cd[cursor + 10], cd[cursor + 11]]);
        let crc32 = u32::from_le_bytes([
            cd[cursor + 16],
            cd[cursor + 17],
            cd[cursor + 18],
            cd[cursor + 19],
        ]);
        let compressed_size = u64::from(u32::from_le_bytes([
            cd[cursor + 20],
            cd[cursor + 21],
            cd[cursor + 22],
            cd[cursor + 23],
        ]));
        let uncompressed_size = u64::from(u32::from_le_bytes([
            cd[cursor + 24],
            cd[cursor + 25],
            cd[cursor + 26],
            cd[cursor + 27],
        ]));
        let name_len = usize::from(u16::from_le_bytes([cd[cursor + 28], cd[cursor + 29]]));
        let extra_len = usize::from(u16::from_le_bytes([cd[cursor + 30], cd[cursor + 31]]));
        let comment_len = usize::from(u16::from_le_bytes([cd[cursor + 32], cd[cursor + 33]]));
        let local_header_offset = u64::from(u32::from_le_bytes([
            cd[cursor + 42],
            cd[cursor + 43],
            cd[cursor + 44],
            cd[cursor + 45],
        ]));

        let name_start = cursor + 46;
        let name_end = name_start.saturating_add(name_len);
        let next = name_end
            .saturating_add(extra_len)
            .saturating_add(comment_len);
        if next > cd.len() {
            break;
        }
        let name = String::from_utf8_lossy(&cd[name_start..name_end]).into_owned();
        out.push(SidecarZipEntry {
            name,
            method,
            local_header_offset,
            compressed_size,
            uncompressed_size,
            crc32,
        });
        cursor = next;
    }
    out
}

fn read_and_verify_zip_entry(
    file: &mut std::fs::File,
    entry: &SidecarZipEntry,
) -> Result<Vec<u8>, AppError> {
    file.seek(SeekFrom::Start(entry.local_header_offset))
        .map_err(|e| AppError::CorruptedData {
            message: format!("failed seeking ZIP local header: {e}"),
        })?;
    let mut local_hdr = [0_u8; 30];
    file.read_exact(&mut local_hdr)
        .map_err(|e| AppError::CorruptedData {
            message: format!("truncated ZIP local header: {e}"),
        })?;
    if local_hdr[0..4] != [0x50, 0x4B, 0x03, 0x04] {
        return Err(AppError::CorruptedData {
            message: "invalid ZIP local header signature".to_owned(),
        });
    }
    let name_len = u64::from(u16::from_le_bytes([local_hdr[26], local_hdr[27]]));
    let extra_len = u64::from(u16::from_le_bytes([local_hdr[28], local_hdr[29]]));
    let data_offset = entry
        .local_header_offset
        .saturating_add(30)
        .saturating_add(name_len)
        .saturating_add(extra_len);

    file.seek(SeekFrom::Start(data_offset))
        .map_err(|e| AppError::CorruptedData {
            message: format!("failed seeking ZIP payload: {e}"),
        })?;
    let comp_len = usize::try_from(entry.compressed_size).unwrap_or(0);
    let mut compressed = vec![0_u8; comp_len];
    file.read_exact(&mut compressed)
        .map_err(|e| AppError::CorruptedData {
            message: format!("truncated ZIP payload for '{}': {e}", entry.name),
        })?;

    let decoded = match entry.method {
        0 => compressed,
        8 => {
            let cap =
                usize::try_from(entry.uncompressed_size.min(MAX_SIDECAR_ENTRY_BYTES)).unwrap_or(0);
            let mut out = Vec::with_capacity(cap);
            DeflateDecoder::new(compressed.as_slice())
                .take(MAX_SIDECAR_ENTRY_BYTES.saturating_add(1))
                .read_to_end(&mut out)
                .map_err(|e| AppError::CorruptedData {
                    message: format!("deflate error extracting '{}': {e}", entry.name),
                })?;
            if u64::try_from(out.len()).unwrap_or(u64::MAX) > MAX_SIDECAR_ENTRY_BYTES {
                return Err(AppError::CorruptedData {
                    message: format!("decompressed '{}' exceeds size cap", entry.name),
                });
            }
            out
        }
        other => {
            return Err(AppError::UnsupportedFormat {
                message: format!("unsupported ZIP method {other} in sidecar archive"),
            });
        }
    };

    let mut crc = Crc32Hasher::new();
    crc.update(&decoded);
    if entry.crc32 != 0 && crc.finalize() != entry.crc32 {
        return Err(AppError::CorruptedData {
            message: format!("CRC-32 mismatch extracting '{}'", entry.name),
        });
    }
    Ok(decoded)
}

fn build_download_client(proxy: Option<&ProxyConfig>) -> Result<Client, AppError> {
    let mut builder = Client::builder()
        .connect_timeout(Duration::from_secs(20))
        .user_agent("RtMaga/3.3.1");

    if let Some(cfg) = proxy
        && cfg.enabled
    {
        let ep = &cfg.default_proxy;
        let host = ep.host.trim();
        if !host.is_empty() && ep.port > 0 {
            let scheme = ep.protocol.as_reqwest_scheme();
            let proxy_url = format!("{scheme}://{host}:{}", ep.port);
            let mut req_proxy = reqwest::Proxy::all(&proxy_url).map_err(|e| AppError::Network {
                message: format!("invalid proxy configuration for model download: {e}"),
            })?;
            if let Some(user) = ep.username.as_deref().filter(|s| !s.is_empty()) {
                let pass = ep.password.as_deref().unwrap_or("");
                req_proxy = req_proxy.basic_auth(user, pass);
            }
            builder = builder.proxy(req_proxy);
        }
    }

    builder.build().map_err(|e| AppError::Network {
        message: format!("failed to build model download HTTP client: {e}"),
    })
}

async fn atomic_replace_file(src: &Path, dest: &Path) -> Result<(), AppError> {
    if tokio::fs::rename(src, dest).await.is_ok() {
        return Ok(());
    }
    let _ = tokio::fs::remove_file(dest).await;
    tokio::fs::rename(src, dest)
        .await
        .map_err(|e| AppError::Internal {
            message: format!(
                "failed to promote verified model '{}' -> '{}': {e}",
                src.display(),
                dest.display()
            ),
        })
}

fn part_path_for(dest: &Path) -> PathBuf {
    let mut os = dest.as_os_str().to_os_string();
    os.push(".part");
    PathBuf::from(os)
}

fn file_matches_size(path: &Path, expected_bytes: u64) -> bool {
    std::fs::metadata(path).is_ok_and(|m| m.is_file() && m.len() == expected_bytes)
}

fn dir_size_sync(dir: &Path) -> u64 {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return 0;
    };
    let mut total = 0_u64;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            total = total.saturating_add(dir_size_sync(&path));
        } else if let Ok(meta) = entry.metadata() {
            total = total.saturating_add(meta.len());
        }
    }
    total
}

fn compute_speed_bps(bytes: u64, elapsed: Duration) -> u64 {
    let millis = u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX);
    if millis == 0 {
        return 0;
    }
    bytes.saturating_mul(1000) / millis
}

#[expect(
    clippy::too_many_arguments,
    reason = "maps fields directly onto AiDownloadProgressEvent DTO"
)]
fn make_progress(
    stage: AiDownloadStage,
    asset_id: &str,
    file_index: u32,
    file_count: u32,
    downloaded_bytes: u64,
    total_bytes: u64,
    speed_bytes_per_sec: u64,
    message: Option<String>,
) -> AiDownloadProgressEvent {
    let percent = if total_bytes == 0 {
        0.0_f32
    } else {
        let ratio =
            (u64_to_f64(downloaded_bytes.min(total_bytes)) / u64_to_f64(total_bytes)) * 100.0_f64;
        #[expect(
            clippy::as_conversions,
            clippy::cast_possible_truncation,
            reason = "ratio is bounded within 0.0..=100.0"
        )]
        {
            ratio.clamp(0.0, 100.0) as f32
        }
    };
    AiDownloadProgressEvent {
        stage,
        asset_id: asset_id.to_owned(),
        file_index,
        file_count,
        downloaded_bytes,
        total_bytes,
        percent,
        speed_bytes_per_sec,
        message,
    }
}

fn u64_to_f64(v: u64) -> f64 {
    #[expect(
        clippy::as_conversions,
        clippy::cast_precision_loss,
        reason = "progress percentages tolerate f64 precision loss above 2^53 bytes"
    )]
    {
        v as f64
    }
}

#[cfg(test)]
fn is_loopback_test_url(raw_url: &str) -> bool {
    url::Url::parse(raw_url)
        .is_ok_and(|u| u.scheme() == "http" && u.host_str() == Some("127.0.0.1"))
}

#[cfg(not(test))]
const fn is_loopback_test_url(_raw_url: &str) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    use super::*;
    use crate::services::ai::manifest::AiAssetKind;

    async fn spawn_range_mock_server(payload: Vec<u8>) -> String {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind ephemeral loopback port");
        let addr = listener.local_addr().expect("local addr");
        tokio::spawn(async move {
            while let Ok((mut sock, _)) = listener.accept().await {
                let data = payload.clone();
                tokio::spawn(async move {
                    let mut req_buf = [0_u8; 2048];
                    let Ok(n) = sock.read(&mut req_buf).await else {
                        return;
                    };
                    let req_str = String::from_utf8_lossy(&req_buf[..n]);
                    let range_start = req_str
                        .lines()
                        .find_map(|line| {
                            let lower = line.to_ascii_lowercase();
                            lower.strip_prefix("range: bytes=").and_then(|rest| {
                                rest.split('-').next()?.trim().parse::<usize>().ok()
                            })
                        })
                        .unwrap_or(0);

                    if range_start > 0 && range_start < data.len() {
                        let slice = &data[range_start..];
                        let header = format!(
                            "HTTP/1.1 206 Partial Content\r\nContent-Length: {}\r\nContent-Range: bytes {}-{}/{}\r\nConnection: close\r\n\r\n",
                            slice.len(),
                            range_start,
                            data.len() - 1,
                            data.len()
                        );
                        let _ = sock.write_all(header.as_bytes()).await;
                        let _ = sock.write_all(slice).await;
                    } else {
                        let header = format!(
                            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                            data.len()
                        );
                        let _ = sock.write_all(header.as_bytes()).await;
                        let _ = sock.write_all(&data).await;
                    }
                    let _ = sock.shutdown().await;
                });
            }
        });
        format!("http://{addr}/model.bin")
    }

    fn build_stored_zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut buf = Vec::new();
        let mut central = Vec::new();
        for (name, data) in entries {
            let local_offset = u32::try_from(buf.len()).expect("small zip");
            let mut crc_h = Crc32Hasher::new();
            crc_h.update(data);
            let crc = crc_h.finalize();
            let name_bytes = name.as_bytes();
            let data_len = u32::try_from(data.len()).expect("small entry");
            let name_len = u16::try_from(name_bytes.len()).expect("short name");

            buf.extend_from_slice(&[0x50, 0x4B, 0x03, 0x04]);
            buf.extend_from_slice(&20_u16.to_le_bytes());
            buf.extend_from_slice(&0_u16.to_le_bytes());
            buf.extend_from_slice(&0_u16.to_le_bytes()); // stored
            buf.extend_from_slice(&0_u16.to_le_bytes());
            buf.extend_from_slice(&0_u16.to_le_bytes());
            buf.extend_from_slice(&crc.to_le_bytes());
            buf.extend_from_slice(&data_len.to_le_bytes());
            buf.extend_from_slice(&data_len.to_le_bytes());
            buf.extend_from_slice(&name_len.to_le_bytes());
            buf.extend_from_slice(&0_u16.to_le_bytes());
            buf.extend_from_slice(name_bytes);
            buf.extend_from_slice(data);

            central.extend_from_slice(&[0x50, 0x4B, 0x01, 0x02]);
            central.extend_from_slice(&20_u16.to_le_bytes());
            central.extend_from_slice(&20_u16.to_le_bytes());
            central.extend_from_slice(&0_u16.to_le_bytes());
            central.extend_from_slice(&0_u16.to_le_bytes());
            central.extend_from_slice(&0_u16.to_le_bytes());
            central.extend_from_slice(&0_u16.to_le_bytes());
            central.extend_from_slice(&crc.to_le_bytes());
            central.extend_from_slice(&data_len.to_le_bytes());
            central.extend_from_slice(&data_len.to_le_bytes());
            central.extend_from_slice(&name_len.to_le_bytes());
            central.extend_from_slice(&0_u16.to_le_bytes());
            central.extend_from_slice(&0_u16.to_le_bytes());
            central.extend_from_slice(&0_u16.to_le_bytes());
            central.extend_from_slice(&0_u16.to_le_bytes());
            central.extend_from_slice(&0_u32.to_le_bytes());
            central.extend_from_slice(&local_offset.to_le_bytes());
            central.extend_from_slice(name_bytes);
        }

        let cd_offset = u32::try_from(buf.len()).expect("small zip");
        let cd_size = u32::try_from(central.len()).expect("small cd");
        let count = u16::try_from(entries.len()).expect("few entries");
        buf.extend_from_slice(&central);

        buf.extend_from_slice(&[0x50, 0x4B, 0x05, 0x06]);
        buf.extend_from_slice(&0_u16.to_le_bytes());
        buf.extend_from_slice(&0_u16.to_le_bytes());
        buf.extend_from_slice(&count.to_le_bytes());
        buf.extend_from_slice(&count.to_le_bytes());
        buf.extend_from_slice(&cd_size.to_le_bytes());
        buf.extend_from_slice(&cd_offset.to_le_bytes());
        buf.extend_from_slice(&0_u16.to_le_bytes());
        buf
    }

    #[tokio::test]
    async fn resumes_partial_download_and_rolls_back_on_sha256_mismatch() {
        let payload: Vec<u8> = (0_u32..4096)
            .map(|i| u8::try_from(i % 251).expect("mod 251 fits in u8"))
            .collect();
        let expected_sha = hex_lower(&Sha256::digest(&payload));
        let url_owned = spawn_range_mock_server(payload.clone()).await;
        let url_leaked: &'static str = Box::leak(url_owned.into_boxed_str());
        let sha_leaked: &'static str = Box::leak(expected_sha.into_boxed_str());

        let root = std::env::temp_dir().join(format!(
            "rtmaga_dl_test_{}_{}",
            std::process::id(),
            crate::repo::now_unix_secs()
        ));
        std::fs::create_dir_all(root.join("tier2")).expect("create test root");

        // Pre-seed first 1024 bytes into .part to exercise HTTP Range resume.
        let dest = root.join("tier2/test.gguf");
        let part = part_path_for(&dest);
        std::fs::write(&part, &payload[..1024]).expect("seed partial file");

        let spec = ModelAssetSpec {
            id: "test_asset",
            rel_path: "tier2/test.gguf",
            url: url_leaked,
            expected_bytes: u64::try_from(payload.len()).expect("len"),
            sha256_hex: sha_leaked,
            kind: AiAssetKind::Tier2Vlm,
            archive: AiArchiveFormat::RawFile,
        };

        let client = build_download_client(None).expect("build test client");
        let (_tx, rx) = watch::channel(false);
        let final_path = download_verified_asset(&client, &root, &spec, rx, |_, _, _, _| {})
            .await
            .expect("resumed download must pass SHA-256 and promote");
        assert_eq!(final_path, dest, "promoted to exact destination");
        assert!(!part.exists(), ".part file cleaned after atomic rename");
        assert_eq!(
            std::fs::read(&dest).expect("read final"),
            payload,
            "final file bytes match payload identically"
        );

        // Now test bad SHA-256 -> must fail and delete .part without touching dest2.
        let bad_spec = ModelAssetSpec {
            id: "bad_sha_asset",
            rel_path: "tier2/bad.gguf",
            url: url_leaked,
            expected_bytes: u64::try_from(payload.len()).expect("len"),
            sha256_hex: "ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff",
            kind: AiAssetKind::Tier2Vlm,
            archive: AiArchiveFormat::RawFile,
        };
        let (_tx2, rx2) = watch::channel(false);
        let err = download_verified_asset(&client, &root, &bad_spec, rx2, |_, _, _, _| {})
            .await
            .expect_err("wrong SHA-256 must fail");
        assert!(
            err.to_string().contains("SHA-256"),
            "error reports SHA-256 mismatch: {err}"
        );
        assert!(
            !root.join("tier2/bad.gguf").exists(),
            "bad file must never be promoted"
        );
        assert!(
            !part_path_for(&root.join("tier2/bad.gguf")).exists(),
            "bad .part must be rolled back immediately"
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn sidecar_zip_extractor_blocks_zip_slip_and_extracts_valid_binaries() {
        let root = std::env::temp_dir().join(format!(
            "rtmaga_zip_test_{}_{}",
            std::process::id(),
            crate::repo::now_unix_secs()
        ));
        std::fs::create_dir_all(&root).expect("create zip test dir");

        let evil_zip_path = root.join("evil.zip");
        let evil_bytes = build_stored_zip(&[
            ("../../evil.dll", b"malicious"),
            ("llama-mtmd-cli.exe", b"mz"),
        ]);
        std::fs::write(&evil_zip_path, evil_bytes).expect("write evil zip");
        let target_evil = root.join("bin_evil");
        let err = extract_llama_sidecar_zip(&evil_zip_path, &target_evil)
            .expect_err("zip-slip path traversal must be rejected");
        assert!(
            err.to_string().contains("zip-slip"),
            "error must report zip-slip: {err}"
        );
        assert!(!target_evil.exists(), "target dir not created on failure");
        assert!(
            !target_evil.with_extension("extracting").exists(),
            "staging dir rolled back on failure"
        );

        let good_zip_path = root.join("good.zip");
        let good_bytes = build_stored_zip(&[
            ("build/bin/llama-mtmd-cli.exe", b"cli-binary"),
            ("build/bin/ggml-vulkan.dll", b"vk-dll"),
            ("README.txt", b"ignored-text"),
        ]);
        std::fs::write(&good_zip_path, good_bytes).expect("write good zip");
        let target_good = root.join("bin_good");
        extract_llama_sidecar_zip(&good_zip_path, &target_good)
            .expect("valid llama.cpp sidecar zip extracts cleanly");
        assert!(
            target_good.join("llama-mtmd-cli.exe").is_file(),
            "llama-mtmd-cli.exe extracted"
        );
        assert!(
            target_good.join("ggml-vulkan.dll").is_file(),
            "DLL extracted"
        );
        assert!(
            !target_good.join("README.txt").exists(),
            "non-binary files skipped"
        );

        let _ = std::fs::remove_dir_all(&root);
    }
}
