//! Shared natural alphanumeric sorting, junk/traversal filtering, metadata parsing
//! (`ComicInfo.xml` and Mihon/Tachiyomi `details.json`), and filename series/chapter inference.

use std::cmp::Ordering;
use std::path::Path;
use std::sync::LazyLock;

use quick_xml::Reader as XmlReader;
use quick_xml::events::Event;
use regex::Regex;
use sha2::{Digest, Sha256};

use crate::error::AppError;

/// Maximum single uncompressed image page size permitted (32 MiB).
pub(crate) const MAX_COMIC_PAGE_BYTES: u64 = 32 * 1024 * 1024;

/// Maximum single imported archive/document file size permitted (1.5 GiB).
pub(crate) const MAX_COMIC_ARCHIVE_BYTES: u64 = 1536 * 1024 * 1024;

/// Maximum number of pages indexed per chapter to prevent memory/UI exhaustion.
pub(crate) const MAX_PAGES_PER_CHAPTER: usize = 4096;

/// Maximum metadata file (`ComicInfo.xml`, `details.json`, `.opf`, XHTML) size read in memory (4 MiB).
pub(crate) const MAX_METADATA_BYTES: u64 = 4 * 1024 * 1024;

static CHAPTER_SUFFIX_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)[\s._-]*(?:vol(?:ume)?\.?\s*\d+[\s._-]*)?(?:chapter|chap|chương|chuong|ch|ep|tập|tap|c|#)\.?\s*(\d+(?:\.\d+)?)\b.*$",
    )
    .expect("valid comic chapter suffix regex")
});
static VOLUME_ONLY_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)[\s._-]*(?:vol(?:ume)?|tập|tap|quyển|quyen)\.?\s*(\d+(?:\.\d+)?)\b.*$")
        .expect("valid comic volume regex")
});
static TRAILING_NUM_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(?P<series>.+?)\s+0*(?P<num>\d{1,4}(?:\.\d+)?)$")
        .expect("valid trailing number regex")
});

/// Normalized bibliographic metadata extracted from `ComicInfo.xml`, EPUB `.opf`, PDF Info, or
/// Mihon/Tachiyomi `details.json`.
#[derive(Debug, Clone, Default)]
pub(crate) struct ComicMetadata {
    pub series: Option<String>,
    pub title: Option<String>,
    pub number: Option<f32>,
    pub volume: Option<String>,
    pub summary: Option<String>,
    pub authors: Vec<String>,
    pub artists: Vec<String>,
    pub genres: Vec<String>,
    pub language: Option<String>,
}

/// Returns `true` for OS junk files (`__MACOSX`, `.DS_Store`, `Thumbs.db`, `desktop.ini`,
/// `AppleDouble` `._*`, hidden dotfiles) or unsafe path traversal entries (`..`).
#[must_use]
pub(crate) fn is_junk_entry(raw_path: &str) -> bool {
    let normalized = raw_path.replace('\\', "/");
    let trimmed = normalized.trim();
    if trimmed.is_empty() || trimmed.ends_with('/') {
        return true;
    }

    for segment in trimmed.split('/') {
        if segment.is_empty() {
            continue;
        }
        if segment == ".." || segment == "." {
            return true;
        }
        let lower = segment.to_ascii_lowercase();
        if lower == "__macosx"
            || lower == ".ds_store"
            || lower == "thumbs.db"
            || lower == "desktop.ini"
            || lower.starts_with("._")
            || lower.starts_with('.')
        {
            return true;
        }
    }

    false
}

/// Returns `true` when `filename_lower` ends with a supported raster comic image extension.
#[must_use]
pub(crate) fn is_supported_image_ext(filename_lower: &str) -> bool {
    [".jpg", ".jpeg", ".png", ".webp", ".avif", ".gif", ".bmp"]
        .iter()
        .any(|ext| filename_lower.ends_with(ext))
}

/// Natural alphanumeric comparator so `"page_2.jpg"` sorts before `"page_10.jpg"`.
#[must_use]
pub(crate) fn natural_cmp(a: &str, b: &str) -> Ordering {
    let mut ia = a.chars().peekable();
    let mut ib = b.chars().peekable();

    loop {
        match (ia.peek(), ib.peek()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(&ca), Some(&cb)) if ca.is_ascii_digit() && cb.is_ascii_digit() => {
                let mut sa = String::new();
                while let Some(&d) = ia.peek() {
                    if d.is_ascii_digit() {
                        sa.push(d);
                        ia.next();
                    } else {
                        break;
                    }
                }
                let mut sb = String::new();
                while let Some(&d) = ib.peek() {
                    if d.is_ascii_digit() {
                        sb.push(d);
                        ib.next();
                    } else {
                        break;
                    }
                }
                let ta = sa.trim_start_matches('0');
                let tb = sb.trim_start_matches('0');
                let cmp = ta
                    .len()
                    .cmp(&tb.len())
                    .then_with(|| ta.cmp(tb))
                    .then_with(|| sa.len().cmp(&sb.len()));
                if cmp != Ordering::Equal {
                    return cmp;
                }
            }
            (Some(_), Some(_)) => {
                let ca = ia.next().unwrap_or_default().to_ascii_lowercase();
                let cb = ib.next().unwrap_or_default().to_ascii_lowercase();
                let cmp = ca.cmp(&cb);
                if cmp != Ordering::Equal {
                    return cmp;
                }
            }
        }
    }
}

/// Parses a `ComicInfo.xml` payload into [`ComicMetadata`].
#[must_use]
pub(crate) fn parse_comic_info_xml(xml_bytes: &[u8]) -> ComicMetadata {
    let mut reader = XmlReader::from_reader(xml_bytes);
    reader.config_mut().trim_text(true);
    let mut meta = ComicMetadata::default();
    let mut current_tag = String::new();
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                current_tag = String::from_utf8_lossy(e.name().as_ref()).to_ascii_lowercase();
            }
            Ok(Event::Text(t)) => {
                if let Ok(val) = t.decode() {
                    let text = val.trim();
                    if !text.is_empty() {
                        match current_tag.as_str() {
                            "series" => meta.series = Some(text.to_owned()),
                            "title" => meta.title = Some(text.to_owned()),
                            "number" => meta.number = text.parse::<f32>().ok(),
                            "volume" if text != "-1" => {
                                meta.volume = Some(text.to_owned());
                            }
                            "summary" => meta.summary = Some(text.to_owned()),
                            "writer" => push_csv_unique(&mut meta.authors, text),
                            "penciller" | "inker" => push_csv_unique(&mut meta.artists, text),
                            "genre" | "tags" => push_csv_unique(&mut meta.genres, text),
                            "languageiso" => meta.language = Some(text.to_owned()),
                            _ => {}
                        }
                    }
                }
            }
            Ok(Event::End(_)) => current_tag.clear(),
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
        buf.clear();
    }
    meta
}

/// Parses a Mihon / Tachiyomi `details.json` metadata file if present in an exported series folder.
#[must_use]
pub(crate) fn parse_mihon_details_json(json_bytes: &[u8]) -> Option<ComicMetadata> {
    let val: serde_json::Value = serde_json::from_slice(json_bytes).ok()?;
    let obj = val.as_object()?;
    let mut meta = ComicMetadata::default();

    if let Some(title) = obj
        .get("title")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        meta.series = Some(title.to_owned());
    }
    if let Some(desc) = obj
        .get("description")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        meta.summary = Some(desc.to_owned());
    }
    if let Some(author) = obj
        .get("author")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        push_csv_unique(&mut meta.authors, author);
    }
    if let Some(artist) = obj
        .get("artist")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        push_csv_unique(&mut meta.artists, artist);
    }
    if let Some(arr) = obj.get("genre").and_then(serde_json::Value::as_array) {
        for item in arr {
            if let Some(g) = item.as_str().map(str::trim).filter(|s| !s.is_empty()) {
                push_csv_unique(&mut meta.genres, g);
            }
        }
    }

    Some(meta)
}

pub(crate) fn push_csv_unique(target: &mut Vec<String>, raw: &str) {
    for part in raw.split(',').map(str::trim).filter(|s| !s.is_empty()) {
        if !target.iter().any(|e| e.eq_ignore_ascii_case(part)) {
            target.push(part.to_owned());
        }
    }
}

/// Infers `(series_title, chapter_label, chapter_number, volume)` from a clean filename or folder stem.
#[must_use]
pub(crate) fn infer_series_and_chapter_from_stem(
    stem: &str,
) -> (String, String, Option<f32>, Option<String>) {
    let cleaned = stem.replace('_', " ").trim().to_owned();
    if let Some(caps) = CHAPTER_SUFFIX_RE.captures(&cleaned)
        && let Some(m0) = caps.get(0)
        && let Some(num_str) = caps.get(1).map(|m| m.as_str())
    {
        let prefix = cleaned[..m0.start()]
            .trim_matches(|c: char| matches!(c, '-' | '_' | ' ' | ':' | '|'))
            .trim();
        let num = num_str.parse::<f32>().ok();
        let series = if prefix.is_empty() {
            cleaned.clone()
        } else {
            prefix.to_owned()
        };
        return (series, format!("Chapter {num_str}"), num, None);
    }

    if let Some(caps) = VOLUME_ONLY_RE.captures(&cleaned)
        && let Some(m0) = caps.get(0)
        && let Some(vol_str) = caps.get(1).map(|m| m.as_str())
    {
        let prefix = cleaned[..m0.start()]
            .trim_matches(|c: char| matches!(c, '-' | '_' | ' ' | ':' | '|'))
            .trim();
        let num = vol_str.parse::<f32>().ok();
        let series = if prefix.is_empty() {
            cleaned.clone()
        } else {
            prefix.to_owned()
        };
        return (
            series,
            format!("Volume {vol_str}"),
            num,
            Some(vol_str.to_owned()),
        );
    }

    if let Some(caps) = TRAILING_NUM_RE.captures(&cleaned)
        && let Some(ser) = caps.name("series").map(|m| m.as_str().trim())
        && let Some(num_str) = caps.name("num").map(|m| m.as_str())
        && !ser.is_empty()
    {
        let num = num_str.parse::<f32>().ok();
        return (ser.to_owned(), format!("Chapter {num_str}"), num, None);
    }

    (cleaned.clone(), cleaned, Some(1.0), None)
}

/// Strips the internal staging prefix (`stg_<16hex>__`) when deriving display titles from staged uploads.
#[must_use]
pub(crate) fn strip_staging_prefix(raw_stem: &str) -> &str {
    if let Some(rest) = raw_stem.strip_prefix("stg_")
        && let Some((hex_part, actual_stem)) = rest.split_once("__")
        && hex_part.len() == 16
        && !actual_stem.trim().is_empty()
    {
        actual_stem.trim()
    } else {
        raw_stem.trim()
    }
}

/// Produces a filesystem-safe, deterministic series slug with a short SHA-256 suffix.
#[must_use]
pub(crate) fn slugify_series(title: &str) -> String {
    let mut out = String::new();
    let mut prev_dash = false;
    for ch in title.trim().chars() {
        if ch.is_alphanumeric() {
            for low in ch.to_lowercase() {
                out.push(low);
            }
            prev_dash = false;
        } else if !prev_dash && !out.is_empty() {
            out.push('-');
            prev_dash = true;
        }
    }
    let clean = out.trim_matches('-').to_owned();
    if clean.is_empty() {
        let mut h = Sha256::new();
        h.update(title.as_bytes());
        let d = h.finalize();
        format!("series-{:02x}{:02x}{:02x}{:02x}", d[0], d[1], d[2], d[3])
    } else {
        let mut h = Sha256::new();
        h.update(clean.as_bytes());
        let d = h.finalize();
        let prefix: String = clean.chars().take(36).collect();
        format!("{}-{:02x}{:02x}", prefix.trim_end_matches('-'), d[0], d[1])
    }
}

/// Computes a fast deterministic 20-hex-char fingerprint from file length + head 64 KiB + tail 64 KiB.
///
/// # Errors
///
/// Returns [`AppError`] if the file cannot be read or exceeds [`MAX_COMIC_ARCHIVE_BYTES`].
pub(crate) fn compute_fast_file_fingerprint(
    file: &mut std::fs::File,
    file_size: u64,
) -> Result<String, AppError> {
    use std::io::{Read as _, Seek as _, SeekFrom};

    if file_size > MAX_COMIC_ARCHIVE_BYTES {
        return Err(AppError::CorruptedData {
            message: "comic archive exceeds maximum permitted 1.5 GiB limit".to_owned(),
        });
    }

    let mut hasher = Sha256::new();
    hasher.update(file_size.to_le_bytes());

    file.seek(SeekFrom::Start(0))
        .map_err(|e| AppError::Internal {
            message: e.to_string(),
        })?;
    let head_len = usize::try_from(file_size.min(64 * 1024)).unwrap_or(0);
    let mut buf = vec![0_u8; head_len];
    file.read_exact(&mut buf).map_err(|e| AppError::Internal {
        message: e.to_string(),
    })?;
    hasher.update(&buf);

    if file_size > 64 * 1024 {
        let tail_len = usize::try_from(file_size.min(64 * 1024)).unwrap_or(0);
        let tail_pos = file_size.saturating_sub(u64::try_from(tail_len).unwrap_or(0));
        file.seek(SeekFrom::Start(tail_pos))
            .map_err(|e| AppError::Internal {
                message: e.to_string(),
            })?;
        let mut tail = vec![0_u8; tail_len];
        file.read_exact(&mut tail).map_err(|e| AppError::Internal {
            message: e.to_string(),
        })?;
        hasher.update(&tail);
    }

    let digest = hasher.finalize();
    let mut hex = String::with_capacity(20);
    for b in &digest[..10] {
        use std::fmt::Write;
        let _ = write!(&mut hex, "{b:02x}");
    }
    Ok(hex)
}

/// Computes a fast deterministic 20-hex-char fingerprint for a collection of loose image paths.
///
/// # Errors
///
/// Returns [`AppError`] if any image file cannot be stated or read.
pub(crate) fn compute_images_fingerprint(
    paths: &[std::path::PathBuf],
) -> Result<(String, u64), AppError> {
    use std::io::Read as _;

    let mut hasher = Sha256::new();
    let mut total_size: u64 = 0;

    for p in paths {
        let meta = std::fs::metadata(p).map_err(|e| AppError::NotFound {
            message: format!("cannot stat image file: {e}"),
        })?;
        let sz = meta.len();
        total_size = total_size.saturating_add(sz);
        hasher.update(sz.to_le_bytes());
        if let Some(name) = p.file_name().and_then(|n| n.to_str()) {
            hasher.update(name.as_bytes());
        }
        if let Ok(mut f) = std::fs::File::open(p) {
            let n = usize::try_from(sz.min(4096)).unwrap_or(0);
            let mut head = vec![0_u8; n];
            if f.read_exact(&mut head).is_ok() {
                hasher.update(&head);
            }
        }
    }

    let digest = hasher.finalize();
    let mut hex = String::with_capacity(20);
    for b in &digest[..10] {
        use std::fmt::Write;
        let _ = write!(&mut hex, "{b:02x}");
    }
    Ok((hex, total_size))
}

/// Extracts clean stem string from a path (stripping internal staging prefixes if any).
#[must_use]
pub(crate) fn clean_stem_from_path(path: &Path, fallback: &str) -> String {
    let raw_stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(fallback)
        .trim();
    let stripped = strip_staging_prefix(raw_stem);
    if stripped.is_empty() {
        fallback.to_owned()
    } else {
        stripped.to_owned()
    }
}
