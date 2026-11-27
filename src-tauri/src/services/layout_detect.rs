//! Fast, non-blocking multi-signal Webtoon vs. Manga layout auto-detection.
//!
//! Classification precedence:
//! 1. **Manual per-manga override** (`reader_mode_override` or `reader_settings_json`) always wins.
//! 2. **Metadata priority** (`genres`, `tags`, and source site hints such as `webtoon`, `manhwa`,
//!    `manhua`, `long strip`, `full color`).
//! 3. **Header-only dimension sniffing** on the first 2–4 pages via `imagesize::blob_size`
//!    (reads header bytes only inside `tokio::task::spawn_blocking`, zero full pixel decode):
//!    triggers Vertical Webtoon mode if median aspect ratio `>= 1.8` or a tall strip (`>= 2.2`)
//!    exhibits width continuity (`±8%`) with another qualifying page; otherwise Manga paged mode.

use serde::Serialize;
use url::Url;

use crate::cache::ImageCache;
use crate::cache::image::CacheEntryMeta;
use crate::error::AppError;
use crate::image_proxy::verify_image_token;
use crate::state::AppState;

/// Maximum header bytes probed per page for dimension sniffing (`64 KiB`).
const HEADER_SNIFF_MAX_BYTES: usize = 64 * 1024;

/// Maximum number of leading chapter pages sniffed when metadata is inconclusive.
const MAX_SNIFF_PAGES: usize = 4;

/// Classified layout verdict returned to the Svelte reader.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub(crate) enum DetectedLayout {
    Webtoon,
    Manga,
    ManualOverride,
    Undecided,
}

/// Signal source that produced [`DetectedLayout`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub(crate) enum DetectionSignal {
    Override,
    Metadata,
    Dimensions,
    None,
}

/// Complete layout classification response delivered asynchronously to the reader.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LayoutDetectionResult {
    pub layout: DetectedLayout,
    pub signal: DetectionSignal,
    pub decided: bool,
    pub is_webtoon: bool,
    pub median_ratio: f64,
    pub sampled_pages: usize,
}

/// Metadata tokens that strongly indicate a vertical long-strip / Webtoon title.
const WEBTOON_METADATA_KEYWORDS: &[&str] = &[
    "webtoon",
    "manhwa",
    "manhua",
    "long strip",
    "long-strip",
    "longstrip",
    "full color",
    "full-color",
    "fullcolor",
    "web comic",
    "webcomic",
    "vertical scroll",
    "truyen mau",
    "truyện màu",
];

/// Source hostname hints strongly associated with vertical long-strip publishers.
const WEBTOON_SITE_HINTS: &[&str] = &["webtoons", "toomics", "lezhin", "manhwa", "toon"];

/// Evaluates manga genres, tags, and source hostname hints for immediate metadata classification.
pub(crate) fn classify_from_metadata(
    genres: &[String],
    tags: &[String],
    site: &str,
) -> Option<DetectedLayout> {
    let matches_keyword = |raw: &str| {
        let lower = raw.trim().to_ascii_lowercase();
        if lower.is_empty() {
            return false;
        }
        WEBTOON_METADATA_KEYWORDS
            .iter()
            .any(|kw| lower == *kw || lower.contains(kw))
    };

    if genres.iter().any(|g| matches_keyword(g)) || tags.iter().any(|t| matches_keyword(t)) {
        return Some(DetectedLayout::Webtoon);
    }

    let site_lower = site.trim().to_ascii_lowercase();
    if !site_lower.starts_with("local:")
        && WEBTOON_SITE_HINTS
            .iter()
            .any(|hint| site_lower.contains(hint))
    {
        return Some(DetectedLayout::Webtoon);
    }

    None
}

/// Classifies Webtoon vs. Manga paged layout purely from natural `(width, height)` pairs.
///
/// Returns `(decided, is_webtoon, median_ratio)`:
/// - Ignores tiny icons/banners (`width < 200` or `height < 300`) and landscape double-page spreads
///   (`width > height * 1.15`).
/// - Requires at least 2 qualifying pages.
/// - Triggers Vertical Webtoon mode if `median_ratio >= 1.8` OR a tall strip (`ratio >= 2.2`)
///   shares matching width continuity (within `±8%`) with at least one other qualifying page.
pub(crate) fn classify_from_dimensions(dims: &[(u32, u32)]) -> (bool, bool, f64) {
    let mut qualifying: Vec<(u32, f64)> = Vec::with_capacity(dims.len());
    for &(w, h) in dims {
        if w < 200 || h < 300 {
            continue;
        }
        if u64::from(w) * 100 > u64::from(h) * 115 {
            continue;
        }
        let ratio = f64::from(h) / f64::from(w);
        qualifying.push((w, ratio));
    }

    if qualifying.len() < 2 {
        return (false, false, 0.0);
    }

    let mut ratios: Vec<f64> = qualifying.iter().map(|&(_, r)| r).collect();
    ratios.sort_by(f64::total_cmp);
    let mid = ratios.len() / 2;
    let median_ratio = if ratios.len() % 2 == 1 {
        ratios[mid]
    } else {
        f64::midpoint(ratios[mid - 1], ratios[mid])
    };

    let tall_with_width_continuity = qualifying.iter().enumerate().any(|(i, &(w_a, r_a))| {
        if r_a < 2.2 {
            return false;
        }
        qualifying.iter().enumerate().any(|(j, &(w_b, _))| {
            if i == j {
                return false;
            }
            let max_w = f64::from(w_a.max(w_b));
            let diff = f64::from(w_a.abs_diff(w_b));
            diff <= max_w * 0.08
        })
    });

    let is_webtoon = median_ratio >= 1.8 || tall_with_width_continuity;
    let rounded_median = (median_ratio * 100.0).round() / 100.0;
    (true, is_webtoon, rounded_median)
}

/// Runs multi-signal Webtoon vs. Manga classification asynchronously without blocking initial paint.
///
/// # Errors
///
/// Returns [`AppError::Db`] on SQLite failure.
pub(crate) async fn detect_chapter_layout(
    state: &AppState,
    manga_id: i64,
    page_tokens: &[String],
) -> Result<LayoutDetectionResult, AppError> {
    let Some(row) = state.db.get_manga_by_id(manga_id).await? else {
        return Ok(LayoutDetectionResult {
            layout: DetectedLayout::Undecided,
            signal: DetectionSignal::None,
            decided: false,
            is_webtoon: false,
            median_ratio: 0.0,
            sampled_pages: 0,
        });
    };

    // Rule 0: Manual per-manga reader override ALWAYS takes precedence over auto-detection.
    if row.reader_mode_override.is_some() || row.reader_settings_json.is_some() {
        return Ok(LayoutDetectionResult {
            layout: DetectedLayout::ManualOverride,
            signal: DetectionSignal::Override,
            decided: true,
            is_webtoon: false,
            median_ratio: 0.0,
            sampled_pages: 0,
        });
    }

    // Rule 1: Metadata priority (zero latency, zero extra I/O).
    let genres: Vec<String> = row
        .genres_json
        .as_deref()
        .and_then(|raw| serde_json::from_str(raw).ok())
        .unwrap_or_default();
    let tags: Vec<String> = row
        .tags_json
        .as_deref()
        .and_then(|raw| serde_json::from_str(raw).ok())
        .unwrap_or_default();

    if let Some(DetectedLayout::Webtoon) = classify_from_metadata(&genres, &tags, &row.site) {
        if row.detected_webtoon == 0 {
            let _ = state.db.mark_manga_detected_webtoon(manga_id).await;
        }
        return Ok(LayoutDetectionResult {
            layout: DetectedLayout::Webtoon,
            signal: DetectionSignal::Metadata,
            decided: true,
            is_webtoon: true,
            median_ratio: 0.0,
            sampled_pages: 0,
        });
    }

    // Rule 2: Header-only dimension sniffing of the first 2–4 chapter pages via `imagesize`.
    let tokens_to_probe: Vec<&str> = page_tokens
        .iter()
        .take(MAX_SNIFF_PAGES)
        .map(String::as_str)
        .collect();
    let mut dims: Vec<(u32, u32)> = Vec::with_capacity(tokens_to_probe.len());

    for token in tokens_to_probe {
        if let Some((w, h)) = sniff_single_page_header_dims(state, token).await {
            dims.push((w, h));
        }
    }

    let (decided, is_webtoon, median_ratio) = classify_from_dimensions(&dims);
    if decided && is_webtoon && row.detected_webtoon == 0 {
        let _ = state.db.mark_manga_detected_webtoon(manga_id).await;
    }

    let layout = if !decided {
        DetectedLayout::Undecided
    } else if is_webtoon {
        DetectedLayout::Webtoon
    } else {
        DetectedLayout::Manga
    };

    Ok(LayoutDetectionResult {
        layout,
        signal: if decided {
            DetectionSignal::Dimensions
        } else {
            DetectionSignal::None
        },
        decided,
        is_webtoon,
        median_ratio,
        sampled_pages: dims.len(),
    })
}

async fn sniff_single_page_header_dims(state: &AppState, token: &str) -> Option<(u32, u32)> {
    let verified = verify_image_token(token, &state.proxy_key)?;

    // Priority A: Local comic archive page (zero network).
    if crate::services::comics::is_local_comic_url(&verified.url)
        && let Ok(local_url) = Url::parse(&verified.url)
        && local_url.path().starts_with("/page/")
    {
        let (bytes, _mime) =
            crate::services::comics::read_local_page_bytes(&state.data_dir, state, &local_url)
                .await
                .ok()?;
        return sniff_dims_blocking(bytes).await;
    }

    // Priority B: Already cached on disk in `ImageCache` (zero network).
    let cache_key = ImageCache::key_for_url(&verified.url);
    if state.images.has_cached_url(&verified.url) {
        let meta = CacheEntryMeta {
            site: verified.site.as_deref(),
            manga_id: verified.manga_id,
            is_cover: false,
            chapter_url: verified.chapter_url.as_deref(),
        };
        if let Some(hit) = state.images.get(&cache_key, meta).await {
            let slice_len = hit.bytes.len().min(HEADER_SNIFF_MAX_BYTES);
            let header_vec = hit.bytes[..slice_len].to_vec();
            return sniff_dims_blocking(header_vec).await;
        }
    }

    // Priority C: Bounded 64 KiB HTTP Range header probe (DoH + fail-closed Proxy + SSRF safe).
    let target_url = Url::parse(&verified.url).ok()?;
    let referer = Url::parse(&verified.referer).ok();
    let header_bytes = state
        .registry
        .http()
        .fetch_image_header_bytes(&target_url, referer.as_ref(), HEADER_SNIFF_MAX_BYTES)
        .await
        .ok()?;

    sniff_dims_blocking(header_bytes).await
}

async fn sniff_dims_blocking(header_bytes: Vec<u8>) -> Option<(u32, u32)> {
    tokio::task::spawn_blocking(move || {
        let size = imagesize::blob_size(&header_bytes).ok()?;
        let w = u32::try_from(size.width).ok()?;
        let h = u32::try_from(size.height).ok()?;
        Some((w, h))
    })
    .await
    .ok()
    .flatten()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metadata_priority_detects_webtoon_manhwa_long_strip_and_full_color() {
        assert_eq!(
            classify_from_metadata(
                &["Action".to_owned(), "Manhwa".to_owned()],
                &[],
                "example.com"
            ),
            Some(DetectedLayout::Webtoon),
            "manhwa genre must trigger immediate webtoon classification"
        );
        assert_eq!(
            classify_from_metadata(&[], &["Long Strip".to_owned()], "example.com"),
            Some(DetectedLayout::Webtoon),
            "long strip tag must trigger immediate webtoon classification"
        );
        assert_eq!(
            classify_from_metadata(&["Full Color".to_owned()], &[], "example.com"),
            Some(DetectedLayout::Webtoon),
            "full color genre must trigger immediate webtoon classification"
        );
        assert_eq!(
            classify_from_metadata(
                &["Shonen".to_owned(), "Adventure".to_owned()],
                &[],
                "example.com"
            ),
            None,
            "standard manga genres must defer to header dimension sniffing"
        );
    }

    #[test]
    fn dimension_heuristic_distinguishes_webtoon_strips_from_manga_pages() {
        // Median ratio >= 1.8 triggers Webtoon.
        let (decided, is_webtoon, median) =
            classify_from_dimensions(&[(800, 1600), (800, 1700), (800, 1520)]);
        assert!(
            decided && is_webtoon && median >= 1.8,
            "median {median} should classify as webtoon"
        );

        // Tall strip >= 2.2 with matching width continuity triggers Webtoon even if other slices are shorter.
        let (decided_tall, is_webtoon_tall, _) =
            classify_from_dimensions(&[(800, 1200), (800, 1300), (800, 2400)]);
        assert!(
            decided_tall && is_webtoon_tall,
            "tall strip >= 2.2 with matching width continuity must classify as webtoon"
        );

        // Standard Japanese manga pages (~1.42 ratio) default cleanly to Manga paged mode.
        let (decided_manga, is_webtoon_manga, _) =
            classify_from_dimensions(&[(1000, 1450), (1000, 1420), (1000, 1400)]);
        assert!(
            decided_manga && !is_webtoon_manga,
            "standard manga aspect ratios must default to manga paged mode"
        );

        // A single mismatched-width tall credits banner inside standard manga must NOT falsely trigger webtoon.
        let (decided_banner, is_webtoon_banner, _) =
            classify_from_dimensions(&[(1000, 1450), (1000, 1420), (1000, 1400), (500, 1500)]);
        assert!(
            decided_banner && !is_webtoon_banner,
            "mismatched-width banner must not falsely trigger webtoon mode"
        );
    }

    #[test]
    fn imagesize_parses_minimal_png_header_without_full_decode() {
        // Valid 24-byte PNG signature + IHDR chunk (800 x 2400) with zero pixel IDAT data.
        let mut png_hdr = Vec::with_capacity(24);
        png_hdr.extend_from_slice(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]);
        png_hdr.extend_from_slice(&13_u32.to_be_bytes());
        png_hdr.extend_from_slice(b"IHDR");
        png_hdr.extend_from_slice(&800_u32.to_be_bytes());
        png_hdr.extend_from_slice(&2400_u32.to_be_bytes());

        let dims = imagesize::blob_size(&png_hdr).expect("header-only PNG must parse");
        assert_eq!((dims.width, dims.height), (800, 2400));
    }
}
