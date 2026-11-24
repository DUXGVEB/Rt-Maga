//! Universal comic image candidate resolution, URL normalization, and pure-Rust binary image
//! format & dimension verification.

use std::sync::LazyLock;

use regex::Regex;
use serde::{Deserialize, Serialize};
use url::Url;

use crate::model::PageImage;
use crate::parse::dom::{abs_url, is_ignored_image_url};

static CSS_URL_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?i)url\(\s*['"]?([^'")\s]+)['"]?\s*\)"#).expect("valid CSS url regex")
});

static HTML_DEC_ENTITY_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"&#(\d{1,7});").expect("valid decimal entity regex"));

static HTML_HEX_ENTITY_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)&#x([0-9a-f]{1,6});").expect("valid hex entity regex"));

/// Origin strategy from which an image URL candidate was discovered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum CandidateOrigin {
    /// Standard or lazy-loaded `<img>` / `<amp-img>` tag.
    HtmlImg,
    /// `<picture><source srcset="...">` element paired with an `<img>`.
    HtmlPictureSource,
    /// Inline CSS `background-image: url(...)` declaration.
    CssBackground,
    /// Structured JSON (`__NEXT_DATA__`, JSON-LD, `ts_reader`, API object).
    StructuredJson,
    /// Document metadata (`og:image`, etc.).
    MetadataTag,
    /// Fallback `<script>` pattern match.
    ScriptEmbedded,
}

/// Unnormalized image candidate collected from DOM, CSS, metadata, or structured data.
#[derive(Debug, Clone)]
pub struct RawImageCandidate {
    /// Primary candidate string (may be relative, protocol-relative, or HTML-entity encoded).
    pub raw_url: String,
    /// Optional backup mirror URLs associated with the same visual page.
    pub fallbacks: Vec<String>,
    /// Page URL required as `Referer` header when downloading the image.
    pub referer: Url,
    /// Extraction mechanism that discovered this candidate.
    pub origin: CandidateOrigin,
}

/// Output of universal candidate normalization, deduplication, and ad/UI filtering.
#[derive(Debug, Clone)]
pub struct ResolvedImageCollection {
    /// Validated, deduplicated page images preserving reading order.
    pub pages: Vec<PageImage>,
    /// Total raw candidates inspected before filtering and deduplication.
    pub raw_candidates: usize,
    /// Number of candidates discarded because they matched ad, tracker, UI icon, or invalid scheme rules.
    pub rejected_ui_or_ads: usize,
    /// Number of duplicate primary URLs collapsed into earlier entries.
    pub duplicates_removed: usize,
    /// Number of `blob:` URLs encountered (useful diagnostic hint when scripts use Blob media).
    pub blob_urls_seen: usize,
}

/// Supported raster image formats verified by binary header inspection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum DetectedImageFormat {
    /// JPEG / JFIF / Exif.
    Jpeg,
    /// Portable Network Graphics.
    Png,
    /// WebP (VP8, VP8L, or VP8X).
    Webp,
    /// Graphics Interchange Format.
    Gif,
    /// AV1 Image File Format / ISO BMFF still image.
    Avif,
    /// Windows Bitmap.
    Bmp,
}

impl DetectedImageFormat {
    /// Canonical MIME content type for this verified image format.
    #[must_use]
    pub const fn mime_type(self) -> &'static str {
        match self {
            Self::Jpeg => "image/jpeg",
            Self::Png => "image/png",
            Self::Webp => "image/webp",
            Self::Gif => "image/gif",
            Self::Avif => "image/avif",
            Self::Bmp => "image/bmp",
        }
    }
}

/// Binary image validation result carrying exact pixel dimensions parsed from headers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImageHeaderInfo {
    /// Binary format detected from magic signature and chunk structure.
    pub format: DetectedImageFormat,
    /// Image width in pixels (`0` only when ISO BMFF `ispe` box is absent).
    pub width: u32,
    /// Image height in pixels (`0` only when ISO BMFF `ispe` box is absent).
    pub height: u32,
    /// Total inspected payload byte length.
    pub byte_len: usize,
}

/// Specific binary image validation errors distinguishing unsupported formats vs corruption.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ImageValidationError {
    /// Upstream returned a 0-byte body.
    EmptyPayload,
    /// Known image header was present, but structural framing/markers were truncated or invalid.
    TruncatedOrCorrupted {
        /// Detected format label.
        format: &'static str,
        /// Technical detail about header/framing defect.
        detail: String,
    },
    /// Payload does not match a supported comic raster image format (e.g. HTML, JSON, SVG, JS).
    UnsupportedFormat {
        /// Short sniffed format hint.
        hint: String,
    },
    /// Image dimensions are unrealistically small (e.g. 1x1 tracking pixel).
    DimensionsTooSmall {
        /// Parsed image width.
        width: u32,
        /// Parsed image height.
        height: u32,
    },
}

/// Unescapes HTML entities, decimal/hex numeric entities, and common JS/JSON string escapes
/// inside extracted URL attributes or script strings.
#[must_use]
pub fn unescape_url_raw(raw: &str) -> String {
    let mut s = raw
        .trim()
        .replace("\\/", "/")
        .replace("\\u002F", "/")
        .replace("\\u002f", "/")
        .replace("\\u0026", "&")
        .replace("\\u003D", "=")
        .replace("\\u003d", "=")
        .replace("\\u003F", "?")
        .replace("\\u003f", "?")
        .replace("&amp;", "&")
        .replace("&AMP;", "&")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'");

    if s.contains("&#") {
        s = HTML_DEC_ENTITY_RE
            .replace_all(&s, |caps: &regex::Captures<'_>| {
                caps.get(1)
                    .and_then(|m| m.as_str().parse::<u32>().ok())
                    .and_then(char::from_u32)
                    .map_or_else(|| caps[0].to_owned(), |ch| ch.to_string())
            })
            .into_owned();
        s = HTML_HEX_ENTITY_RE
            .replace_all(&s, |caps: &regex::Captures<'_>| {
                caps.get(1)
                    .and_then(|m| u32::from_str_radix(m.as_str(), 16).ok())
                    .and_then(char::from_u32)
                    .map_or_else(|| caps[0].to_owned(), |ch| ch.to_string())
            })
            .into_owned();
    }

    s
}

/// Normalizes a raw image attribute or script URL string against `base`, splitting an optional
/// `#media-fallback=` mirror URL if present.
#[must_use]
pub fn resolve_image_url(base: &Url, raw: &str) -> Option<(Url, Option<Url>)> {
    let cleaned = unescape_url_raw(raw);
    let trimmed = cleaned.trim();
    if trimmed.is_empty() || trimmed.contains("${") || trimmed.contains("{{") {
        return None;
    }

    let (main_part, fallback_part) = match trimmed.split_once("#media-fallback=") {
        Some((m, fb)) => (m.trim(), Some(fb.trim())),
        None => (trimmed, None),
    };

    let main_encoded = if main_part.contains(' ') {
        main_part.replace(' ', "%20")
    } else {
        main_part.to_owned()
    };

    let primary = abs_url(base, &main_encoded)?;
    let fallback = fallback_part
        .map(percent_decode_simple)
        .and_then(|dec| abs_url(base, &dec))
        .filter(|u| u != &primary);

    Some((primary, fallback))
}

fn percent_decode_simple(raw: &str) -> String {
    let bytes = raw.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let (Some(hi), Some(lo)) = (hex_val(bytes[i + 1]), hex_val(bytes[i + 2]))
        {
            out.push((hi << 4) | lo);
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

const fn hex_val(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

/// Parses an HTML `srcset` or `data-srcset` attribute and returns resolved URLs sorted from
/// highest descriptor (`w` or `x`) to lowest so fallbacks preserve resolution order.
#[must_use]
pub fn parse_srcset_ranked(srcset: &str, base: &Url) -> Vec<Url> {
    let decoded = unescape_url_raw(srcset);
    let mut scored: Vec<(f32, usize, Url)> = Vec::new();
    for (idx, part) in decoded.split(',').enumerate() {
        let mut tokens = part.split_whitespace();
        let Some(raw_url) = tokens.next() else {
            continue;
        };
        let weight = tokens
            .next()
            .and_then(|desc| {
                desc.strip_suffix('w')
                    .or_else(|| desc.strip_suffix('x'))
                    .and_then(|n| n.parse::<f32>().ok())
            })
            .unwrap_or(1.0);
        if let Some((url, _)) = resolve_image_url(base, raw_url) {
            scored.push((weight, idx, url));
        }
    }
    scored.sort_by(|a, b| b.0.total_cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    let mut unique = Vec::with_capacity(scored.len());
    for (_, _, url) in scored {
        if !unique.contains(&url) {
            unique.push(url);
        }
    }
    unique
}

/// Extracts resolved image URLs from an inline CSS `style` attribute (`background-image`,
/// `background: ... url(...)`).
#[must_use]
pub fn extract_css_background_urls(style: &str, base: &Url) -> Vec<Url> {
    let decoded = unescape_url_raw(style);
    let mut urls = Vec::new();
    for caps in CSS_URL_RE.captures_iter(&decoded) {
        if let Some((url, fb)) = resolve_image_url(base, &caps[1]) {
            if !urls.contains(&url) {
                urls.push(url);
            }
            if let Some(alt) = fb
                && !urls.contains(&alt)
            {
                urls.push(alt);
            }
        }
    }
    urls
}

/// Normalizes, filters, and deduplicates raw image candidates while preserving reading order and
/// merging fallback mirrors when the same primary image appears twice.
#[must_use]
pub fn resolve_page_images(
    candidates: Vec<RawImageCandidate>,
    base: &Url,
) -> ResolvedImageCollection {
    let raw_candidates = candidates.len();
    let mut pages: Vec<PageImage> = Vec::with_capacity(candidates.len());
    let mut index_by_primary: std::collections::HashMap<String, usize> =
        std::collections::HashMap::with_capacity(candidates.len());
    let mut rejected_ui_or_ads = 0_usize;
    let mut duplicates_removed = 0_usize;
    let mut blob_urls_seen = 0_usize;

    for cand in candidates {
        if cand.raw_url.trim().starts_with("blob:") {
            blob_urls_seen += 1;
            continue;
        }
        let Some((primary, split_fb)) = resolve_image_url(base, &cand.raw_url) else {
            rejected_ui_or_ads += 1;
            continue;
        };
        if is_ignored_image_url(&primary) {
            rejected_ui_or_ads += 1;
            continue;
        }

        let mut fallbacks = Vec::new();
        if let Some(fb) = split_fb
            && !is_ignored_image_url(&fb)
        {
            fallbacks.push(fb);
        }
        for raw_fb in &cand.fallbacks {
            if let Some((fb_url, _)) = resolve_image_url(base, raw_fb)
                && fb_url != primary
                && !is_ignored_image_url(&fb_url)
                && !fallbacks.contains(&fb_url)
            {
                fallbacks.push(fb_url);
            }
        }

        let key = primary.as_str().to_owned();
        if let Some(&existing_idx) = index_by_primary.get(&key) {
            duplicates_removed += 1;
            for fb in fallbacks {
                if !pages[existing_idx].fallbacks.contains(&fb) {
                    pages[existing_idx].fallbacks.push(fb);
                }
            }
            continue;
        }

        index_by_primary.insert(key, pages.len());
        pages.push(PageImage {
            url: primary,
            fallbacks,
            referer: cand.referer,
        });
    }

    ResolvedImageCollection {
        pages,
        raw_candidates,
        rejected_ui_or_ads,
        duplicates_removed,
        blob_urls_seen,
    }
}

/// Validates binary image payload magic bytes, container headers, exact pixel dimensions, and
/// truncation sanity without pulling external decoding crates.
///
/// # Errors
///
/// Returns [`ImageValidationError`] distinguishing empty/corrupted payloads from non-image or
/// 1x1 tracking payloads.
pub fn inspect_image_bytes(bytes: &[u8]) -> Result<ImageHeaderInfo, ImageValidationError> {
    if bytes.is_empty() {
        return Err(ImageValidationError::EmptyPayload);
    }

    let info = if bytes.starts_with(&[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]) {
        inspect_png(bytes)?
    } else if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        inspect_jpeg(bytes)?
    } else if bytes.len() >= 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP" {
        inspect_webp(bytes)?
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        inspect_gif(bytes)?
    } else if bytes.starts_with(b"BM") {
        inspect_bmp(bytes)?
    } else if bytes.len() >= 12 && &bytes[4..8] == b"ftyp" {
        inspect_isobmff(bytes)?
    } else {
        return Err(ImageValidationError::UnsupportedFormat {
            hint: sniff_unsupported_hint(bytes),
        });
    };

    if (info.width > 0 && info.width < 16) || (info.height > 0 && info.height < 16) {
        return Err(ImageValidationError::DimensionsTooSmall {
            width: info.width,
            height: info.height,
        });
    }

    Ok(info)
}

fn inspect_png(bytes: &[u8]) -> Result<ImageHeaderInfo, ImageValidationError> {
    if bytes.len() < 33 || &bytes[12..16] != b"IHDR" {
        return Err(ImageValidationError::TruncatedOrCorrupted {
            format: "png",
            detail: "truncated PNG header or missing IHDR chunk".to_owned(),
        });
    }
    let width = u32::from_be_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]);
    let height = u32::from_be_bytes([bytes[20], bytes[21], bytes[22], bytes[23]]);
    if width == 0 || height == 0 {
        return Err(ImageValidationError::TruncatedOrCorrupted {
            format: "png",
            detail: "PNG declares zero width or height".to_owned(),
        });
    }
    Ok(ImageHeaderInfo {
        format: DetectedImageFormat::Png,
        width,
        height,
        byte_len: bytes.len(),
    })
}

fn inspect_jpeg(bytes: &[u8]) -> Result<ImageHeaderInfo, ImageValidationError> {
    if bytes.len() < 32 {
        return Err(ImageValidationError::TruncatedOrCorrupted {
            format: "jpeg",
            detail: "truncated JPEG payload".to_owned(),
        });
    }
    let mut pos = 2_usize;
    let mut dims: Option<(u32, u32)> = None;
    let mut saw_sos = false;

    while pos + 3 < bytes.len() {
        while pos < bytes.len() && bytes[pos] != 0xFF {
            pos += 1;
        }
        while pos < bytes.len() && bytes[pos] == 0xFF {
            pos += 1;
        }
        if pos >= bytes.len() {
            break;
        }
        let marker = bytes[pos];
        pos += 1;

        if marker == 0x00 || (0xD0..=0xD7).contains(&marker) || marker == 0x01 {
            continue;
        }
        if marker == 0xD9 {
            break;
        }
        if marker == 0xDA {
            saw_sos = true;
            break;
        }
        if pos + 1 >= bytes.len() {
            break;
        }
        let seg_len = usize::from(u16::from_be_bytes([bytes[pos], bytes[pos + 1]]));
        if seg_len < 2 || pos + seg_len > bytes.len() {
            return Err(ImageValidationError::TruncatedOrCorrupted {
                format: "jpeg",
                detail: "truncated JPEG segment marker".to_owned(),
            });
        }
        if is_jpeg_sof_marker(marker) {
            if seg_len < 7 || pos + 6 >= bytes.len() {
                return Err(ImageValidationError::TruncatedOrCorrupted {
                    format: "jpeg",
                    detail: "truncated JPEG SOF frame header".to_owned(),
                });
            }
            let height = u32::from(u16::from_be_bytes([bytes[pos + 3], bytes[pos + 4]]));
            let width = u32::from(u16::from_be_bytes([bytes[pos + 5], bytes[pos + 6]]));
            dims = Some((width, height));
        }
        pos += seg_len;
    }

    let Some((width, height)) = dims else {
        return Err(ImageValidationError::TruncatedOrCorrupted {
            format: "jpeg",
            detail: "missing JPEG SOF frame dimensions".to_owned(),
        });
    };
    if !saw_sos || width == 0 || height == 0 {
        return Err(ImageValidationError::TruncatedOrCorrupted {
            format: "jpeg",
            detail: if saw_sos {
                "JPEG declares zero width or height".to_owned()
            } else {
                "JPEG truncated before image scan data (SOS)".to_owned()
            },
        });
    }

    Ok(ImageHeaderInfo {
        format: DetectedImageFormat::Jpeg,
        width,
        height,
        byte_len: bytes.len(),
    })
}

const fn is_jpeg_sof_marker(marker: u8) -> bool {
    matches!(
        marker,
        0xC0 | 0xC1 | 0xC2 | 0xC3 | 0xC5 | 0xC6 | 0xC7 | 0xC9 | 0xCA | 0xCB | 0xCD | 0xCE | 0xCF
    )
}

fn inspect_webp(bytes: &[u8]) -> Result<ImageHeaderInfo, ImageValidationError> {
    if bytes.len() < 26 {
        return Err(ImageValidationError::TruncatedOrCorrupted {
            format: "webp",
            detail: "truncated WebP RIFF header".to_owned(),
        });
    }
    let chunk = &bytes[12..16];
    let (width, height) = if chunk == b"VP8 " {
        if bytes.len() < 30 || bytes[23] != 0x9D || bytes[24] != 0x01 || bytes[25] != 0x2A {
            return Err(ImageValidationError::TruncatedOrCorrupted {
                format: "webp",
                detail: "invalid or truncated VP8 keyframe header".to_owned(),
            });
        }
        let raw_w = u16::from_le_bytes([bytes[26], bytes[27]]) & 0x3FFF;
        let raw_h = u16::from_le_bytes([bytes[28], bytes[29]]) & 0x3FFF;
        (u32::from(raw_w), u32::from(raw_h))
    } else if chunk == b"VP8L" {
        if bytes.len() < 25 || bytes[20] != 0x2F {
            return Err(ImageValidationError::TruncatedOrCorrupted {
                format: "webp",
                detail: "invalid or truncated VP8L lossless header".to_owned(),
            });
        }
        let bits = u32::from_le_bytes([bytes[21], bytes[22], bytes[23], bytes[24]]);
        let w = (bits & 0x3FFF) + 1;
        let h = ((bits >> 14) & 0x3FFF) + 1;
        (w, h)
    } else if chunk == b"VP8X" {
        if bytes.len() < 30 {
            return Err(ImageValidationError::TruncatedOrCorrupted {
                format: "webp",
                detail: "truncated VP8X extended header".to_owned(),
            });
        }
        let w = u32::from_le_bytes([bytes[24], bytes[25], bytes[26], 0]) + 1;
        let h = u32::from_le_bytes([bytes[27], bytes[28], bytes[29], 0]) + 1;
        (w, h)
    } else {
        return Err(ImageValidationError::TruncatedOrCorrupted {
            format: "webp",
            detail: "unrecognized WebP chunk signature".to_owned(),
        });
    };

    if width == 0 || height == 0 {
        return Err(ImageValidationError::TruncatedOrCorrupted {
            format: "webp",
            detail: "WebP declares zero dimensions".to_owned(),
        });
    }
    Ok(ImageHeaderInfo {
        format: DetectedImageFormat::Webp,
        width,
        height,
        byte_len: bytes.len(),
    })
}

fn inspect_gif(bytes: &[u8]) -> Result<ImageHeaderInfo, ImageValidationError> {
    if bytes.len() < 14 {
        return Err(ImageValidationError::TruncatedOrCorrupted {
            format: "gif",
            detail: "truncated GIF logical screen descriptor".to_owned(),
        });
    }
    let width = u32::from(u16::from_le_bytes([bytes[6], bytes[7]]));
    let height = u32::from(u16::from_le_bytes([bytes[8], bytes[9]]));
    if width == 0 || height == 0 {
        return Err(ImageValidationError::TruncatedOrCorrupted {
            format: "gif",
            detail: "GIF declares zero dimensions".to_owned(),
        });
    }
    Ok(ImageHeaderInfo {
        format: DetectedImageFormat::Gif,
        width,
        height,
        byte_len: bytes.len(),
    })
}

fn inspect_bmp(bytes: &[u8]) -> Result<ImageHeaderInfo, ImageValidationError> {
    if bytes.len() < 26 {
        return Err(ImageValidationError::TruncatedOrCorrupted {
            format: "bmp",
            detail: "truncated BMP DIB header".to_owned(),
        });
    }
    let width = u32::from_le_bytes([bytes[18], bytes[19], bytes[20], bytes[21]]);
    let raw_h = i32::from_le_bytes([bytes[22], bytes[23], bytes[24], bytes[25]]);
    let height = raw_h.unsigned_abs();
    if width == 0 || height == 0 {
        return Err(ImageValidationError::TruncatedOrCorrupted {
            format: "bmp",
            detail: "BMP declares zero dimensions".to_owned(),
        });
    }
    Ok(ImageHeaderInfo {
        format: DetectedImageFormat::Bmp,
        width,
        height,
        byte_len: bytes.len(),
    })
}

fn inspect_isobmff(bytes: &[u8]) -> Result<ImageHeaderInfo, ImageValidationError> {
    let major = &bytes[8..12];
    let is_avif_brand = matches!(
        major,
        b"avif" | b"avis" | b"mif1" | b"msf1" | b"heic" | b"heix"
    ) || bytes[12..bytes.len().min(48)]
        .windows(4)
        .any(|w| w == b"avif" || w == b"avis");
    if !is_avif_brand {
        return Err(ImageValidationError::UnsupportedFormat {
            hint: format!("ISO BMFF container with non-AVIF brand {major:?}"),
        });
    }
    if bytes.len() < 32 {
        return Err(ImageValidationError::TruncatedOrCorrupted {
            format: "avif",
            detail: "truncated AVIF container".to_owned(),
        });
    }
    let (width, height) = find_ispe_dimensions(bytes).unwrap_or((0, 0));
    Ok(ImageHeaderInfo {
        format: DetectedImageFormat::Avif,
        width,
        height,
        byte_len: bytes.len(),
    })
}

fn find_ispe_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    // Search top-level boxes cleanly, including inside `meta` -> `iprp` -> `ipco` -> `ispe`.
    scan_isobmff_range(bytes, 0, bytes.len(), 0)
}

fn scan_isobmff_range(bytes: &[u8], start: usize, end: usize, depth: u8) -> Option<(u32, u32)> {
    if depth > 5 {
        return None;
    }
    let mut pos = start;
    while pos + 8 <= end && pos + 8 <= bytes.len() {
        let size_u32 =
            u32::from_be_bytes([bytes[pos], bytes[pos + 1], bytes[pos + 2], bytes[pos + 3]]);
        let box_size = if size_u32 == 0 {
            end.saturating_sub(pos)
        } else {
            usize::try_from(size_u32).ok()?
        };
        if box_size < 8 || pos.saturating_add(box_size) > end {
            break;
        }
        let kind = &bytes[pos + 4..pos + 8];
        if kind == b"ispe" && box_size >= 20 {
            let w = u32::from_be_bytes([
                bytes[pos + 12],
                bytes[pos + 13],
                bytes[pos + 14],
                bytes[pos + 15],
            ]);
            let h = u32::from_be_bytes([
                bytes[pos + 16],
                bytes[pos + 17],
                bytes[pos + 18],
                bytes[pos + 19],
            ]);
            if w > 0 && h > 0 {
                return Some((w, h));
            }
        }
        if kind == b"meta" && box_size > 12 {
            if let Some(d) = scan_isobmff_range(bytes, pos + 12, pos + box_size, depth + 1) {
                return Some(d);
            }
        } else if matches!(kind, b"iprp" | b"ipco")
            && let Some(d) = scan_isobmff_range(bytes, pos + 8, pos + box_size, depth + 1)
        {
            return Some(d);
        }
        pos += box_size;
    }
    None
}

fn sniff_unsupported_hint(bytes: &[u8]) -> String {
    let prefix = std::str::from_utf8(&bytes[..bytes.len().min(64)])
        .ok()
        .map_or("", str::trim_start);
    let lower = prefix.to_ascii_lowercase();
    if lower.starts_with("<svg") {
        "SVG vector graphics are not supported as raster comic pages".to_owned()
    } else if lower.starts_with("<!doctype") || lower.starts_with("<html") {
        "HTML error/challenge page returned instead of image bytes".to_owned()
    } else if lower.starts_with('{') || lower.starts_with('[') {
        "JSON payload returned instead of image bytes".to_owned()
    } else {
        "payload header does not match JPEG, PNG, WebP, GIF, BMP, or AVIF".to_owned()
    }
}
