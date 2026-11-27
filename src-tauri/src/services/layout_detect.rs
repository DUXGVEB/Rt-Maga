//! Fast, non-blocking multi-signal Webtoon vs. Manga layout classifier.
//!
//! Classification precedence:
//! 1. **Manual per-manga override** (`reader_mode_override` or `reader_settings_json`) always wins.
//! 2. **Strict Webtoon metadata signals** (`webtoon`, `long strip`, `vertical scroll`, or dedicated
//!    vertical-strip publishers `webtoons`, `toomics`, `lezhin`). Neutral/ambiguous tags such as
//!    `"full color"`, `"truyện màu"`, generic `"toon"` substrings, or standalone `"manhwa"` /
//!    `"manhua"` NEVER force Webtoon mode on their own.
//! 3. **Multi-signal page dimension & structure heuristic** on up to 6 leading pages via
//!    header-only `imagesize::blob_size` (plus lightweight PNG border/gutter uniformity when bytes
//!    are already local or cached):
//!    - Evaluates aspect-ratio distribution (`1.15..=1.68` manga range vs `>= 1.85` tall strips),
//!    - Evaluates vertical strip continuity (at least 2 consecutive or majority width-matched tall
//!      strips within `±8%` — NEVER flipping on a single outlier credit/promo banner),
//!    - Evaluates strip-width uniformity & seamless vertical gutter/border uniformity.
//! 4. **Manga-positive metadata fallback** (`manga`, `doujinshi`, `oneshot`, `tankobon`, `4-koma`,
//!    `shonen`, `seinen`, `shojo`, `josei` when no explicit webtoon/manhwa/manhua conflict exists).

use std::io::Read;

use flate2::read::ZlibDecoder;
use serde::Serialize;
use url::Url;

use crate::cache::ImageCache;
use crate::cache::image::CacheEntryMeta;
use crate::error::AppError;
use crate::image_proxy::verify_image_token;
use crate::state::AppState;

/// Maximum header bytes probed per page over HTTP Range (`64 KiB`).
const HEADER_SNIFF_MAX_BYTES: usize = 64 * 1024;

/// Maximum number of leading chapter pages sampled by the multi-signal heuristic.
pub(crate) const MAX_SNIFF_PAGES: usize = 6;

/// Maximum byte size inspected for optional PNG edge/gutter uniformity (keeps decode tiny & fast).
const MAX_EDGE_INSPECT_BYTES: usize = 4 * 1024 * 1024;

/// Maximum PNG pixel width inspected for single-row edge uniformity.
const MAX_EDGE_INSPECT_WIDTH: usize = 4096;

/// Aspect ratio bounds for standard paged manga sheets (`height / width`).
const MANGA_RATIO_MIN: f64 = 1.15;
const MANGA_RATIO_MAX: f64 = 1.68;

/// Aspect ratio threshold for vertical long-strip slices (`height / width`).
const TALL_STRIP_RATIO_MIN: f64 = 1.85;

/// Width tolerance for vertical strip continuity (`±8%`).
const STRIP_WIDTH_TOLERANCE: f64 = 0.08;

/// Tighter width tolerance for uniform master-canvas strip slicing (`±4%`).
const UNIFORM_STRIP_WIDTH_TOLERANCE: f64 = 0.04;

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

impl DetectedLayout {
    /// Persisted SQLite string representation (`"manga"` | `"webtoon"` | `None`).
    #[must_use]
    pub(crate) const fn as_db_str(self) -> Option<&'static str> {
        match self {
            Self::Webtoon => Some("webtoon"),
            Self::Manga => Some("manga"),
            Self::ManualOverride | Self::Undecided => None,
        }
    }

    /// Parses a persisted SQLite `detected_layout` string.
    #[must_use]
    pub(crate) fn from_db_str(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "webtoon" => Some(Self::Webtoon),
            "manga" => Some(Self::Manga),
            _ => None,
        }
    }
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

/// Lightweight border/gutter edge profile extracted from local/cached PNG bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct EdgeUniformity {
    pub top_row_uniform: bool,
    pub bottom_row_uniform: bool,
    pub side_margins_uniform: bool,
    pub top_luma: u8,
    pub bottom_luma: u8,
}

/// Dimension and optional border/gutter sample for a single leading chapter page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PageSample {
    pub width: u32,
    pub height: u32,
    pub edge: Option<EdgeUniformity>,
}

/// Detailed multi-signal breakdown produced by [`analyze_dimensions_and_structure`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct DimensionAnalysis {
    pub layout: DetectedLayout,
    pub decided: bool,
    pub is_webtoon: bool,
    pub median_ratio: f64,
    pub qualifying_count: usize,
    pub manga_range_count: usize,
    pub tall_strip_count: usize,
    pub has_consecutive_tall_strips: bool,
    pub seamless_seam_pairs: usize,
}

/// Secondary regional origin hint passed into dimension analysis (never flips paged manga alone).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OriginHint {
    Neutral,
    SoftManhwaManhua,
}

/// Detailed metadata classification preserving soft ambiguous hints for dimension tie-breaking.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct MetadataAssessment {
    strict_webtoon: bool,
    manga_positive: bool,
    soft_manhwa_manhua: bool,
}

impl MetadataAssessment {
    const fn origin_hint(self) -> OriginHint {
        if self.soft_manhwa_manhua {
            OriginHint::SoftManhwaManhua
        } else {
            OriginHint::Neutral
        }
    }
}

/// Strict Webtoon / vertical long-strip format keywords (explicit format indicators only).
const STRICT_WEBTOON_KEYWORDS: &[&str] = &[
    "webtoon",
    "webtoons",
    "web toon",
    "long strip",
    "longstrip",
    "vertical scroll",
    "verticalscroll",
    "vertical strip",
    "infinite scroll",
    "truyen doc",
    "truyện dọc",
    "toomics",
    "lezhin",
];

/// Dedicated vertical-strip publisher domain labels (matched by exact DNS label, NEVER generic `"toon"`).
const STRICT_WEBTOON_HOST_LABELS: &[&str] = &[
    "webtoons",
    "webtoon",
    "toomics",
    "lezhin",
    "lezhinus",
    "lezhinjp",
    "lezhincomics",
];

/// Ambiguous regional origin tags that must NOT auto-classify as Webtoon without strip evidence.
const SOFT_ORIGIN_TAGS: &[&str] = &["manhwa", "manhua"];

/// Positive Manga format/demographic signals (used when no strict Webtoon or conflicting Manhwa/Manhua tag exists).
const MANGA_POSITIVE_KEYWORDS: &[&str] = &[
    "manga",
    "doujinshi",
    "oneshot",
    "one shot",
    "tankobon",
    "tankoubon",
    "4 koma",
    "4koma",
    "yonkoma",
    "shonen",
    "shounen",
    "seinen",
    "shojo",
    "shoujo",
    "josei",
];

fn normalize_meta_token(raw: &str) -> String {
    raw.trim()
        .to_ascii_lowercase()
        .replace(['_', '-', '/'], " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn contains_whole_phrase(haystack: &str, needle: &str) -> bool {
    if haystack == needle {
        return true;
    }
    let Some(start) = haystack.find(needle) else {
        return false;
    };
    let end = start + needle.len();
    let before_ok = start == 0
        || haystack[..start]
            .chars()
            .next_back()
            .is_some_and(|c| !c.is_alphanumeric());
    let after_ok = end == haystack.len()
        || haystack[end..]
            .chars()
            .next()
            .is_some_and(|c| !c.is_alphanumeric());
    before_ok && after_ok
}

fn host_matches_strict_webtoon_publisher(site: &str) -> bool {
    let lower = site.trim().to_ascii_lowercase();
    if lower.is_empty() || lower.starts_with("local:") {
        return false;
    }
    let host_only = lower
        .strip_prefix("https://")
        .or_else(|| lower.strip_prefix("http://"))
        .unwrap_or(&lower)
        .split('/')
        .next()
        .unwrap_or("");
    let host_no_port = host_only.split(':').next().unwrap_or("");

    host_no_port
        .split('.')
        .any(|label| STRICT_WEBTOON_HOST_LABELS.contains(&label))
}

fn assess_metadata(genres: &[String], tags: &[String], site: &str) -> MetadataAssessment {
    let mut strict_webtoon = host_matches_strict_webtoon_publisher(site);
    let mut manga_positive = false;
    let mut soft_manhwa_manhua = false;

    for raw in genres.iter().chain(tags.iter()) {
        let norm = normalize_meta_token(raw);
        if norm.is_empty() {
            continue;
        }
        if STRICT_WEBTOON_KEYWORDS
            .iter()
            .any(|kw| contains_whole_phrase(&norm, kw))
        {
            strict_webtoon = true;
        }
        if SOFT_ORIGIN_TAGS
            .iter()
            .any(|kw| contains_whole_phrase(&norm, kw))
        {
            soft_manhwa_manhua = true;
        }
        if MANGA_POSITIVE_KEYWORDS
            .iter()
            .any(|kw| contains_whole_phrase(&norm, kw))
        {
            manga_positive = true;
        }
    }

    MetadataAssessment {
        strict_webtoon,
        manga_positive,
        soft_manhwa_manhua,
    }
}

/// Evaluates manga genres, tags, and strict publisher hostname hints for instant metadata
/// classification without misclassifying colored manga or paged manhwa/manhua.
///
/// - Strict Webtoon tags (`webtoon`, `long strip`, `vertical scroll`, `toomics`, `lezhin`,
///   `webtoons`) -> `Some(DetectedLayout::Webtoon)`.
/// - Manga-positive tags (`manga`, `doujinshi`, `oneshot`, `tankobon`, `4-koma`, `shonen`,
///   `seinen`, `shojo`, `josei`) with no strict Webtoon or conflicting `manhwa`/`manhua` tag
///   -> `Some(DetectedLayout::Manga)`.
/// - Ambiguous/neutral tags (`full color`, `truyện màu`, standalone `manhwa`/`manhua`, or
///   conflicting `manhwa` + `shonen`) -> `None` (defer to page dimension/structure analysis).
#[must_use]
pub(crate) fn classify_from_metadata(
    genres: &[String],
    tags: &[String],
    site: &str,
) -> Option<DetectedLayout> {
    let assessment = assess_metadata(genres, tags, site);
    if assessment.strict_webtoon {
        return Some(DetectedLayout::Webtoon);
    }
    if assessment.manga_positive && !assessment.soft_manhwa_manhua {
        return Some(DetectedLayout::Manga);
    }
    None
}

fn widths_match(w_a: u32, w_b: u32, tolerance: f64) -> bool {
    let max_w = f64::from(w_a.max(w_b));
    let diff = f64::from(w_a.abs_diff(w_b));
    diff <= max_w * tolerance
}

/// Evaluates aspect-ratio distribution, vertical strip continuity, width uniformity, and optional
/// border/gutter edge uniformity across up to 6 leading pages.
#[must_use]
#[expect(
    clippy::too_many_lines,
    reason = "multi-signal aspect-ratio, strip continuity, width uniformity, and gutter analysis in one pure function"
)]
pub(crate) fn analyze_dimensions_and_structure(
    samples: &[PageSample],
    origin_hint: OriginHint,
) -> DimensionAnalysis {
    let soft_webtoon_hint = origin_hint == OriginHint::SoftManhwaManhua;
    let mut qualifying: Vec<(u32, f64, Option<EdgeUniformity>)> = Vec::with_capacity(samples.len());
    let mut spread_count = 0_usize;

    for sample in samples {
        if sample.width < 200 || sample.height < 300 {
            continue;
        }
        // Exclude horizontal double-page spreads (`width > height * 1.15`), while recording their
        // presence as supporting Manga evidence.
        if u64::from(sample.width) * 100 > u64::from(sample.height) * 115 {
            if sample.width >= 600 {
                spread_count += 1;
            }
            continue;
        }
        let ratio = f64::from(sample.height) / f64::from(sample.width);
        qualifying.push((sample.width, ratio, sample.edge));
    }

    if qualifying.len() < 2 {
        return DimensionAnalysis {
            layout: DetectedLayout::Undecided,
            decided: false,
            is_webtoon: false,
            median_ratio: 0.0,
            qualifying_count: qualifying.len(),
            manga_range_count: 0,
            tall_strip_count: 0,
            has_consecutive_tall_strips: false,
            seamless_seam_pairs: 0,
        };
    }

    let mut ratios: Vec<f64> = qualifying.iter().map(|&(_, r, _)| r).collect();
    ratios.sort_by(f64::total_cmp);
    let mid = ratios.len() / 2;
    let median_ratio = if ratios.len() % 2 == 1 {
        ratios[mid]
    } else {
        f64::midpoint(ratios[mid - 1], ratios[mid])
    };

    let manga_range_count = qualifying
        .iter()
        .filter(|&&(_, r, _)| (MANGA_RATIO_MIN..=MANGA_RATIO_MAX).contains(&r))
        .count();

    let tall_strips: Vec<(usize, u32, f64)> = qualifying
        .iter()
        .enumerate()
        .filter_map(|(i, &(w, r, _))| (r >= TALL_STRIP_RATIO_MIN).then_some((i, w, r)))
        .collect();
    let tall_strip_count = tall_strips.len();

    // Signal B: Consecutive vertical strip continuity — requires TWO adjacent qualifying pages
    // that are BOTH tall (`>= 1.85`) AND share width within ±8%. Never fires on a single banner!
    let has_consecutive_tall_strips = qualifying.windows(2).any(|pair| {
        let (w_a, r_a, _) = pair[0];
        let (w_b, r_b, _) = pair[1];
        r_a >= TALL_STRIP_RATIO_MIN
            && r_b >= TALL_STRIP_RATIO_MIN
            && widths_match(w_a, w_b, STRIP_WIDTH_TOLERANCE)
    });

    // Pairwise width continuity among tall strips only (never matching a tall banner to a normal manga page).
    let has_tall_pair_width_continuity = tall_strips.iter().enumerate().any(|(i, &(_, w_a, _))| {
        tall_strips
            .iter()
            .enumerate()
            .any(|(j, &(_, w_b, _))| i != j && widths_match(w_a, w_b, STRIP_WIDTH_TOLERANCE))
    });

    // Signal C: Strip-width uniformity (`±4%`) + optional seamless PNG vertical gutter seams.
    let has_uniform_strip_widths = qualifying.iter().any(|&(anchor_w, _, _)| {
        let close = qualifying
            .iter()
            .filter(|&&(w, _, _)| widths_match(anchor_w, w, UNIFORM_STRIP_WIDTH_TOLERANCE))
            .count();
        close * 4 >= qualifying.len() * 3
    });

    let seamless_seam_pairs = qualifying
        .windows(2)
        .filter(|pair| {
            let (w_a, _, edge_a) = pair[0];
            let (w_b, _, edge_b) = pair[1];
            let Some(ea) = edge_a else { return false };
            let Some(eb) = edge_b else { return false };
            widths_match(w_a, w_b, STRIP_WIDTH_TOLERANCE)
                && ea.bottom_row_uniform
                && eb.top_row_uniform
                && ea.bottom_luma.abs_diff(eb.top_luma) <= 12
                && !ea.side_margins_uniform
                && !eb.side_margins_uniform
        })
        .count();

    let manga_bordered_pages = qualifying
        .iter()
        .filter(|&&(_, r, edge)| {
            (MANGA_RATIO_MIN..=MANGA_RATIO_MAX).contains(&r)
                && edge.is_some_and(|e| {
                    e.side_margins_uniform && e.top_row_uniform && e.bottom_row_uniform
                })
        })
        .count();

    let total_f = f64::from(u32::try_from(qualifying.len()).unwrap_or(u32::MAX));
    let manga_fraction = f64::from(u32::try_from(manga_range_count).unwrap_or(0)) / total_f;
    let is_majority_tall = tall_strip_count * 2 > qualifying.len();

    // Strict Webtoon decision: ALWAYS requires at least 2 tall strips (`>= 1.85`), eliminating
    // single-banner / scanlator-credit false positives completely.
    let is_webtoon = tall_strip_count >= 2
        && ((median_ratio >= TALL_STRIP_RATIO_MIN
            && (has_tall_pair_width_continuity || is_majority_tall))
            || (has_consecutive_tall_strips && manga_fraction < 0.60)
            || (is_majority_tall && has_tall_pair_width_continuity)
            || (has_tall_pair_width_continuity
                && manga_fraction < 0.50
                && (seamless_seam_pairs > 0 || (soft_webtoon_hint && has_uniform_strip_widths))));

    // Guard against a 1-vs-1 tie on a tiny 2-page sample (e.g. 1 manga page + 1 tall page).
    let is_ambiguous_two_page_split =
        qualifying.len() == 2 && tall_strip_count == 1 && manga_range_count == 1;

    let is_manga = !is_webtoon
        && !is_ambiguous_two_page_split
        && (manga_fraction >= 0.50
            || median_ratio <= MANGA_RATIO_MAX
            || (qualifying.len() >= 3 && tall_strip_count < 2)
            || (spread_count > 0 && tall_strip_count < 2)
            || manga_bordered_pages >= 2);

    let layout = if is_webtoon {
        DetectedLayout::Webtoon
    } else if is_manga {
        DetectedLayout::Manga
    } else {
        DetectedLayout::Undecided
    };

    let rounded_median = (median_ratio * 100.0).round() / 100.0;
    DimensionAnalysis {
        layout,
        decided: layout != DetectedLayout::Undecided,
        is_webtoon,
        median_ratio: rounded_median,
        qualifying_count: qualifying.len(),
        manga_range_count,
        tall_strip_count,
        has_consecutive_tall_strips,
        seamless_seam_pairs,
    }
}

/// Classifies Webtoon vs. Manga paged layout purely from natural `(width, height)` pairs.
///
/// Returns `(decided, is_webtoon, median_ratio)` synchronized with
/// [`analyze_dimensions_and_structure`].
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "pure dimension heuristic helper exercised by unit tests"
    )
)]
#[must_use]
pub(crate) fn classify_from_dimensions(dims: &[(u32, u32)]) -> (bool, bool, f64) {
    let samples: Vec<PageSample> = dims
        .iter()
        .map(|&(width, height)| PageSample {
            width,
            height,
            edge: None,
        })
        .collect();
    let analysis = analyze_dimensions_and_structure(&samples, OriginHint::Neutral);
    (analysis.decided, analysis.is_webtoon, analysis.median_ratio)
}

/// Classifies a newly imported or resolved manga using metadata and optional leading page byte
/// samples (up to 6 pages for local comic imports; empty slice for instant web imports),
/// persisting `'manga'` or `'webtoon'` into SQLite (or leaving `NULL` when undecided so first
/// chapter open in the Reader refines it).
///
/// Never overwrites an existing manual per-manga reader override.
///
/// # Errors
///
/// Returns [`AppError::Db`] on SQLite failure.
pub(crate) async fn classify_manga_on_import(
    state: &AppState,
    manga_id: i64,
    genres: &[String],
    tags: &[String],
    site: &str,
    sampled_pages: &[Vec<u8>],
) -> Result<Option<&'static str>, AppError> {
    let Some(row) = state.db.get_manga_by_id(manga_id).await? else {
        return Ok(None);
    };

    if row.reader_mode_override.is_some() || row.reader_settings_json.is_some() {
        return Ok(None);
    }

    // Preserve an already-classified layout during web re-resolves without new page samples.
    if sampled_pages.is_empty() && row.detected_layout.is_some() {
        return Ok(row
            .detected_layout
            .as_deref()
            .and_then(DetectedLayout::from_db_str)
            .and_then(DetectedLayout::as_db_str));
    }

    let meta = assess_metadata(genres, tags, site);
    if meta.strict_webtoon {
        state
            .db
            .set_manga_detected_layout(manga_id, Some("webtoon"))
            .await?;
        return Ok(Some("webtoon"));
    }

    if !sampled_pages.is_empty() {
        let pages_owned: Vec<Vec<u8>> = sampled_pages
            .iter()
            .take(MAX_SNIFF_PAGES)
            .cloned()
            .collect();
        let samples = tokio::task::spawn_blocking(move || {
            pages_owned
                .iter()
                .filter_map(|bytes| sniff_page_sample_sync(bytes))
                .collect::<Vec<PageSample>>()
        })
        .await
        .unwrap_or_default();

        let analysis = analyze_dimensions_and_structure(&samples, meta.origin_hint());
        if let Some(db_label) = analysis.layout.as_db_str() {
            state
                .db
                .set_manga_detected_layout(manga_id, Some(db_label))
                .await?;
            return Ok(Some(db_label));
        }
    }

    if meta.manga_positive && !meta.soft_manhwa_manhua {
        state
            .db
            .set_manga_detected_layout(manga_id, Some("manga"))
            .await?;
        return Ok(Some("manga"));
    }

    Ok(None)
}

async fn persist_detected_layout_if_changed(
    state: &AppState,
    row: &crate::repo::manga::MangaRow,
    layout: DetectedLayout,
) {
    let Some(db_label) = layout.as_db_str() else {
        return;
    };
    let desired_webtoon = i64::from(layout == DetectedLayout::Webtoon);
    if row.detected_layout.as_deref() != Some(db_label) || row.detected_webtoon != desired_webtoon {
        let _ = state
            .db
            .set_manga_detected_layout(row.id, Some(db_label))
            .await;
    }
}

fn build_detection_result(
    layout: DetectedLayout,
    signal: DetectionSignal,
    median_ratio: f64,
    sampled_pages: usize,
) -> LayoutDetectionResult {
    LayoutDetectionResult {
        layout,
        signal,
        decided: layout != DetectedLayout::Undecided,
        is_webtoon: layout == DetectedLayout::Webtoon,
        median_ratio,
        sampled_pages,
    }
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
        return Ok(build_detection_result(
            DetectedLayout::Undecided,
            DetectionSignal::None,
            0.0,
            0,
        ));
    };

    // Rule 0: Manual per-manga reader override ALWAYS takes precedence over auto-detection.
    if row.reader_mode_override.is_some() || row.reader_settings_json.is_some() {
        return Ok(build_detection_result(
            DetectedLayout::ManualOverride,
            DetectionSignal::Override,
            0.0,
            0,
        ));
    }

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

    // Rule 1: Strict Webtoon metadata signals (zero latency, zero extra I/O).
    let meta = assess_metadata(&genres, &tags, &row.site);
    if meta.strict_webtoon {
        persist_detected_layout_if_changed(state, &row, DetectedLayout::Webtoon).await;
        return Ok(build_detection_result(
            DetectedLayout::Webtoon,
            DetectionSignal::Metadata,
            0.0,
            0,
        ));
    }

    // Rule 2: Multi-signal header-only dimension & structure sniffing on up to 6 leading pages.
    let mut samples: Vec<PageSample> = Vec::with_capacity(page_tokens.len().min(MAX_SNIFF_PAGES));
    for token in page_tokens.iter().take(MAX_SNIFF_PAGES) {
        if let Some(sample) = sniff_single_page_sample(state, token).await {
            samples.push(sample);
        }
    }

    let analysis = analyze_dimensions_and_structure(&samples, meta.origin_hint());
    if analysis.decided {
        persist_detected_layout_if_changed(state, &row, analysis.layout).await;
        return Ok(build_detection_result(
            analysis.layout,
            DetectionSignal::Dimensions,
            analysis.median_ratio,
            samples.len(),
        ));
    }

    // Rule 3: Fallback to Manga-positive metadata (no manhwa/manhua conflict) or existing verdict.
    if meta.manga_positive && !meta.soft_manhwa_manhua {
        persist_detected_layout_if_changed(state, &row, DetectedLayout::Manga).await;
        return Ok(build_detection_result(
            DetectedLayout::Manga,
            DetectionSignal::Metadata,
            analysis.median_ratio,
            samples.len(),
        ));
    }

    if let Some(existing_layout) = row
        .detected_layout
        .as_deref()
        .and_then(DetectedLayout::from_db_str)
    {
        return Ok(build_detection_result(
            existing_layout,
            DetectionSignal::Metadata,
            analysis.median_ratio,
            samples.len(),
        ));
    }

    Ok(build_detection_result(
        DetectedLayout::Undecided,
        DetectionSignal::None,
        analysis.median_ratio,
        samples.len(),
    ))
}

async fn sniff_single_page_sample(state: &AppState, token: &str) -> Option<PageSample> {
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
        return sniff_page_sample_blocking(bytes).await;
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
            let slice_len = hit.bytes.len().min(MAX_EDGE_INSPECT_BYTES);
            let cached_vec = hit.bytes[..slice_len].to_vec();
            return sniff_page_sample_blocking(cached_vec).await;
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

    sniff_page_sample_blocking(header_bytes).await
}

async fn sniff_page_sample_blocking(bytes: Vec<u8>) -> Option<PageSample> {
    tokio::task::spawn_blocking(move || sniff_page_sample_sync(&bytes))
        .await
        .ok()
        .flatten()
}

pub(crate) fn sniff_page_sample_sync(bytes: &[u8]) -> Option<PageSample> {
    let size = imagesize::blob_size(bytes).ok()?;
    let width = u32::try_from(size.width).ok()?;
    let height = u32::try_from(size.height).ok()?;
    let edge = inspect_png_edge_uniformity(bytes);
    Some(PageSample {
        width,
        height,
        edge,
    })
}

/// Fast pure-Rust top/bottom/side border uniformity inspector for standard non-interlaced 8-bit
/// PNG images already in memory (local imports / cached pages). Returns `None` for non-PNG,
/// header-only probes, interlaced PNGs, or oversized images so it never slows down classification.
#[expect(
    clippy::too_many_lines,
    reason = "self-contained PNG IHDR + IDAT scanline unfilter for top, bottom, and side edge uniformity"
)]
fn inspect_png_edge_uniformity(bytes: &[u8]) -> Option<EdgeUniformity> {
    const PNG_SIG: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    if bytes.len() < 33 || bytes.len() > MAX_EDGE_INSPECT_BYTES || bytes[..8] != PNG_SIG {
        return None;
    }

    let mut pos = 8_usize;
    let mut width = 0_usize;
    let mut height = 0_usize;
    let mut channels = 0_usize;
    let mut idat = Vec::new();

    while pos + 12 <= bytes.len() {
        let len = usize::try_from(u32::from_be_bytes([
            bytes[pos],
            bytes[pos + 1],
            bytes[pos + 2],
            bytes[pos + 3],
        ]))
        .ok()?;
        let kind = &bytes[pos + 4..pos + 8];
        let data_start = pos + 8;
        let data_end = data_start.checked_add(len)?;
        let next_pos = data_end.checked_add(4)?;
        if next_pos > bytes.len() {
            return None;
        }
        let chunk = &bytes[data_start..data_end];
        match kind {
            b"IHDR" => {
                if chunk.len() < 13 {
                    return None;
                }
                width =
                    usize::try_from(u32::from_be_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
                        .ok()?;
                height =
                    usize::try_from(u32::from_be_bytes([chunk[4], chunk[5], chunk[6], chunk[7]]))
                        .ok()?;
                let bit_depth = chunk[8];
                let color_type = chunk[9];
                let interlace = chunk[12];
                if bit_depth != 8 || interlace != 0 || width < 16 || height < 16 {
                    return None;
                }
                if width > MAX_EDGE_INSPECT_WIDTH {
                    return None;
                }
                channels = match color_type {
                    0 => 1, // Grayscale
                    2 => 3, // RGB
                    4 => 2, // Grayscale + Alpha
                    6 => 4, // RGBA
                    _ => return None,
                };
            }
            b"IDAT" => {
                if idat.len().saturating_add(chunk.len()) > MAX_EDGE_INSPECT_BYTES {
                    return None;
                }
                idat.extend_from_slice(chunk);
            }
            b"IEND" => break,
            _ => {}
        }
        pos = next_pos;
    }

    if width == 0 || height == 0 || channels == 0 || idat.is_empty() {
        return None;
    }

    let row_bytes = width.checked_mul(channels)?;
    let stride = row_bytes.checked_add(1)?;
    let total_raw = stride.checked_mul(height)?;
    // Cap decoded scanline buffer at 16 MiB to keep peak RAM tiny.
    if total_raw > 16 * 1024 * 1024 {
        return None;
    }

    let mut decoder = ZlibDecoder::new(idat.as_slice());
    let mut raw = Vec::with_capacity(total_raw.min(64 * 1024));
    let cap = u64::try_from(total_raw.saturating_add(1)).ok()?;
    decoder.by_ref().take(cap).read_to_end(&mut raw).ok()?;
    if raw.len() != total_raw {
        return None;
    }

    let mut prev_row = vec![0_u8; row_bytes];
    let mut curr_row = vec![0_u8; row_bytes];
    let mut top_luma_row = Vec::new();
    let mut bottom_luma_row = Vec::new();
    let mut left_col = Vec::with_capacity(height.min(64));
    let mut right_col = Vec::with_capacity(height.min(64));
    let col_step = (height / 64).max(1);

    for y in 0..height {
        let base = y * stride;
        let filter = raw[base];
        let src = &raw[base + 1..base + stride];
        unfilter_png_row(filter, src, &prev_row, &mut curr_row, channels)?;

        if y == 0 {
            top_luma_row = row_to_luma(&curr_row, channels);
        }
        if y + 1 == height {
            bottom_luma_row = row_to_luma(&curr_row, channels);
        }
        if y % col_step == 0 {
            left_col.push(pixel_luma(&curr_row[..channels]));
            right_col.push(pixel_luma(&curr_row[row_bytes - channels..]));
        }
        std::mem::swap(&mut prev_row, &mut curr_row);
    }

    let (top_row_uniform, top_luma) = is_strip_uniform(&top_luma_row);
    let (bottom_row_uniform, bottom_luma) = is_strip_uniform(&bottom_luma_row);
    let (left_uniform, left_luma) = is_strip_uniform(&left_col);
    let (right_uniform, right_luma) = is_strip_uniform(&right_col);
    let side_margins_uniform =
        left_uniform && right_uniform && left_luma.abs_diff(right_luma) <= 12;

    Some(EdgeUniformity {
        top_row_uniform,
        bottom_row_uniform,
        side_margins_uniform,
        top_luma,
        bottom_luma,
    })
}

fn unfilter_png_row(filter: u8, src: &[u8], prev: &[u8], dst: &mut [u8], bpp: usize) -> Option<()> {
    match filter {
        0 => dst.copy_from_slice(src),
        1 => {
            for i in 0..src.len() {
                let left = if i >= bpp { dst[i - bpp] } else { 0 };
                dst[i] = src[i].wrapping_add(left);
            }
        }
        2 => {
            for i in 0..src.len() {
                dst[i] = src[i].wrapping_add(prev[i]);
            }
        }
        3 => {
            for i in 0..src.len() {
                let left = if i >= bpp { u16::from(dst[i - bpp]) } else { 0 };
                let up = u16::from(prev[i]);
                let avg = u8::try_from(u16::midpoint(left, up)).unwrap_or(0);
                dst[i] = src[i].wrapping_add(avg);
            }
        }
        4 => {
            for i in 0..src.len() {
                let a = if i >= bpp { dst[i - bpp] } else { 0 };
                let b = prev[i];
                let c = if i >= bpp { prev[i - bpp] } else { 0 };
                dst[i] = src[i].wrapping_add(paeth_predictor(a, b, c));
            }
        }
        _ => return None,
    }
    Some(())
}

fn paeth_predictor(a: u8, b: u8, c: u8) -> u8 {
    let ia = i32::from(a);
    let ib = i32::from(b);
    let ic = i32::from(c);
    let p = ia + ib - ic;
    let pa = (p - ia).abs();
    let pb = (p - ib).abs();
    let pc = (p - ic).abs();
    if pa <= pb && pa <= pc {
        a
    } else if pb <= pc {
        b
    } else {
        c
    }
}

fn pixel_luma(px: &[u8]) -> u8 {
    match px.len() {
        1 | 2 => px[0],
        3 | 4 => {
            let luma =
                (u32::from(px[0]) * 77 + u32::from(px[1]) * 150 + u32::from(px[2]) * 29) >> 8;
            u8::try_from(luma).unwrap_or(255)
        }
        _ => 0,
    }
}

fn row_to_luma(row: &[u8], channels: usize) -> Vec<u8> {
    row.chunks_exact(channels).map(pixel_luma).collect()
}

fn is_strip_uniform(samples: &[u8]) -> (bool, u8) {
    if samples.is_empty() {
        return (false, 0);
    }
    let sum: u64 = samples.iter().map(|&v| u64::from(v)).sum();
    let count = u64::try_from(samples.len()).unwrap_or(1);
    let mean = u8::try_from(sum / count).unwrap_or(0);
    let outliers = samples.iter().filter(|&&v| v.abs_diff(mean) > 10).count();
    let uniform = outliers * 20 <= samples.len();
    (uniform, mean)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mk_solid_png(width: u32, height: u32, luma: u8) -> Vec<u8> {
        let mut pixmap = hayro::vello_cpu::Pixmap::new(
            u16::try_from(width).expect("test width fits u16"),
            u16::try_from(height).expect("test height fits u16"),
        );
        pixmap
            .data_mut()
            .fill(hayro::vello_cpu::color::PremulRgba8 {
                r: luma,
                g: luma,
                b: luma,
                a: 255,
            });
        pixmap.into_png().expect("solid png encodes")
    }

    #[test]
    fn metadata_separates_strict_webtoon_from_neutral_tags_and_manga_signals() {
        // Strict Webtoon tags & dedicated publisher hosts trigger immediate Webtoon classification.
        assert_eq!(
            classify_from_metadata(&[], &["Long Strip".to_owned()], "example.com"),
            Some(DetectedLayout::Webtoon),
            "long strip tag must trigger Webtoon"
        );
        assert_eq!(
            classify_from_metadata(&["Webtoon".to_owned()], &[], "example.com"),
            Some(DetectedLayout::Webtoon),
            "webtoon tag must trigger Webtoon"
        );
        assert_eq!(
            classify_from_metadata(&[], &["Vertical Scroll".to_owned()], "example.com"),
            Some(DetectedLayout::Webtoon),
            "vertical scroll tag must trigger Webtoon"
        );
        assert_eq!(
            classify_from_metadata(&[], &[], "www.webtoons.com"),
            Some(DetectedLayout::Webtoon),
            "webtoons.com publisher host must trigger Webtoon"
        );
        assert_eq!(
            classify_from_metadata(&[], &[], "toomics.com"),
            Some(DetectedLayout::Webtoon),
            "toomics.com publisher host must trigger Webtoon"
        );
        assert_eq!(
            classify_from_metadata(&[], &[], "www.lezhinus.com"),
            Some(DetectedLayout::Webtoon),
            "lezhinus.com publisher host must trigger Webtoon"
        );

        // Regression: neutral/ambiguous tags MUST NOT auto-classify as Webtoon!
        assert_eq!(
            classify_from_metadata(&["Full Color".to_owned()], &[], "example.com"),
            None,
            "full color alone must NOT auto-classify as Webtoon"
        );
        assert_eq!(
            classify_from_metadata(&["Truyện Màu".to_owned()], &[], "example.com"),
            None,
            "truyen mau alone must NOT auto-classify as Webtoon"
        );
        assert_eq!(
            classify_from_metadata(&["Action".to_owned()], &[], "cartoon-archive.com"),
            None,
            "generic 'toon' substring in host must NOT auto-classify as Webtoon"
        );
        assert_eq!(
            classify_from_metadata(
                &["Action".to_owned(), "Manhwa".to_owned()],
                &[],
                "example.com"
            ),
            None,
            "standalone manhwa without strip evidence must defer to dimensions"
        );
        assert_eq!(
            classify_from_metadata(
                &["Shonen".to_owned(), "Manhwa".to_owned()],
                &[],
                "example.com"
            ),
            None,
            "conflicting Shonen + Manhwa must stay undecided for dimension sniffing"
        );

        // Manga-positive tags (including colored manga!) classify cleanly as Manga.
        assert_eq!(
            classify_from_metadata(
                &[
                    "Manga".to_owned(),
                    "Full Color".to_owned(),
                    "Shonen".to_owned()
                ],
                &[],
                "example.com"
            ),
            Some(DetectedLayout::Manga),
            "Digital Colored Manga with Manga+Full Color must classify as Manga"
        );
        assert_eq!(
            classify_from_metadata(
                &["Seinen".to_owned(), "Adventure".to_owned()],
                &[],
                "example.com"
            ),
            Some(DetectedLayout::Manga),
            "Seinen without webtoon/manhwa conflict must classify as Manga"
        );
        assert_eq!(
            classify_from_metadata(&["Doujinshi".to_owned()], &[], "example.com"),
            Some(DetectedLayout::Manga),
            "Doujinshi must classify as Manga"
        );
    }

    #[test]
    fn dimension_heuristic_eliminates_single_banner_false_positives_in_manga() {
        // Regression 1: Standard Japanese manga with 1 same-width tall scanlator credit banner.
        // Previously falsely classified as Webtoon!
        let (decided, is_webtoon, _) =
            classify_from_dimensions(&[(800, 1200), (800, 1180), (800, 1220), (800, 2600)]);
        assert!(
            decided && !is_webtoon,
            "manga with 1 same-width tall credit strip must classify as Manga, never Webtoon"
        );

        // Regression 2: Standard manga with tall credit strip on page 0.
        let (decided_p0, is_webtoon_p0, _) =
            classify_from_dimensions(&[(800, 2500), (800, 1150), (800, 1160), (800, 1150)]);
        assert!(
            decided_p0 && !is_webtoon_p0,
            "manga starting with 1 tall credit page must classify as Manga"
        );

        // Regression 3: 6-page manga with a double-page spread + 1 tall banner.
        let (decided_6p, is_webtoon_6p, _) = classify_from_dimensions(&[
            (1000, 1450),
            (1000, 1430),
            (2000, 1440), // double-page spread
            (1000, 1420),
            (1000, 1440),
            (1000, 2800), // outlier credit banner
        ]);
        assert!(
            decided_6p && !is_webtoon_6p,
            "6-page manga with spread and 1 outlier banner must classify as Manga"
        );

        // Regression 4: 2-page split (1 manga page + 1 tall page) must stay undecided, NEVER Webtoon!
        let (decided_split, is_webtoon_split, _) =
            classify_from_dimensions(&[(800, 1200), (800, 2400)]);
        assert!(
            !decided_split && !is_webtoon_split,
            "1-vs-1 split on 2 pages must remain undecided and never false-flip to Webtoon"
        );

        // Regression 5: 4 manga pages + 2 narrow mismatched-width promo banners -> Manga!
        let (decided_mismatch, is_webtoon_mismatch, _) = classify_from_dimensions(&[
            (1000, 1450),
            (1000, 1420),
            (1000, 1430),
            (1000, 1440),
            (400, 1200),
            (600, 1800),
        ]);
        assert!(
            decided_mismatch && !is_webtoon_mismatch,
            "manga majority with mismatched-width promo strips must classify as Manga"
        );
    }

    #[test]
    fn dimension_heuristic_reliably_detects_true_webtoon_strips() {
        // Pure vertical webtoon strips.
        let (decided, is_webtoon, median) =
            classify_from_dimensions(&[(800, 1600), (800, 1700), (800, 1520)]);
        assert!(
            decided && is_webtoon && median >= 1.85,
            "tall continuous strips (median {median}) must classify as Webtoon"
        );

        // Webtoon with 1 normal title card followed by 3 consecutive tall strips.
        let (decided_intro, is_webtoon_intro, _) =
            classify_from_dimensions(&[(800, 1150), (800, 2200), (800, 2400), (800, 2100)]);
        assert!(
            decided_intro && is_webtoon_intro,
            "webtoon with intro card + 3 tall strips must classify as Webtoon"
        );

        // Webtoon with 2 normal title cards + 2 consecutive tall width-matched strips.
        let (decided_consec, is_webtoon_consec, _) =
            classify_from_dimensions(&[(800, 1200), (800, 1250), (800, 2400), (800, 2600)]);
        assert!(
            decided_consec && is_webtoon_consec,
            "2 consecutive tall width-matched strips (50% of chapter) must classify as Webtoon"
        );
    }

    #[test]
    fn png_header_and_edge_uniformity_inspect_cleanly() {
        // Header-only 24-byte PNG still resolves dimensions cleanly.
        let mut png_hdr = Vec::with_capacity(24);
        png_hdr.extend_from_slice(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]);
        png_hdr.extend_from_slice(&13_u32.to_be_bytes());
        png_hdr.extend_from_slice(b"IHDR");
        png_hdr.extend_from_slice(&800_u32.to_be_bytes());
        png_hdr.extend_from_slice(&2400_u32.to_be_bytes());

        let hdr_sample = sniff_page_sample_sync(&png_hdr).expect("header-only PNG must parse");
        assert_eq!(
            (hdr_sample.width, hdr_sample.height),
            (800, 2400),
            "header dimensions must match IHDR"
        );
        assert!(
            hdr_sample.edge.is_none(),
            "header-only PNG without IDAT must skip edge inspection cleanly"
        );

        // Full synthetic PNG produces edge uniformity profile.
        let solid = mk_solid_png(64, 96, 250);
        let full_sample = sniff_page_sample_sync(&solid).expect("full solid PNG must parse");
        let edge = full_sample
            .edge
            .expect("full solid PNG must compute edge uniformity");
        assert!(
            edge.top_row_uniform && edge.bottom_row_uniform && edge.side_margins_uniform,
            "uniform white page must report uniform top, bottom, and side edges"
        );
    }
}
