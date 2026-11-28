//! Image preprocessing (`<= 1024px` proportional downscale), `[0..1000]` normalized bounding-box
//! coordinate conversions, and strict compact JSON extraction for VLM translation outputs.

use std::io::Cursor;

use image::imageops::FilterType;
use image::{DynamicImage, GenericImageView, ImageFormat, RgbImage};
use serde::Deserialize;

use crate::error::AppError;
use crate::services::ai::TranslatedRegionView;

/// Maximum pixel dimension (width or height) sent to any on-device AI model.
pub(crate) const MAX_AI_IMAGE_DIM: u32 = 1024;

/// Upper safety limit on input page dimensions to reject decompression bombs.
const MAX_DECODABLE_DIM: u32 = 16_384;

/// Maximum normalized bounding-box coordinate value (`[0..=1000]`).
pub(crate) const NORMALIZED_COORD_MAX: u16 = 1000;

/// Decoded and proportionally downscaled manga page ready for Tier 1 or Tier 2 inference.
#[derive(Debug, Clone)]
pub(crate) struct PreprocessedPage {
    /// RGB image downscaled so `max(width, height) <= 1024`.
    pub rgb: RgbImage,
    /// Original unscaled page width in pixels.
    pub original_width: u32,
    /// Original unscaled page height in pixels.
    pub original_height: u32,
}

/// Axis-aligned rectangle in native pixel canvas coordinates.
#[cfg(any(not(target_os = "android"), test))]
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct PixelRect {
    pub x1: f64,
    pub y1: f64,
    pub x2: f64,
    pub y2: f64,
}

/// Decodes raw image bytes (`JPEG`, `PNG`, `WebP`, `GIF`, `BMP`) and downscales proportionally so
/// `max(width, height) <= 1024px`.
///
/// # Errors
///
/// Returns [`AppError::InvalidImageFormat`] or [`AppError::CorruptedData`] if `bytes` cannot be
/// decoded or has invalid dimensions.
pub(crate) fn preprocess_image_bytes(bytes: &[u8]) -> Result<PreprocessedPage, AppError> {
    if bytes.is_empty() {
        return Err(AppError::CorruptedData {
            message: "empty page image payload".to_owned(),
        });
    }

    let decoded = image::load_from_memory(bytes).map_err(|e| AppError::InvalidImageFormat {
        message: format!("failed to decode page image for AI: {e}"),
    })?;

    preprocess_dynamic_image(&decoded)
}

/// Proportionally downscales an already-decoded [`DynamicImage`] to `max(width, height) <= 1024px`.
///
/// # Errors
///
/// Returns [`AppError::InvalidImageFormat`] if image dimensions are zero or exceed safety bounds.
pub(crate) fn preprocess_dynamic_image(
    decoded: &DynamicImage,
) -> Result<PreprocessedPage, AppError> {
    let (orig_w, orig_h) = decoded.dimensions();
    if orig_w == 0 || orig_h == 0 || orig_w > MAX_DECODABLE_DIM || orig_h > MAX_DECODABLE_DIM {
        return Err(AppError::InvalidImageFormat {
            message: format!("unsupported page image dimensions: {orig_w}x{orig_h}"),
        });
    }

    let (target_w, target_h) = compute_downscaled_dims(orig_w, orig_h, MAX_AI_IMAGE_DIM);
    let rgb = if target_w == orig_w && target_h == orig_h {
        decoded.to_rgb8()
    } else {
        decoded
            .resize_exact(target_w, target_h, FilterType::Triangle)
            .to_rgb8()
    };

    Ok(PreprocessedPage {
        rgb,
        original_width: orig_w,
        original_height: orig_h,
    })
}

/// Computes proportional dimensions with `max(width, height) <= max_dim` and both sides `>= 1`.
pub(crate) fn compute_downscaled_dims(width: u32, height: u32, max_dim: u32) -> (u32, u32) {
    let max_side = width.max(height);
    if max_side <= max_dim || max_dim == 0 {
        return (width.max(1), height.max(1));
    }
    let scale = f64::from(max_dim) / f64::from(max_side);
    let scaled_w = round_positive_u32(f64::from(width) * scale).clamp(1, max_dim);
    let scaled_h = round_positive_u32(f64::from(height) * scale).clamp(1, max_dim);
    (scaled_w, scaled_h)
}

/// Encodes a preprocessed RGB page into compact JPEG bytes suitable for VLM / mobile bridges.
///
/// # Errors
///
/// Returns [`AppError::Internal`] if JPEG encoding fails.
pub(crate) fn encode_jpeg_for_inference(rgb: &RgbImage) -> Result<Vec<u8>, AppError> {
    let mut out = Cursor::new(Vec::with_capacity(64 * 1024));
    let dyn_img = DynamicImage::ImageRgb8(rgb.clone());
    dyn_img
        .write_to(&mut out, ImageFormat::Jpeg)
        .map_err(|e| AppError::Internal {
            message: format!("failed to encode JPEG for AI inference: {e}"),
        })?;
    Ok(out.into_inner())
}

/// Maps a pixel bounding box `(x1, y1, x2, y2)` inside an image of size `(img_w, img_h)` to a
/// clamped `[0..=1000]` normalized `[x1, y1, x2, y2]` box. Returns `None` if degenerate.
pub(crate) fn pixel_to_normalized(
    x1: f64,
    y1: f64,
    x2: f64,
    y2: f64,
    img_w: u32,
    img_h: u32,
) -> Option<[u16; 4]> {
    if img_w == 0 || img_h == 0 {
        return None;
    }
    let w = f64::from(img_w);
    let h = f64::from(img_h);
    let nx1 = clamp_norm_u16((x1 / w) * 1000.0);
    let ny1 = clamp_norm_u16((y1 / h) * 1000.0);
    let nx2 = clamp_norm_u16((x2 / w) * 1000.0);
    let ny2 = clamp_norm_u16((y2 / h) * 1000.0);
    validate_normalized_bbox([nx1, ny1, nx2, ny2])
}

/// Maps a `[0..=1000]` normalized bounding box back into native pixel coordinates on a canvas of
/// size `(canvas_w, canvas_h)`.
#[cfg(any(not(target_os = "android"), test))]
pub(crate) fn normalized_to_pixel(bbox: [u16; 4], canvas_w: f64, canvas_h: f64) -> PixelRect {
    let w = canvas_w.max(0.0);
    let h = canvas_h.max(0.0);
    let x1 = (f64::from(bbox[0].min(NORMALIZED_COORD_MAX)) / 1000.0) * w;
    let y1 = (f64::from(bbox[1].min(NORMALIZED_COORD_MAX)) / 1000.0) * h;
    let x2 = (f64::from(bbox[2].min(NORMALIZED_COORD_MAX)) / 1000.0) * w;
    let y2 = (f64::from(bbox[3].min(NORMALIZED_COORD_MAX)) / 1000.0) * h;
    PixelRect { x1, y1, x2, y2 }
}

/// Clamps coordinates to `0..=1000` and rejects degenerate boxes where `x2 <= x1` or `y2 <= y1`.
pub(crate) fn validate_normalized_bbox(raw: [u16; 4]) -> Option<[u16; 4]> {
    let x1 = raw[0].min(NORMALIZED_COORD_MAX);
    let y1 = raw[1].min(NORMALIZED_COORD_MAX);
    let x2 = raw[2].min(NORMALIZED_COORD_MAX);
    let y2 = raw[3].min(NORMALIZED_COORD_MAX);
    if x2 > x1 && y2 > y1 {
        Some([x1, y1, x2, y2])
    } else {
        None
    }
}

#[derive(Debug, Deserialize)]
struct CompactRegionPayload {
    t: Option<String>,
    b: Option<[f64; 4]>,
}

/// Extracts and validates the strict compact JSON schema
/// `[{"t":"translated_text","b":[x1,y1,x2,y2]}]` from raw VLM text output.
///
/// Tolerates surrounding markdown code fences or prose, clamps coordinates to `0..=1000`, enforces
/// `x2 > x1` and `y2 > y1`, and drops malformed or empty entries safely without failing the page.
pub(crate) fn parse_vlm_compact_json(raw: &str) -> Vec<TranslatedRegionView> {
    let cleaned = strip_think_blocks(raw);
    let Some(array_slice) = extract_first_json_array(&cleaned) else {
        return Vec::new();
    };

    let Ok(values) = serde_json::from_str::<Vec<serde_json::Value>>(array_slice) else {
        return Vec::new();
    };

    let mut regions = Vec::with_capacity(values.len());
    for value in values {
        let Ok(item) = serde_json::from_value::<CompactRegionPayload>(value) else {
            continue;
        };
        let Some(raw_text) = item.t else {
            continue;
        };
        let text = raw_text.trim();
        if text.is_empty() {
            continue;
        }
        let Some(coords) = item.b else {
            continue;
        };
        if !coords.iter().all(|c| c.is_finite()) {
            continue;
        }
        let clamped = [
            clamp_norm_u16(coords[0]),
            clamp_norm_u16(coords[1]),
            clamp_norm_u16(coords[2]),
            clamp_norm_u16(coords[3]),
        ];
        let Some(bbox) = validate_normalized_bbox(clamped) else {
            continue;
        };
        regions.push(TranslatedRegionView {
            text: text.to_owned(),
            source_text: None,
            bbox,
            confidence: 0.92,
        });
    }
    regions
}

/// Strips Qwen3 `<think>...</think>` reasoning spans so stray brackets inside thought traces
/// never confuse the JSON array extractor.
fn strip_think_blocks(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut rest = raw;
    while let Some(start) = rest.find("<think>") {
        out.push_str(&rest[..start]);
        let after = &rest[start + "<think>".len()..];
        if let Some(end) = after.find("</think>") {
            rest = &after[end + "</think>".len()..];
        } else {
            return out;
        }
    }
    out.push_str(rest);
    out
}

/// Finds the first syntactically balanced JSON array `[...]` in `raw`, respecting JSON string
/// literals and escape sequences.
fn extract_first_json_array(raw: &str) -> Option<&str> {
    let bytes = raw.as_bytes();
    let mut start_idx = None;
    let mut depth: usize = 0;
    let mut in_string = false;
    let mut escaped = false;

    for (idx, &b) in bytes.iter().enumerate() {
        if in_string {
            if escaped {
                escaped = false;
            } else if b == b'\\' {
                escaped = true;
            } else if b == b'"' {
                in_string = false;
            }
            continue;
        }

        match b {
            b'"' if start_idx.is_some() => in_string = true,
            b'[' => {
                if start_idx.is_none() {
                    start_idx = Some(idx);
                }
                depth = depth.saturating_add(1);
            }
            b']' if start_idx.is_some() => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    let start = start_idx?;
                    return raw.get(start..=idx);
                }
            }
            _ => {}
        }
    }
    None
}

fn clamp_norm_u16(value: f64) -> u16 {
    if !value.is_finite() || value <= 0.0 {
        return 0;
    }
    if value >= f64::from(NORMALIZED_COORD_MAX) {
        return NORMALIZED_COORD_MAX;
    }
    let rounded = round_positive_u32(value);
    u16::try_from(rounded).unwrap_or(NORMALIZED_COORD_MAX)
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

#[cfg(test)]
mod tests {
    use image::{ImageBuffer, Rgb};

    use super::*;

    fn make_png_bytes(width: u32, height: u32) -> Vec<u8> {
        let img = ImageBuffer::from_pixel(width, height, Rgb([240_u8, 240, 240]));
        let mut cursor = Cursor::new(Vec::new());
        DynamicImage::ImageRgb8(img)
            .write_to(&mut cursor, ImageFormat::Png)
            .expect("test PNG encodes cleanly");
        cursor.into_inner()
    }

    #[test]
    fn downscales_large_images_proportionally_and_preserves_small_images() {
        let portrait = make_png_bytes(1600, 2400);
        let pre = preprocess_image_bytes(&portrait).expect("portrait PNG decodes");
        assert_eq!(pre.original_width, 1600, "original width recorded");
        assert_eq!(pre.original_height, 2400, "original height recorded");
        assert_eq!(pre.rgb.height(), 1024, "longest side clamped to 1024");
        assert_eq!(
            pre.rgb.width(),
            683,
            "portrait width preserves 2:3 aspect ratio"
        );

        let landscape = make_png_bytes(2048, 1024);
        let pre_land = preprocess_image_bytes(&landscape).expect("landscape PNG decodes");
        assert_eq!(pre_land.rgb.width(), 1024, "landscape width clamped");
        assert_eq!(pre_land.rgb.height(), 512, "landscape height scaled");

        let small = make_png_bytes(800, 600);
        let pre_small = preprocess_image_bytes(&small).expect("small PNG decodes");
        assert_eq!(pre_small.rgb.width(), 800, "small image not upscaled");
        assert_eq!(pre_small.rgb.height(), 600, "small image not upscaled");
    }

    #[test]
    fn normalized_bbox_round_trips_and_rejects_degenerate_boxes() {
        let norm = pixel_to_normalized(100.0, 200.0, 500.0, 800.0, 1000, 2000)
            .expect("valid pixel box normalizes");
        assert_eq!(norm, [100, 100, 500, 400], "exact 0..1000 mapping");

        let px = normalized_to_pixel(norm, 2000.0, 4000.0);
        assert!((px.x1 - 200.0).abs() < 0.01, "x1 maps to canvas");
        assert!((px.y1 - 400.0).abs() < 0.01, "y1 maps to canvas");
        assert!((px.x2 - 1000.0).abs() < 0.01, "x2 maps to canvas");
        assert!((px.y2 - 1600.0).abs() < 0.01, "y2 maps to canvas");

        assert!(
            pixel_to_normalized(500.0, 100.0, 500.0, 200.0, 1000, 1000).is_none(),
            "zero-width box must be rejected"
        );
        assert!(
            validate_normalized_bbox([400, 300, 200, 500]).is_none(),
            "inverted x coordinates must be rejected"
        );
    }

    #[test]
    fn strict_compact_json_extractor_clamps_and_drops_malformed_entries() {
        let noisy_vlm = r#"Sure! Here is the translation:
```json
[
  {"t": "Watch out! [Danger]", "b": [-20, 45.4, 1200, 310.6]},
  {"t": "   ", "b": [10, 10, 100, 100]},
  {"t": "Invalid inverted", "b": [500, 500, 400, 600]},
  {"bad": 123},
  {"t": "Run now!", "b": [610, 400, 890, 520]}
]
```
Hope that helps."#;

        let parsed = parse_vlm_compact_json(noisy_vlm);
        assert_eq!(parsed.len(), 2, "only the 2 valid regions survive");
        assert_eq!(
            parsed[0].text, "Watch out! [Danger]",
            "brackets inside string literal must not truncate array extraction"
        );
        assert_eq!(
            parsed[0].bbox,
            [0, 45, 1000, 311],
            "out-of-range coords clamp cleanly into 0..=1000"
        );
        assert_eq!(parsed[1].text, "Run now!", "second valid region preserved");
        assert_eq!(parsed[1].bbox, [610, 400, 890, 520], "second bbox exact");

        assert!(
            parse_vlm_compact_json("no json array here").is_empty(),
            "non-JSON text returns empty list cleanly"
        );
    }
}
