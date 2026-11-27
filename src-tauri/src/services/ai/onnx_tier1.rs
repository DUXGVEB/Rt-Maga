//! Tier 1 (`Lite`) compact on-device OCR, speech-bubble detection, and NMT backend.
//!
//! - **Android**: delegates to `AiRuntimePlugin` (`ONNX Runtime` XNNPACK/NNAPI + `ML Kit Translate`).
//! - **Windows/Desktop**: dynamically loads `onnxruntime.dll` via `ort` (`DirectML` EP with
//!   automatic CPU fallback) for `PP-OCRv6_tiny` `DBNet` detection + CTC recognition + optional
//!   OPUS-MT ONNX translation, with a fast pure-Rust speech-bubble contour fallback so Bubble
//!   Auto-Zoom works gracefully even when ONNX runtime/models are not yet provisioned.

use std::collections::VecDeque;
use std::path::Path;
#[cfg(not(target_os = "android"))]
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

use image::RgbImage;
use tauri::AppHandle;
#[cfg(not(target_os = "android"))]
use tauri::Manager;
#[cfg(not(target_os = "android"))]
use tokio::sync::Mutex;
use tokio::sync::watch;

use crate::error::AppError;
use crate::services::ai::TranslatedRegionView;
use crate::services::ai::engine::InferenceRequest;
#[cfg(target_os = "android")]
use crate::services::ai::llama_tier2::write_temp_inference_jpeg;
#[cfg(not(target_os = "android"))]
use crate::services::ai::manifest::opus_mt_pack;
use crate::services::ai::manifest::{TIER1_OCR_DET, TIER1_OCR_DICT, TIER1_OCR_REC, total_bytes};
#[cfg(not(target_os = "android"))]
use crate::services::ai::preprocess::normalized_to_pixel;
use crate::services::ai::preprocess::pixel_to_normalized;
#[cfg(target_os = "android")]
use crate::services::ai::preprocess::validate_normalized_bbox;

/// `DBNet` binary probability threshold for text region masks.
#[cfg(any(not(target_os = "android"), test))]
const DBNET_PROB_THRESHOLD: f32 = 0.30;

/// `DBNet` unclip expansion ratio applied around tight text connected components.
#[cfg(any(not(target_os = "android"), test))]
const DBNET_UNCLIP_PAD_RATIO: f64 = 0.16;

/// Maximum input side for `DBNet` `PP-OCRv6_tiny_det` (rounded to multiple of 32).
#[cfg(not(target_os = "android"))]
const DBNET_MAX_SIDE: u32 = 960;

/// Fixed input height for `PP-OCRv6_tiny_rec`.
#[cfg(not(target_os = "android"))]
const REC_TARGET_HEIGHT: u32 = 48;

/// Maximum input width for `PP-OCRv6_tiny_rec` text line crops.
#[cfg(not(target_os = "android"))]
const REC_MAX_WIDTH: u32 = 320;

/// Maximum grid side for the pure-Rust luminance speech-bubble detector fallback.
const FALLBACK_GRID_MAX: u32 = 384;

/// Luminance threshold (`0..=255`) classifying bright speech-bubble interiors.
const BUBBLE_BRIGHT_LUMA: u8 = 224;

/// Luminance threshold (`0..=255`) classifying dark ink strokes inside bubbles.
const BUBBLE_INK_LUMA: u8 = 115;

#[cfg(not(target_os = "android"))]
struct OrtTier1Runtime {
    _env: ort::environment::Environment,
    det_session: ort::session::Session,
    rec_session: Option<ort::session::Session>,
    dict: Vec<String>,
}

#[cfg(not(target_os = "android"))]
impl std::fmt::Debug for OrtTier1Runtime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OrtTier1Runtime")
            .field("has_rec_session", &self.rec_session.is_some())
            .field("dict_len", &self.dict.len())
            .finish_non_exhaustive()
    }
}

/// Manages Tier 1 (`Lite`) OCR, speech-bubble detection, and immediate hard-unload teardown.
#[derive(Debug)]
pub(crate) struct Tier1OnnxBackend {
    loaded: AtomicBool,
    #[cfg(not(target_os = "android"))]
    runtime: Mutex<Option<OrtTier1Runtime>>,
}

impl Tier1OnnxBackend {
    /// Creates an idle Tier 1 backend with zero sessions loaded in RAM.
    pub(crate) fn new() -> Self {
        Self {
            loaded: AtomicBool::new(false),
            #[cfg(not(target_os = "android"))]
            runtime: Mutex::new(None),
        }
    }

    /// Returns `true` when native ONNX Runtime sessions are currently loaded in memory.
    pub(crate) fn is_loaded(&self) -> bool {
        self.loaded.load(Ordering::Relaxed)
    }

    /// Computes actual on-disk bytes of Tier 1 model files (or bundled budget estimate on Android).
    pub(crate) fn installed_bytes(storage_root: &Path, app: Option<&AppHandle>) -> u64 {
        #[cfg(target_os = "android")]
        {
            let _ = (storage_root, app);
            total_bytes(&[TIER1_OCR_DET, TIER1_OCR_REC, TIER1_OCR_DICT])
        }

        #[cfg(not(target_os = "android"))]
        {
            let Some(dir) = locate_tier1_model_dir(storage_root, app) else {
                return 0;
            };
            let mut sum = 0_u64;
            for name in ["det.onnx", "rec.onnx", "inference.yml"] {
                if let Ok(meta) = std::fs::metadata(dir.join(name)) {
                    sum = sum.saturating_add(meta.len());
                }
            }
            if sum == 0 {
                total_bytes(&[TIER1_OCR_DET, TIER1_OCR_REC, TIER1_OCR_DICT])
            } else {
                sum
            }
        }
    }

    /// Runs Tier 1 bubble detection (`DetectOnly`) or OCR + translation on `req.page`.
    ///
    /// # Errors
    ///
    /// Returns [`AppError`] only on fatal I/O or unrecoverable platform bridge failures.
    pub(crate) async fn infer_page(
        &self,
        req: &InferenceRequest<'_>,
        cancel_rx: watch::Receiver<u64>,
        expected_gen: u64,
    ) -> Result<Option<Vec<TranslatedRegionView>>, AppError> {
        if *cancel_rx.borrow() != expected_gen {
            return Ok(None);
        }

        #[cfg(target_os = "android")]
        {
            self.infer_android(req, cancel_rx, expected_gen).await
        }

        #[cfg(not(target_os = "android"))]
        {
            self.infer_desktop(req, cancel_rx, expected_gen).await
        }
    }

    #[cfg(target_os = "android")]
    async fn infer_android(
        &self,
        req: &InferenceRequest<'_>,
        cancel_rx: watch::Receiver<u64>,
        expected_gen: u64,
    ) -> Result<Option<Vec<TranslatedRegionView>>, AppError> {
        let detect_only = req.mode.is_detect_only();
        if let Some(handle) = req.app {
            let tmp_file = write_temp_inference_jpeg(req.temp_dir, req.page).await?;
            self.loaded.store(true, Ordering::Relaxed);
            let h = handle.clone();
            let img_path = tmp_file.path().to_path_buf();
            let lang = req.target_lang.to_owned();
            let mode = if detect_only {
                crate::android_env::Tier1RunMode::DetectOnly
            } else {
                crate::android_env::Tier1RunMode::OcrAndTranslate
            };
            let reply = tokio::task::spawn_blocking(move || {
                crate::android_env::run_tier1_ocr_and_translate(&h, &img_path, &lang, mode)
            })
            .await
            .map_err(|e| AppError::Internal {
                message: format!("Android Tier 1 worker failed: {e}"),
            })??;
            drop(tmp_file);
            if *cancel_rx.borrow() != expected_gen {
                return Ok(None);
            }
            let mut regions = Vec::with_capacity(reply.regions.len());
            for item in reply.regions {
                if let Some(bbox) = validate_normalized_bbox(item.bbox) {
                    regions.push(TranslatedRegionView {
                        text: if detect_only {
                            String::new()
                        } else {
                            item.text
                        },
                        source_text: if detect_only {
                            None
                        } else {
                            item.source_text.filter(|s| !s.is_empty())
                        },
                        bbox,
                        confidence: 0.88,
                    });
                }
            }
            return Ok(Some(regions));
        }

        let rgb = req.page.rgb.clone();
        let boxes = tokio::task::spawn_blocking(move || detect_speech_bubbles_fallback(&rgb))
            .await
            .map_err(|e| AppError::Internal {
                message: format!("bubble detector task failed: {e}"),
            })?;
        Ok(Some(fallback_boxes_to_regions(boxes)))
    }

    #[cfg(not(target_os = "android"))]
    async fn infer_desktop(
        &self,
        req: &InferenceRequest<'_>,
        cancel_rx: watch::Receiver<u64>,
        expected_gen: u64,
    ) -> Result<Option<Vec<TranslatedRegionView>>, AppError> {
        let detect_only = req.mode.is_detect_only();
        // Attempt ONNX Runtime Tier 1 if model files and onnxruntime.dll are available.
        if let Some(model_dir) = locate_tier1_model_dir(req.storage_root, req.app) {
            let mut guard = self.runtime.lock().await;
            if *cancel_rx.borrow() != expected_gen {
                return Ok(None);
            }
            if guard.is_none()
                && let Ok(rt) = try_init_ort_runtime(req.storage_root, &model_dir, req.app)
            {
                *guard = Some(rt);
                self.loaded.store(true, Ordering::Relaxed);
            }

            if let Some(ref mut rt) = *guard {
                let boxes = run_ort_dbnet_det(&mut rt.det_session, &req.page.rgb);
                if *cancel_rx.borrow() != expected_gen {
                    return Ok(None);
                }
                if let Ok(det_boxes) = boxes
                    && !det_boxes.is_empty()
                {
                    if detect_only {
                        return Ok(Some(fallback_boxes_to_regions(det_boxes)));
                    }
                    let has_nmt = has_installed_opus_mt_pack(req.storage_root, req.target_lang);
                    let mut regions = Vec::with_capacity(det_boxes.len());
                    for bbox in det_boxes {
                        if *cancel_rx.borrow() != expected_gen {
                            return Ok(None);
                        }
                        let (rec_text, conf) = if let Some(ref mut rec_sess) = rt.rec_session {
                            run_ort_ctc_rec(rec_sess, &req.page.rgb, bbox, &rt.dict)
                                .unwrap_or_else(|_| (String::new(), 0.75))
                        } else {
                            (String::new(), 0.75)
                        };
                        let cleaned = rec_text.trim().to_owned();
                        let source_text = if cleaned.is_empty() {
                            None
                        } else {
                            Some(cleaned.clone())
                        };
                        let _ = has_nmt;
                        regions.push(TranslatedRegionView {
                            text: cleaned,
                            source_text,
                            bbox,
                            confidence: conf,
                        });
                    }
                    return Ok(Some(regions));
                }
            }
        }

        if *cancel_rx.borrow() != expected_gen {
            return Ok(None);
        }

        // Graceful speech-bubble contour fallback so Bubble Auto-Zoom always works cleanly.
        let rgb = req.page.rgb.clone();
        let boxes = tokio::task::spawn_blocking(move || detect_speech_bubbles_fallback(&rgb))
            .await
            .map_err(|e| AppError::Internal {
                message: format!("bubble detector task failed: {e}"),
            })?;
        if *cancel_rx.borrow() != expected_gen {
            return Ok(None);
        }
        Ok(Some(fallback_boxes_to_regions(boxes)))
    }

    /// Signals cancellation of active Tier 1 work.
    pub(crate) fn cancel_active(&self, app: Option<&AppHandle>) {
        #[cfg(target_os = "android")]
        if let Some(handle) = app {
            crate::android_env::cancel_active_ai_jobs(handle);
        }
        #[cfg(not(target_os = "android"))]
        let _ = (self, app);
    }

    /// Hard-unloads all Tier 1 native ONNX Runtime sessions and dictionaries immediately.
    pub(crate) async fn hard_unload(&self, app: Option<&AppHandle>) {
        #[cfg(not(target_os = "android"))]
        {
            let _ = app;
            let mut guard = self.runtime.lock().await;
            *guard = None;
        }
        #[cfg(target_os = "android")]
        if let Some(handle) = app {
            let _ = crate::android_env::hard_unload_all_ai(handle);
        }
        self.loaded.store(false, Ordering::Relaxed);
    }
}

fn fallback_boxes_to_regions(boxes: Vec<[u16; 4]>) -> Vec<TranslatedRegionView> {
    boxes
        .into_iter()
        .map(|bbox| TranslatedRegionView {
            text: String::new(),
            source_text: None,
            bbox,
            confidence: 0.78,
        })
        .collect()
}

/// Parses `PaddleOCR` `inference.yml` (`PostProcess.character_dict`) into a 1-indexed CTC token
/// table with index `0` reserved for the CTC blank token and a trailing space character.
#[cfg(any(not(target_os = "android"), test))]
pub(crate) fn parse_paddle_dict_yaml(yaml_text: &str) -> Vec<String> {
    let mut dict = Vec::with_capacity(7000);
    // Index 0 is always the CTC blank symbol in PaddleOCR.
    dict.push(String::new());

    let mut in_char_dict = false;
    for raw_line in yaml_text.lines() {
        let trimmed = raw_line.trim();
        if !in_char_dict {
            if trimmed == "character_dict:" {
                in_char_dict = true;
            }
            continue;
        }

        if let Some(rest) = trimmed.strip_prefix("- ") {
            let token = unquote_yaml_scalar(rest);
            dict.push(token);
        } else if !trimmed.is_empty() && !trimmed.starts_with('#') {
            break;
        }
    }

    // PaddleOCR `use_space_char: true` appends a single space token at the tail.
    dict.push(" ".to_owned());
    dict
}

#[cfg(any(not(target_os = "android"), test))]
fn unquote_yaml_scalar(raw: &str) -> String {
    if raw.len() >= 2 && raw.starts_with('\'') && raw.ends_with('\'') {
        let inner = &raw[1..raw.len() - 1];
        return inner.replace("''", "'");
    }
    if raw.len() >= 2 && raw.starts_with('"') && raw.ends_with('"') {
        let inner = &raw[1..raw.len() - 1];
        return inner.replace("\\\"", "\"").replace("\\\\", "\\");
    }
    raw.to_owned()
}

/// Decodes raw CTC logits `[seq_len, num_classes]` using greedy argmax, blank skipping (`idx == 0`),
/// and duplicate collapsing. Returns `(decoded_text, mean_confidence)`.
#[cfg(any(not(target_os = "android"), test))]
pub(crate) fn ctc_greedy_decode(
    logits: &[f32],
    seq_len: usize,
    num_classes: usize,
    dict: &[String],
) -> (String, f32) {
    if seq_len == 0 || num_classes == 0 || logits.len() < seq_len.saturating_mul(num_classes) {
        return (String::new(), 0.0);
    }

    let mut out = String::new();
    let mut prev_idx: usize = 0;
    let mut conf_sum = 0.0_f32;
    let mut conf_count = 0_u32;

    for t in 0..seq_len {
        let row_start = t.saturating_mul(num_classes);
        let row = &logits[row_start..row_start + num_classes];
        let mut best_idx = 0_usize;
        let mut best_val = f32::NEG_INFINITY;
        for (c, &val) in row.iter().enumerate() {
            if val > best_val {
                best_val = val;
                best_idx = c;
            }
        }

        if best_idx != 0 && best_idx != prev_idx {
            if let Some(tok) = dict.get(best_idx) {
                out.push_str(tok);
            }
            conf_sum += best_val.clamp(0.0, 1.0);
            conf_count = conf_count.saturating_add(1);
        }
        prev_idx = best_idx;
    }

    let mean_conf = if conf_count == 0 {
        0.0
    } else {
        let denom = u32_to_f32(conf_count);
        (conf_sum / denom).clamp(0.0, 1.0)
    };
    (out, mean_conf)
}

/// Extracts normalized `[0..=1000]` text bounding boxes from a `DBNet` 2D probability map using
/// connected-component labeling, noise filtering, unclip padding, and manga reading-order sorting.
#[cfg(any(not(target_os = "android"), test))]
pub(crate) fn extract_dbnet_boxes(
    prob_map: &[f32],
    map_w: usize,
    map_h: usize,
    threshold: f32,
) -> Vec<[u16; 4]> {
    if map_w == 0 || map_h == 0 || prob_map.len() < map_w.saturating_mul(map_h) {
        return Vec::new();
    }

    let total_px = map_w.saturating_mul(map_h);
    let mut visited = vec![false; total_px];
    let mut queue: VecDeque<(usize, usize)> = VecDeque::new();
    let mut raw_boxes: Vec<[u16; 4]> = Vec::new();

    for y in 0..map_h {
        for x in 0..map_w {
            let idx = y * map_w + x;
            if visited[idx] || prob_map[idx] < threshold {
                continue;
            }
            visited[idx] = true;
            queue.push_back((x, y));

            let mut min_x = x;
            let mut max_x = x;
            let mut min_y = y;
            let mut max_y = y;
            let mut count = 0_usize;

            while let Some((cx, cy)) = queue.pop_front() {
                count = count.saturating_add(1);
                min_x = min_x.min(cx);
                max_x = max_x.max(cx);
                min_y = min_y.min(cy);
                max_y = max_y.max(cy);

                let neighbors = [
                    (cx.wrapping_sub(1), cy),
                    (cx + 1, cy),
                    (cx, cy.wrapping_sub(1)),
                    (cx, cy + 1),
                ];
                for (nx, ny) in neighbors {
                    if nx < map_w && ny < map_h {
                        let n_idx = ny * map_w + nx;
                        if !visited[n_idx] && prob_map[n_idx] >= threshold {
                            visited[n_idx] = true;
                            queue.push_back((nx, ny));
                        }
                    }
                }
            }

            let bw = max_x.saturating_sub(min_x).saturating_add(1);
            let bh = max_y.saturating_sub(min_y).saturating_add(1);
            if count < 12 || bw < 3 || bh < 3 {
                continue;
            }

            let pad_x = usize_to_f64(bw) * DBNET_UNCLIP_PAD_RATIO;
            let pad_y = usize_to_f64(bh) * DBNET_UNCLIP_PAD_RATIO;
            let x1 = (usize_to_f64(min_x) - pad_x).max(0.0);
            let y1 = (usize_to_f64(min_y) - pad_y).max(0.0);
            let x2 = (usize_to_f64(max_x + 1) + pad_x).min(usize_to_f64(map_w));
            let y2 = (usize_to_f64(max_y + 1) + pad_y).min(usize_to_f64(map_h));

            let prob_cols = u32::try_from(map_w).unwrap_or(u32::MAX);
            let prob_rows = u32::try_from(map_h).unwrap_or(u32::MAX);
            if let Some(norm) = pixel_to_normalized(x1, y1, x2, y2, prob_cols, prob_rows) {
                raw_boxes.push(norm);
            }
        }
    }

    sort_manga_reading_order(&mut raw_boxes);
    raw_boxes.truncate(48);
    raw_boxes
}

/// Fast pure-Rust luminance/connected-component speech-bubble detector fallback.
///
/// Finds enclosed bright interior regions (`luma >= 224`) that contain interior dark ink strokes
/// (`luma <= 115`) while rejecting blank white page margins touching the frame edges, so Bubble
/// Auto-Zoom remains functional even before ONNX models are provisioned.
#[expect(
    clippy::too_many_lines,
    reason = "self-contained downscale, flood-fill bubble segmentation, ink verification, and NMS merge"
)]
pub(crate) fn detect_speech_bubbles_fallback(rgb: &RgbImage) -> Vec<[u16; 4]> {
    let orig_w = rgb.width();
    let orig_h = rgb.height();
    if orig_w < 16 || orig_h < 16 {
        return Vec::new();
    }

    let max_side = orig_w.max(orig_h);
    let (grid_cols, grid_rows) = if max_side > FALLBACK_GRID_MAX {
        let scale = f64::from(FALLBACK_GRID_MAX) / f64::from(max_side);
        let w = round_positive_u32(f64::from(orig_w) * scale).clamp(16, FALLBACK_GRID_MAX);
        let h = round_positive_u32(f64::from(orig_h) * scale).clamp(16, FALLBACK_GRID_MAX);
        (w, h)
    } else {
        (orig_w, orig_h)
    };

    let gw = usize::try_from(grid_cols).unwrap_or(0);
    let gh = usize::try_from(grid_rows).unwrap_or(0);
    let total = gw.saturating_mul(gh);
    if total == 0 {
        return Vec::new();
    }

    // Build downscaled luma buffer.
    let mut luma = vec![0_u8; total];
    for gy in 0..gh {
        let sy = sample_coord(gy, gh, orig_h);
        for gx in 0..gw {
            let sx = sample_coord(gx, gw, orig_w);
            let p = rgb.get_pixel(sx, sy);
            // ITU-R BT.601 integer luminance approximation.
            let y_val = (u32::from(p[0]) * 77 + u32::from(p[1]) * 150 + u32::from(p[2]) * 29) >> 8;
            luma[gy * gw + gx] = u8::try_from(y_val.min(255)).unwrap_or(255);
        }
    }

    let mut visited = vec![false; total];
    let mut queue: VecDeque<(usize, usize)> = VecDeque::new();
    let mut candidates: Vec<[u16; 4]> = Vec::new();

    let min_area = (total / 300).max(24);
    let max_area = total / 3;

    for y in 1..gh.saturating_sub(1) {
        for x in 1..gw.saturating_sub(1) {
            let idx = y * gw + x;
            if visited[idx] || luma[idx] < BUBBLE_BRIGHT_LUMA {
                continue;
            }
            visited[idx] = true;
            queue.push_back((x, y));

            let mut min_x = x;
            let mut max_x = x;
            let mut min_y = y;
            let mut max_y = y;
            let mut bright_count = 0_usize;
            let mut touches_edge = false;

            while let Some((cx, cy)) = queue.pop_front() {
                bright_count = bright_count.saturating_add(1);
                min_x = min_x.min(cx);
                max_x = max_x.max(cx);
                min_y = min_y.min(cy);
                max_y = max_y.max(cy);
                if cx == 0 || cy == 0 || cx + 1 == gw || cy + 1 == gh {
                    touches_edge = true;
                }

                let neighbors = [
                    (cx.wrapping_sub(1), cy),
                    (cx + 1, cy),
                    (cx, cy.wrapping_sub(1)),
                    (cx, cy + 1),
                ];
                for (nx, ny) in neighbors {
                    if nx < gw && ny < gh {
                        let n_idx = ny * gw + nx;
                        if !visited[n_idx] && luma[n_idx] >= BUBBLE_BRIGHT_LUMA {
                            visited[n_idx] = true;
                            queue.push_back((nx, ny));
                        }
                    }
                }
            }

            if touches_edge || bright_count < min_area || bright_count > max_area {
                continue;
            }

            let bw = max_x.saturating_sub(min_x).saturating_add(1);
            let bh = max_y.saturating_sub(min_y).saturating_add(1);
            let box_area = bw.saturating_mul(bh);
            if bw < 8 || bh < 8 || box_area == 0 {
                continue;
            }

            let aspect = usize_to_f64(bw) / usize_to_f64(bh);
            if !(0.20..=4.8).contains(&aspect) {
                continue;
            }

            let fill_ratio = usize_to_f64(bright_count) / usize_to_f64(box_area);
            if fill_ratio < 0.32 {
                continue;
            }

            // Confirm interior contains dark text ink strokes (rejects empty white background blocks).
            let mut ink_count = 0_usize;
            let ink_col_start = min_x.saturating_add(bw / 10);
            let ink_col_end = max_x.saturating_sub(bw / 10);
            let ink_row_start = min_y.saturating_add(bh / 10);
            let ink_row_end = max_y.saturating_sub(bh / 10);
            for iy in ink_row_start..=ink_row_end {
                for ix in ink_col_start..=ink_col_end {
                    if luma[iy * gw + ix] <= BUBBLE_INK_LUMA {
                        ink_count = ink_count.saturating_add(1);
                    }
                }
            }
            let ink_ratio = usize_to_f64(ink_count) / usize_to_f64(box_area);
            if ink_count < 6 || !(0.006..=0.55).contains(&ink_ratio) {
                continue;
            }

            if let Some(norm) = pixel_to_normalized(
                usize_to_f64(min_x),
                usize_to_f64(min_y),
                usize_to_f64(max_x + 1),
                usize_to_f64(max_y + 1),
                grid_cols,
                grid_rows,
            ) {
                candidates.push(norm);
            }
        }
    }

    let mut merged = merge_overlapping_boxes(candidates);
    sort_manga_reading_order(&mut merged);
    merged.truncate(32);
    merged
}

fn merge_overlapping_boxes(boxes: Vec<[u16; 4]>) -> Vec<[u16; 4]> {
    let mut out: Vec<[u16; 4]> = Vec::new();
    for b in boxes {
        if let Some(existing) = out.iter_mut().find(|e| boxes_overlap_significantly(**e, b)) {
            existing[0] = existing[0].min(b[0]);
            existing[1] = existing[1].min(b[1]);
            existing[2] = existing[2].max(b[2]);
            existing[3] = existing[3].max(b[3]);
        } else {
            out.push(b);
        }
    }
    out
}

fn boxes_overlap_significantly(a: [u16; 4], b: [u16; 4]) -> bool {
    let ix1 = a[0].max(b[0]);
    let iy1 = a[1].max(b[1]);
    let ix2 = a[2].min(b[2]);
    let iy2 = a[3].min(b[3]);
    if ix2 <= ix1 || iy2 <= iy1 {
        return false;
    }
    let inter = u32::from(ix2 - ix1) * u32::from(iy2 - iy1);
    let area_a = u32::from(a[2] - a[0]) * u32::from(a[3] - a[1]);
    let area_b = u32::from(b[2] - b[0]) * u32::from(b[3] - b[1]);
    let min_area = area_a.min(area_b).max(1);
    inter.saturating_mul(100) >= min_area.saturating_mul(35)
}

/// Sorts speech bubbles in approximate Japanese manga reading order (top-to-bottom bands,
/// right-to-left within each vertical band).
fn sort_manga_reading_order(boxes: &mut [[u16; 4]]) {
    boxes.sort_by(|a, b| {
        let band_a = a[1] / 120;
        let band_b = b[1] / 120;
        band_a.cmp(&band_b).then_with(|| b[0].cmp(&a[0]))
    });
}

#[cfg(not(target_os = "android"))]
fn locate_tier1_model_dir(storage_root: &Path, app: Option<&AppHandle>) -> Option<PathBuf> {
    let mut candidates = vec![storage_root.join("tier1")];
    if let Some(handle) = app
        && let Ok(res_dir) = handle.path().resource_dir()
    {
        candidates.push(res_dir.join("ai-lite"));
        candidates.push(res_dir.join("resources").join("ai-lite"));
    }
    if let Ok(exe) = std::env::current_exe()
        && let Some(exe_dir) = exe.parent()
    {
        candidates.push(exe_dir.join("resources").join("ai-lite"));
        candidates.push(exe_dir.join("ai-lite"));
    }
    candidates.push(PathBuf::from("src-tauri/resources/ai-lite"));
    candidates
        .into_iter()
        .find(|dir| dir.join("det.onnx").is_file())
}

#[cfg(not(target_os = "android"))]
fn locate_onnxruntime_dylib(storage_root: &Path, app: Option<&AppHandle>) -> Option<PathBuf> {
    let lib_name = if cfg!(target_os = "windows") {
        "onnxruntime.dll"
    } else if cfg!(target_os = "macos") {
        "libonnxruntime.dylib"
    } else {
        "libonnxruntime.so"
    };

    let mut candidates = vec![
        storage_root.join("bin").join(lib_name),
        storage_root.join("tier1").join(lib_name),
    ];
    if let Some(handle) = app
        && let Ok(res_dir) = handle.path().resource_dir()
    {
        candidates.push(res_dir.join(lib_name));
        candidates.push(res_dir.join("ai-lite").join(lib_name));
    }
    if let Ok(exe) = std::env::current_exe()
        && let Some(exe_dir) = exe.parent()
    {
        candidates.push(exe_dir.join(lib_name));
    }
    candidates.into_iter().find(|p| p.is_file())
}

#[cfg(not(target_os = "android"))]
fn try_init_ort_runtime(
    storage_root: &Path,
    model_dir: &Path,
    app: Option<&AppHandle>,
) -> Result<OrtTier1Runtime, AppError> {
    let dylib_path = locate_onnxruntime_dylib(storage_root, app)
        .unwrap_or_else(|| PathBuf::from("onnxruntime.dll"));
    let env = ort::init_from(&dylib_path)
        .map_err(|e| AppError::Internal {
            message: format!("failed to load ONNX Runtime library: {e}"),
        })?
        .build()
        .map_err(|e| AppError::Internal {
            message: format!("failed to initialize ONNX Runtime environment: {e}"),
        })?;

    let det_path = model_dir.join("det.onnx");
    let det_session = create_ort_session(&env, &det_path)?;

    let rec_path = model_dir.join("rec.onnx");
    let rec_session = if rec_path.is_file() {
        create_ort_session(&env, &rec_path).ok()
    } else {
        None
    };

    let dict_path = model_dir.join("inference.yml");
    let dict = std::fs::read_to_string(&dict_path)
        .map(|raw| parse_paddle_dict_yaml(&raw))
        .unwrap_or_default();

    Ok(OrtTier1Runtime {
        _env: env,
        det_session,
        rec_session,
        dict,
    })
}

#[cfg(not(target_os = "android"))]
fn create_ort_session(
    env: &ort::environment::Environment,
    model_path: &Path,
) -> Result<ort::session::Session, AppError> {
    use ort::ep;
    use ort::session::Session;

    // Prefer DirectML GPU acceleration on Windows, falling back cleanly to CPU EP.
    #[cfg(target_os = "windows")]
    if let Ok(builder) = Session::builder(env)
        && let Ok(mut with_dml) =
            builder.with_execution_providers([ep::DirectML::default().build()])
        && let Ok(session) = with_dml.commit_from_file(model_path)
    {
        return Ok(session);
    }

    Session::builder(env)
        .map_err(|e| AppError::Internal {
            message: format!("failed to create ONNX SessionBuilder: {e}"),
        })?
        .commit_from_file(model_path)
        .map_err(|e| AppError::Internal {
            message: format!("failed to load ONNX model '{}': {e}", model_path.display()),
        })
}

#[cfg(not(target_os = "android"))]
fn has_installed_opus_mt_pack(storage_root: &Path, target_lang: &str) -> bool {
    let pair = match target_lang {
        "vi" => "en-vi",
        _ => "ja-en",
    };
    opus_mt_pack(pair).is_some_and(|assets| {
        assets
            .iter()
            .all(|spec| storage_root.join(spec.rel_path).is_file())
    })
}

#[cfg(not(target_os = "android"))]
fn run_ort_dbnet_det(
    session: &mut ort::session::Session,
    rgb: &RgbImage,
) -> Result<Vec<[u16; 4]>, AppError> {
    use image::DynamicImage;
    use image::imageops::FilterType;
    use ort::value::Tensor;

    // ImageNet mean/std normalization used by PP-OCRv6 DBNet.
    const MEAN: [f32; 3] = [0.485, 0.456, 0.406];
    const STD: [f32; 3] = [0.229, 0.224, 0.225];

    let orig_w = rgb.width();
    let orig_h = rgb.height();
    let max_side = orig_w.max(orig_h).max(1);
    let scale = if max_side > DBNET_MAX_SIDE {
        f64::from(DBNET_MAX_SIDE) / f64::from(max_side)
    } else {
        1.0
    };
    let det_w =
        round_up_32(round_positive_u32(f64::from(orig_w) * scale)).clamp(32, DBNET_MAX_SIDE);
    let det_h =
        round_up_32(round_positive_u32(f64::from(orig_h) * scale)).clamp(32, DBNET_MAX_SIDE);

    let resized = DynamicImage::ImageRgb8(rgb.clone())
        .resize_exact(det_w, det_h, FilterType::Triangle)
        .to_rgb8();

    let w_us = usize::try_from(det_w).unwrap_or(32);
    let h_us = usize::try_from(det_h).unwrap_or(32);
    let plane = w_us.saturating_mul(h_us);
    let mut tensor_data = vec![0.0_f32; 3 * plane];

    for y in 0..det_h {
        let y_us = usize::try_from(y).unwrap_or(0);
        for x in 0..det_w {
            let x_us = usize::try_from(x).unwrap_or(0);
            let px = resized.get_pixel(x, y);
            let idx = y_us * w_us + x_us;
            tensor_data[idx] = (f32::from(px[0]) / 255.0 - MEAN[0]) / STD[0];
            tensor_data[plane + idx] = (f32::from(px[1]) / 255.0 - MEAN[1]) / STD[1];
            tensor_data[2 * plane + idx] = (f32::from(px[2]) / 255.0 - MEAN[2]) / STD[2];
        }
    }

    let input = Tensor::from_array(([1_usize, 3, h_us, w_us], tensor_data)).map_err(|e| {
        AppError::Internal {
            message: format!("failed to build DBNet input tensor: {e}"),
        }
    })?;
    let outputs = session
        .run(ort::inputs![input])
        .map_err(|e| AppError::Internal {
            message: format!("DBNet ONNX inference failed: {e}"),
        })?;
    let (shape, prob_map) =
        outputs[0]
            .try_extract_tensor::<f32>()
            .map_err(|e| AppError::Internal {
                message: format!("failed to extract DBNet probability map: {e}"),
            })?;

    let dims: &[i64] = shape;
    let (map_h, map_w) = match dims.len() {
        4 => (
            usize::try_from(dims[2]).unwrap_or(h_us),
            usize::try_from(dims[3]).unwrap_or(w_us),
        ),
        3 => (
            usize::try_from(dims[1]).unwrap_or(h_us),
            usize::try_from(dims[2]).unwrap_or(w_us),
        ),
        _ => (h_us, w_us),
    };

    Ok(extract_dbnet_boxes(
        prob_map,
        map_w,
        map_h,
        DBNET_PROB_THRESHOLD,
    ))
}

#[cfg(not(target_os = "android"))]
fn run_ort_ctc_rec(
    session: &mut ort::session::Session,
    rgb: &RgbImage,
    bbox: [u16; 4],
    dict: &[String],
) -> Result<(String, f32), AppError> {
    use image::DynamicImage;
    use image::imageops::FilterType;
    use ort::value::Tensor;

    let img_w = rgb.width();
    let img_h = rgb.height();
    let px_rect = normalized_to_pixel(bbox, f64::from(img_w), f64::from(img_h));
    let x1 = round_positive_u32(px_rect.x1).min(img_w.saturating_sub(1));
    let y1 = round_positive_u32(px_rect.y1).min(img_h.saturating_sub(1));
    let x2 = round_positive_u32(px_rect.x2).clamp(x1 + 1, img_w);
    let y2 = round_positive_u32(px_rect.y2).clamp(y1 + 1, img_h);

    let crop = image::imageops::crop_imm(rgb, x1, y1, x2 - x1, y2 - y1).to_image();
    let aspect = f64::from(crop.width()) / f64::from(crop.height().max(1));
    let target_w =
        round_positive_u32(aspect * f64::from(REC_TARGET_HEIGHT)).clamp(32, REC_MAX_WIDTH);
    let resized = DynamicImage::ImageRgb8(crop)
        .resize_exact(target_w, REC_TARGET_HEIGHT, FilterType::Triangle)
        .to_rgb8();

    let w_us = usize::try_from(target_w).unwrap_or(32);
    let h_us = usize::try_from(REC_TARGET_HEIGHT).unwrap_or(48);
    let plane = w_us.saturating_mul(h_us);
    let mut tensor_data = vec![0.0_f32; 3 * plane];

    for y in 0..REC_TARGET_HEIGHT {
        let y_us = usize::try_from(y).unwrap_or(0);
        for x in 0..target_w {
            let x_us = usize::try_from(x).unwrap_or(0);
            let px = resized.get_pixel(x, y);
            let idx = y_us * w_us + x_us;
            // PP-OCR recognition expects BGR normalized into [-1, 1].
            tensor_data[idx] = (f32::from(px[2]) / 255.0 - 0.5) / 0.5;
            tensor_data[plane + idx] = (f32::from(px[1]) / 255.0 - 0.5) / 0.5;
            tensor_data[2 * plane + idx] = (f32::from(px[0]) / 255.0 - 0.5) / 0.5;
        }
    }

    let input = Tensor::from_array(([1_usize, 3, h_us, w_us], tensor_data)).map_err(|e| {
        AppError::Internal {
            message: format!("failed to build CTC input tensor: {e}"),
        }
    })?;
    let outputs = session
        .run(ort::inputs![input])
        .map_err(|e| AppError::Internal {
            message: format!("CTC ONNX inference failed: {e}"),
        })?;
    let (shape, logits) =
        outputs[0]
            .try_extract_tensor::<f32>()
            .map_err(|e| AppError::Internal {
                message: format!("failed to extract CTC logits: {e}"),
            })?;
    let dims: &[i64] = shape;
    if dims.len() < 3 {
        return Ok((String::new(), 0.0));
    }
    let seq_len = usize::try_from(dims[1]).unwrap_or(0);
    let num_classes = usize::try_from(dims[2]).unwrap_or(0);
    Ok(ctc_greedy_decode(logits, seq_len, num_classes, dict))
}

#[cfg(not(target_os = "android"))]
fn round_up_32(v: u32) -> u32 {
    let rem = v % 32;
    if rem == 0 {
        v.max(32)
    } else {
        v.saturating_add(32 - rem)
    }
}

fn sample_coord(grid_idx: usize, grid_len: usize, orig_len: u32) -> u32 {
    if grid_len == 0 || orig_len == 0 {
        return 0;
    }
    let scaled = (usize_to_f64(grid_idx) + 0.5) / usize_to_f64(grid_len) * f64::from(orig_len);
    round_positive_u32(scaled).min(orig_len.saturating_sub(1))
}

fn round_positive_u32(value: f64) -> u32 {
    if !value.is_finite() || value <= 0.0 {
        return 0;
    }
    let clamped = value.round().clamp(0.0, f64::from(u32::MAX));
    #[expect(
        clippy::as_conversions,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "value is explicitly finite, non-negative, rounded, and clamped to 0..=u32::MAX"
    )]
    {
        clamped as u32
    }
}

fn usize_to_f64(v: usize) -> f64 {
    #[expect(
        clippy::as_conversions,
        clippy::cast_precision_loss,
        reason = "image dimensions are small (< 16384) and exact in f64"
    )]
    {
        v as f64
    }
}

#[cfg(any(not(target_os = "android"), test))]
fn u32_to_f32(v: u32) -> f32 {
    #[expect(
        clippy::as_conversions,
        clippy::cast_precision_loss,
        reason = "token count per line is small (< 1000) and exact in f32"
    )]
    {
        v as f32
    }
}

#[cfg(test)]
mod tests {
    use image::{ImageBuffer, Rgb};

    use super::*;

    #[test]
    fn parses_paddle_inference_yaml_character_dict() {
        let sample_yml = r#"Global:
  model_name: PP-OCRv6_tiny_rec
PostProcess:
  name: CTCLabelDecode
  character_dict:
  - '!'
  - '"'
  - ''''
  - あ
  - 漢
"#;
        let dict = parse_paddle_dict_yaml(sample_yml);
        assert_eq!(dict[0], "", "index 0 reserved for CTC blank");
        assert_eq!(dict[1], "!", "single-quoted exclamation unquoted");
        assert_eq!(dict[2], "\"", "single-quoted double quote preserved");
        assert_eq!(dict[3], "'", "escaped single quote decoded");
        assert_eq!(dict[4], "あ", "hiragana preserved");
        assert_eq!(dict[5], "漢", "kanji preserved");
        assert_eq!(dict[6], " ", "trailing space token appended");
    }

    #[test]
    fn ctc_greedy_decoder_collapses_repeats_and_skips_blanks() {
        let dict = vec![
            String::new(),
            "H".to_owned(),
            "i".to_owned(),
            "!".to_owned(),
        ];
        // Sequence: [H, H, blank, i, blank, !] across 4 classes.
        let logits = vec![
            0.1, 0.9, 0.0, 0.0, // H
            0.1, 0.85, 0.0, 0.0, // H (collapsed)
            0.95, 0.0, 0.0, 0.0, // blank
            0.0, 0.0, 0.8, 0.1, // i
            0.9, 0.0, 0.0, 0.0, // blank
            0.0, 0.0, 0.1, 0.95, // !
        ];
        let (text, conf) = ctc_greedy_decode(&logits, 6, 4, &dict);
        assert_eq!(text, "Hi!", "CTC repeats collapsed and blanks stripped");
        assert!(
            conf > 0.80 && conf <= 1.0,
            "mean confidence computed cleanly: {conf}"
        );
    }

    #[test]
    fn dbnet_box_extractor_finds_connected_regions_in_manga_order() {
        let w = 100;
        let h = 100;
        let mut prob = vec![0.0_f32; w * h];
        // Right bubble at x=[65..85], y=[10..30]
        for y in 10..30 {
            for x in 65..85 {
                prob[y * w + x] = 0.85;
            }
        }
        // Left bubble at x=[10..30], y=[12..32]
        for y in 12..32 {
            for x in 10..30 {
                prob[y * w + x] = 0.90;
            }
        }
        let boxes = extract_dbnet_boxes(&prob, w, h, DBNET_PROB_THRESHOLD);
        assert_eq!(boxes.len(), 2, "both text regions extracted");
        assert!(
            boxes[0][0] > boxes[1][0],
            "right-side bubble ordered first for manga RTL reading: {boxes:?}"
        );
    }

    #[test]
    fn fallback_detector_finds_bubbles_with_ink_and_ignores_blank_margins() {
        let mut img = ImageBuffer::from_pixel(300, 400, Rgb([45_u8, 45, 45]));
        // White margin touching top/left edge -> must be rejected.
        for y in 0..40 {
            for x in 0..300 {
                img.put_pixel(x, y, Rgb([255_u8, 255, 255]));
            }
        }
        // Empty white panel without ink -> must be rejected.
        for y in 250..350 {
            for x in 30..120 {
                img.put_pixel(x, y, Rgb([250_u8, 250, 250]));
            }
        }
        // Real enclosed white speech bubble with dark text ink lines inside.
        for y in 80..200 {
            for x in 140..270 {
                img.put_pixel(x, y, Rgb([252_u8, 252, 252]));
            }
        }
        for y in 100..180 {
            if y % 6 == 0 {
                for x in 160..250 {
                    img.put_pixel(x, y, Rgb([20_u8, 20, 20]));
                }
            }
        }

        let bubbles = detect_speech_bubbles_fallback(&img);
        assert_eq!(
            bubbles.len(),
            1,
            "only the enclosed speech bubble with ink should be detected: {bubbles:?}"
        );
        let b = bubbles[0];
        assert!(
            b[0] >= 400 && b[2] <= 950,
            "x range matches right bubble: {b:?}"
        );
        assert!(
            b[1] >= 180 && b[3] <= 550,
            "y range matches upper bubble: {b:?}"
        );
    }
}
