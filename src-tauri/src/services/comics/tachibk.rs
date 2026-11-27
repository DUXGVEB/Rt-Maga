//! Pure-Rust Mihon / Tachiyomi / Suwayomi `.tachibk` (and legacy `.proto.gz`) backup decoder,
//! format detector, preview builder, and transactional library reconciliation engine.
//!
//! Uses `flate2` (pure-Rust GZIP decompression with strict bomb guards) and `prost` derive structs
//! without requiring `prost-build` or an external `protoc` binary.

use std::cmp::Ordering;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::io::Read;
use std::path::{Path, PathBuf};

use flate2::read::GzDecoder;
use mangacore::parse::chapters::extract_chapter_number;
use prost::Message;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{QueryBuilder, Sqlite};
use url::Url;

use crate::cache::image::CoverHydrationTarget;
use crate::error::AppError;
use crate::image_proxy::encode_cover_token;
use crate::repo::manga::MangaMatchCandidate;
use crate::repo::now_unix_secs;
use crate::services::comics::sort::{natural_cmp, push_csv_unique, slugify_series};
use crate::services::comics::tar_src::looks_like_tar_header;
use crate::services::comics::{DetectedFileKind, detect_format_magic, is_local_site};
use crate::state::AppState;

/// Synthetic `manga.site` value identifying metadata-only catalog entries imported from `.tachibk`
/// backups that do not have local comic archive files linked yet.
pub(crate) const TACHIBK_SITE_KEY: &str = "local:tachibk";

/// Synthetic URL prefix used for metadata-only `.tachibk` series and chapter rows.
pub(crate) const TACHIBK_URL_PREFIX: &str = "https://tachibk.local/";

/// Marker persisted in `chapter.source_id` when a metadata-only `.tachibk` chapter was bookmarked.
pub(crate) const TACHIBK_BOOKMARKED_CHAPTER_TAG: &str = "tachibk:bookmarked";

/// Maximum compressed `.tachibk` / `.proto.gz` input file size permitted (64 MiB).
pub(crate) const MAX_TACHIBK_COMPRESSED_BYTES: u64 = 64 * 1024 * 1024;

/// Maximum decompressed protobuf stream size permitted (128 MiB) to guard against GZIP bombs.
pub(crate) const MAX_TACHIBK_DECOMPRESSED_BYTES: u64 = 128 * 1024 * 1024;

/// Maximum number of manga entries accepted from a single backup file.
const MAX_TACHIBK_MANGA_ENTRIES: usize = 50_000;

/// GZIP magic header prefix (`0x1F 0x8B`).
const GZIP_MAGIC: [u8; 2] = [0x1F, 0x8B];

// ---------------------------------------------------------------------------
// Standard Mihon / Tachiyomi / Suwayomi Protobuf wire schema
// ---------------------------------------------------------------------------

/// Root Mihon / Tachiyomi `Backup` protobuf message.
#[derive(Clone, PartialEq, Message)]
#[expect(
    clippy::struct_field_names,
    reason = "matches standard Mihon/Tachiyomi Backup protobuf schema field names"
)]
pub(crate) struct Backup {
    #[prost(message, repeated, tag = "1")]
    pub backup_manga: Vec<BackupManga>,
    #[prost(message, repeated, tag = "2")]
    pub backup_categories: Vec<BackupCategory>,
    #[prost(message, repeated, tag = "101")]
    pub backup_sources: Vec<BackupSource>,
}

/// Library category entry (`eu.kanade.tachiyomi.data.backup.models.BackupCategory`).
#[derive(Clone, PartialEq, Eq, Message)]
pub(crate) struct BackupCategory {
    #[prost(string, tag = "1")]
    pub name: String,
    #[prost(int64, tag = "2")]
    pub order: i64,
    #[prost(int64, optional, tag = "3")]
    pub id: Option<i64>,
    #[prost(int64, tag = "100")]
    pub flags: i64,
}

/// Extension source descriptor (`eu.kanade.tachiyomi.data.backup.models.BackupSource`).
#[derive(Clone, PartialEq, Eq, Message)]
pub(crate) struct BackupSource {
    #[prost(string, tag = "1")]
    pub name: String,
    #[prost(int64, tag = "2")]
    pub source_id: i64,
}

/// Manga entry (`eu.kanade.tachiyomi.data.backup.models.BackupManga`).
#[derive(Clone, PartialEq, Message)]
pub(crate) struct BackupManga {
    #[prost(int64, tag = "1")]
    pub source: i64,
    #[prost(string, tag = "2")]
    pub url: String,
    #[prost(string, tag = "3")]
    pub title: String,
    #[prost(string, optional, tag = "4")]
    pub artist: Option<String>,
    #[prost(string, optional, tag = "5")]
    pub author: Option<String>,
    #[prost(string, optional, tag = "6")]
    pub description: Option<String>,
    #[prost(string, repeated, tag = "7")]
    pub genre: Vec<String>,
    #[prost(int32, tag = "8")]
    pub status: i32,
    #[prost(string, optional, tag = "9")]
    pub thumbnail_url: Option<String>,
    #[prost(int64, tag = "13")]
    pub date_added: i64,
    #[prost(int32, tag = "14")]
    pub viewer: i32,
    #[prost(message, repeated, tag = "16")]
    pub chapters: Vec<BackupChapter>,
    #[prost(int64, repeated, tag = "17")]
    pub categories: Vec<i64>,
    #[prost(message, repeated, tag = "18")]
    pub tracking: Vec<BackupTracking>,
    /// In Mihon/Tachiyomi `favorite` defaults to `true` when omitted on the wire.
    #[prost(bool, optional, tag = "100")]
    pub favorite: Option<bool>,
    #[prost(int32, tag = "101")]
    pub chapter_flags: i32,
    #[prost(int32, optional, tag = "103")]
    pub viewer_flags: Option<i32>,
    #[prost(message, repeated, tag = "104")]
    pub history: Vec<BackupHistory>,
    #[prost(int32, tag = "105")]
    pub update_strategy: i32,
    #[prost(int64, tag = "106")]
    pub last_modified_at: i64,
    #[prost(int64, optional, tag = "107")]
    pub favorite_modified_at: Option<i64>,
    #[prost(string, repeated, tag = "108")]
    pub excluded_scanlators: Vec<String>,
    #[prost(int64, tag = "109")]
    pub version: i64,
    #[prost(string, tag = "110")]
    pub notes: String,
    #[prost(bool, tag = "111")]
    pub initialized: bool,
    /// Optional fork custom title (`TachiyomiSY` / J2K / Yokai).
    #[prost(string, optional, tag = "800")]
    pub custom_title: Option<String>,
}

/// Chapter record (`eu.kanade.tachiyomi.data.backup.models.BackupChapter`).
#[derive(Clone, PartialEq, Message)]
pub(crate) struct BackupChapter {
    #[prost(string, tag = "1")]
    pub url: String,
    #[prost(string, tag = "2")]
    pub name: String,
    #[prost(string, optional, tag = "3")]
    pub scanlator: Option<String>,
    #[prost(bool, tag = "4")]
    pub read: bool,
    #[prost(bool, tag = "5")]
    pub bookmark: bool,
    #[prost(int64, tag = "6")]
    pub last_page_read: i64,
    #[prost(int64, tag = "7")]
    pub date_fetch: i64,
    #[prost(int64, tag = "8")]
    pub date_upload: i64,
    #[prost(float, tag = "9")]
    pub chapter_number: f32,
    #[prost(int64, tag = "10")]
    pub source_order: i64,
    #[prost(int64, tag = "11")]
    pub last_modified_at: i64,
    #[prost(int64, tag = "12")]
    pub version: i64,
}

/// Reading history entry (`eu.kanade.tachiyomi.data.backup.models.BackupHistory`).
#[derive(Clone, PartialEq, Eq, Message)]
pub(crate) struct BackupHistory {
    #[prost(string, tag = "1")]
    pub url: String,
    #[prost(int64, tag = "2")]
    pub last_read: i64,
    #[prost(int64, tag = "3")]
    pub read_duration: i64,
}

/// External tracker binding (`eu.kanade.tachiyomi.data.backup.models.BackupTracking`).
#[derive(Clone, PartialEq, Message)]
pub(crate) struct BackupTracking {
    #[prost(int32, tag = "1")]
    pub sync_id: i32,
    #[prost(int64, tag = "2")]
    pub library_id: i64,
    #[prost(int32, tag = "3")]
    pub media_id_int: i32,
    #[prost(string, tag = "4")]
    pub tracking_url: String,
    #[prost(string, tag = "5")]
    pub title: String,
    #[prost(float, tag = "6")]
    pub last_chapter_read: f32,
    #[prost(int32, tag = "7")]
    pub total_chapters: i32,
    #[prost(float, tag = "8")]
    pub score: f32,
    #[prost(int32, tag = "9")]
    pub status: i32,
    #[prost(int64, tag = "10")]
    pub started_reading_date: i64,
    #[prost(int64, tag = "11")]
    pub finished_reading_date: i64,
    #[prost(int64, tag = "100")]
    pub media_id: i64,
}

impl BackupManga {
    /// Returns the preferred display title (`custom_title` when non-empty, otherwise `title`).
    #[must_use]
    pub(crate) fn display_title(&self) -> &str {
        self.custom_title
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| self.title.trim())
    }

    /// Returns whether this entry is marked favorite in Mihon/Tachiyomi (defaults to `true` when omitted).
    #[must_use]
    pub(crate) fn is_favorite(&self) -> bool {
        self.favorite.unwrap_or(true)
    }
}

// ---------------------------------------------------------------------------
// Public IPC DTOs
// ---------------------------------------------------------------------------

/// Classification of how a `.tachibk` entry matches an existing `RtMaga` library entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub(crate) enum TachibkMatchKind {
    /// No existing entry found; will be created as a metadata-only catalog entry if enabled.
    None,
    /// Matched an existing local comic entry with offline archive/image files linked.
    LocalLibrary,
    /// Matched an existing web-tracked manga entry in the library.
    WebLibrary,
    /// Matched an existing metadata-only catalog entry previously imported from a backup.
    CatalogEntry,
}

/// Summary of one Mihon/Tachiyomi category present in the backup.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TachibkCategoryPreview {
    pub name: String,
    pub order: i64,
    pub manga_count: usize,
}

/// Structured preview of a single manga inside a `.tachibk` backup.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TachibkMangaPreview {
    pub index: usize,
    pub title: String,
    pub author: Option<String>,
    pub artist: Option<String>,
    pub description: Option<String>,
    pub genres: Vec<String>,
    pub categories: Vec<String>,
    pub status: Option<String>,
    pub cover_token: Option<String>,
    pub source_name: Option<String>,
    pub favorite: bool,
    pub total_chapters: usize,
    pub read_chapters: usize,
    pub bookmarked_chapters: usize,
    pub last_read_at: Option<i64>,
    pub date_added: Option<i64>,
    pub tracking_count: usize,
    pub matched_manga_id: Option<i64>,
    pub matched_manga_title: Option<String>,
    pub match_kind: TachibkMatchKind,
}

/// Full preview response returned before committing a `.tachibk` backup import.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TachibkBackupPreview {
    pub source_path: Option<String>,
    pub upload_id: Option<String>,
    pub file_name: String,
    pub file_size: u64,
    pub total_manga: usize,
    pub matched_local_count: usize,
    pub matched_existing_count: usize,
    pub unmatched_count: usize,
    pub total_chapters: usize,
    pub total_read_chapters: usize,
    pub categories: Vec<TachibkCategoryPreview>,
    pub entries: Vec<TachibkMangaPreview>,
}

/// User-configured import request submitted from the `.tachibk` preview modal.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
#[expect(
    clippy::struct_excessive_bools,
    reason = "IPC import options DTO carrying four independent user toggle flags"
)]
pub(crate) struct TachibkImportRequest {
    pub source_path: Option<String>,
    pub upload_id: Option<String>,
    pub selected_indices: Vec<usize>,
    pub sync_reading_progress: bool,
    pub import_metadata_only: bool,
    pub import_categories: bool,
    pub import_bookmarks: bool,
}

/// Summary report returned after committing a `.tachibk` backup import into `SQLite`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TachibkImportReport {
    pub linked_local_count: usize,
    pub updated_existing_count: usize,
    pub created_catalog_count: usize,
    pub skipped_count: usize,
    pub synced_chapters_read: usize,
    pub synced_bookmarks: usize,
    pub synced_history: usize,
    pub imported_manga_ids: Vec<i64>,
}

// ---------------------------------------------------------------------------
// Detection, Decompression & Protobuf Decoding
// ---------------------------------------------------------------------------

/// Returns `true` when `url` belongs to a synthetic metadata-only `.tachibk` catalog entry.
#[must_use]
pub(crate) fn is_tachibk_url(url: &str) -> bool {
    url.starts_with(TACHIBK_URL_PREFIX)
}

/// Returns `true` when `header` begins with the GZIP magic bytes `0x1F 0x8B`.
#[must_use]
pub(crate) fn is_gzip_magic(header: &[u8]) -> bool {
    header.len() >= 2 && header[0..2] == GZIP_MAGIC
}

/// Returns `true` when `path` carries a known Mihon/Tachiyomi backup extension (`.tachibk` or `.proto.gz`).
#[must_use]
pub(crate) fn has_tachibk_extension(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
        return false;
    };
    let lower = name.to_ascii_lowercase();
    lower.ends_with(".tachibk") || lower.ends_with(".proto.gz") || lower.ends_with(".tachibk.gz")
}

/// Safely decompresses a GZIP byte slice with a strict upper bound to prevent decompression bombs,
/// rejects GZIP-compressed TAR archives (`.tar.gz` / `.cbt`), and decodes a Mihon/Tachiyomi `Backup`.
///
/// # Errors
///
/// Returns [`AppError::UnsupportedFormat`] if the payload is not GZIP or is a compressed TAR archive,
/// or [`AppError::CorruptedData`] if the stream exceeds size limits or fails protobuf decoding.
pub(crate) fn decode_tachibk_bytes(raw_gz: &[u8]) -> Result<Backup, AppError> {
    let compressed_len = u64::try_from(raw_gz.len()).unwrap_or(u64::MAX);
    if compressed_len == 0 {
        return Err(AppError::CorruptedData {
            message: "backup file is empty (0 bytes)".to_owned(),
        });
    }
    if compressed_len > MAX_TACHIBK_COMPRESSED_BYTES {
        return Err(AppError::CorruptedData {
            message: "backup file exceeds maximum permitted 64 MiB compressed size limit"
                .to_owned(),
        });
    }
    if !is_gzip_magic(raw_gz) {
        return Err(AppError::UnsupportedFormat {
            message: "file does not begin with GZIP magic bytes (0x1F 0x8B)".to_owned(),
        });
    }

    let decompressed = decompress_gzip_bounded(raw_gz, MAX_TACHIBK_DECOMPRESSED_BYTES)?;
    if looks_like_tar_header(&decompressed) {
        return Err(AppError::UnsupportedFormat {
            message: "compressed TAR archive (.tar.gz/.cbt) is not a Mihon/Tachiyomi backup"
                .to_owned(),
        });
    }

    let backup = Backup::decode(decompressed.as_slice()).map_err(|e| AppError::CorruptedData {
        message: format!("invalid Mihon/Tachiyomi backup protobuf payload: {e}"),
    })?;

    if backup.backup_manga.len() > MAX_TACHIBK_MANGA_ENTRIES {
        return Err(AppError::CorruptedData {
            message: format!(
                "backup contains {} manga entries, exceeding the {MAX_TACHIBK_MANGA_ENTRIES} entry safety limit",
                backup.backup_manga.len()
            ),
        });
    }

    Ok(backup)
}

/// Decompresses a GZIP stream while enforcing `max_bytes` so malicious GZIP bombs cannot exhaust RAM.
fn decompress_gzip_bounded<R: Read>(reader: R, max_bytes: u64) -> Result<Vec<u8>, AppError> {
    let decoder = GzDecoder::new(reader);
    let cap = max_bytes.saturating_add(1);
    let mut limited = decoder.take(cap);
    let mut out = Vec::new();
    limited
        .read_to_end(&mut out)
        .map_err(|e| AppError::CorruptedData {
            message: format!("failed to decompress GZIP backup stream: {e}"),
        })?;
    let out_len = u64::try_from(out.len()).unwrap_or(u64::MAX);
    if out_len > max_bytes {
        return Err(AppError::CorruptedData {
            message: format!(
                "decompressed backup exceeds maximum permitted {} MiB safety limit",
                max_bytes / (1024 * 1024)
            ),
        });
    }
    Ok(out)
}

/// Opens, size-checks, decompresses, and decodes a `.tachibk` / `.proto.gz` file on disk.
///
/// # Errors
///
/// Returns [`AppError`] if the file cannot be read, exceeds safety limits, is a `.tar.gz`,
/// or fails protobuf decoding.
pub(crate) fn decode_tachibk_file_sync(path: &Path) -> Result<(Backup, u64), AppError> {
    let meta = std::fs::metadata(path).map_err(|e| AppError::NotFound {
        message: format!("cannot access backup file: {e}"),
    })?;
    if !meta.is_file() {
        return Err(AppError::UnsupportedFormat {
            message: "selected backup path is not a file".to_owned(),
        });
    }
    let file_size = meta.len();
    if file_size == 0 {
        return Err(AppError::CorruptedData {
            message: "backup file is empty (0 bytes)".to_owned(),
        });
    }
    if file_size > MAX_TACHIBK_COMPRESSED_BYTES {
        return Err(AppError::CorruptedData {
            message: "backup file exceeds maximum permitted 64 MiB compressed size limit"
                .to_owned(),
        });
    }

    let raw = std::fs::read(path).map_err(|e| AppError::CorruptedData {
        message: format!("failed to read backup file: {e}"),
    })?;
    let backup = decode_tachibk_bytes(&raw)?;
    Ok((backup, file_size))
}

/// Checks whether a GZIP file at `path` contains a valid Mihon/Tachiyomi `Backup` protobuf payload
/// (and is definitively not a `.tar.gz` / gzipped `.cbt` archive).
pub(crate) fn probe_tachibk_file_sync(path: &Path) -> bool {
    let Ok(meta) = std::fs::metadata(path) else {
        return false;
    };
    if !meta.is_file() || meta.len() == 0 || meta.len() > MAX_TACHIBK_COMPRESSED_BYTES {
        return false;
    }

    let Ok(mut file) = std::fs::File::open(path) else {
        return false;
    };
    let mut magic = [0_u8; 3];
    if file.read_exact(&mut magic).is_err() || !is_gzip_magic(&magic) {
        return false;
    }

    // Fast prefix check: immediately exclude gzipped TAR archives (`.tar.gz` / `.cbt`).
    let Ok(prefix_file) = std::fs::File::open(path) else {
        return false;
    };
    let mut prefix_buf = Vec::with_capacity(1024);
    if GzDecoder::new(prefix_file)
        .take(1024)
        .read_to_end(&mut prefix_buf)
        .is_err()
    {
        return false;
    }
    if looks_like_tar_header(&prefix_buf) {
        return false;
    }

    let Ok((backup, _)) = decode_tachibk_file_sync(path) else {
        return false;
    };

    let has_recognizable_payload = !backup.backup_manga.is_empty()
        || !backup.backup_categories.is_empty()
        || !backup.backup_sources.is_empty();

    has_recognizable_payload || has_tachibk_extension(path)
}

/// Filters `paths` and returns those detected as valid `.tachibk` / `.proto.gz` backup files
/// using binary magic-byte + protobuf validation (never false-matching `.cbt`/`.tar.gz`).
pub(crate) async fn detect_tachibk_paths(paths: &[PathBuf]) -> Vec<String> {
    let owned = paths.to_vec();
    tokio::task::spawn_blocking(move || {
        let mut matched = Vec::new();
        for p in owned {
            if matches!(detect_format_magic(&p), Ok(DetectedFileKind::Tachibk)) {
                matched.push(p.to_string_lossy().into_owned());
            }
        }
        matched
    })
    .await
    .unwrap_or_default()
}

// ---------------------------------------------------------------------------
// Normalization & Matching Helpers
// ---------------------------------------------------------------------------

/// Normalizes a manga or chapter title into a punctuation-insensitive, lowercase,
/// single-space-collapsed comparison key for fuzzy local library matching.
#[must_use]
pub(crate) fn normalize_match_title(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut prev_space = true;
    for ch in raw.chars() {
        if ch.is_alphanumeric() {
            for low in ch.to_lowercase() {
                out.push(low);
            }
            prev_space = false;
        } else if !prev_space {
            out.push(' ');
            prev_space = true;
        }
    }
    out.trim().to_owned()
}

/// Maps Mihon/Tachiyomi `SManga` integer status codes to human-readable status labels.
#[must_use]
pub(crate) fn map_mihon_status(code: i32) -> Option<&'static str> {
    match code {
        1 => Some("ongoing"),
        2 => Some("completed"),
        3 => Some("licensed"),
        4 => Some("publishing finished"),
        5 => Some("cancelled"),
        6 => Some("hiatus"),
        _ => None,
    }
}

/// Converts a Mihon/Tachiyomi timestamp (milliseconds or seconds) into positive Unix seconds.
#[must_use]
pub(crate) fn normalize_unix_secs(raw: i64) -> Option<i64> {
    if raw <= 0 {
        return None;
    }
    if raw > 10_000_000_000 {
        Some(raw / 1000)
    } else {
        Some(raw)
    }
}

/// Formats a Unix timestamp in seconds as `YYYY-MM-DD` (UTC) without external date crates.
#[must_use]
fn format_unix_date(secs: i64) -> Option<String> {
    if secs <= 0 {
        return None;
    }
    let days = secs.div_euclid(86_400);
    // Howard Hinnant's civil-from-days algorithm (exact Gregorian calendar).
    let z = days.checked_add(719_468)?;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if m <= 2 { y + 1 } else { y };
    if !(1970..=9999).contains(&year) {
        return None;
    }
    Some(format!("{year:04}-{m:02}-{d:02}"))
}

/// Validates an untrusted cover URL from a `.tachibk` backup against SSRF/local-network rules.
#[must_use]
fn sanitize_public_cover_url(raw: Option<&str>) -> Option<String> {
    let trimmed = raw?.trim();
    if trimmed.is_empty() {
        return None;
    }
    let parsed = Url::parse(trimmed).ok()?;
    mangacore::http::validate_public_http_url(&parsed).ok()?;
    Some(parsed.into())
}

fn cover_referer_for(backup_url: &str, cover_url: &str) -> String {
    if let Ok(parsed) = Url::parse(backup_url.trim())
        && mangacore::http::validate_public_http_url(&parsed).is_ok()
    {
        return parsed.into();
    }
    if let Ok(parsed) = Url::parse(cover_url)
        && let Some(host) = parsed.host_str()
    {
        return format!("{}://{host}/", parsed.scheme());
    }
    cover_url.to_owned()
}

struct CategoryIndex {
    by_order: HashMap<i64, String>,
    by_id: HashMap<i64, String>,
    ordered: Vec<(i64, String)>,
}

impl CategoryIndex {
    fn from_backup(categories: &[BackupCategory]) -> Self {
        let mut by_order = HashMap::with_capacity(categories.len());
        let mut by_id = HashMap::with_capacity(categories.len());
        let mut ordered = Vec::with_capacity(categories.len());

        for cat in categories {
            let name = cat.name.trim();
            if name.is_empty() {
                continue;
            }
            by_order.entry(cat.order).or_insert_with(|| name.to_owned());
            if let Some(id) = cat.id {
                by_id.entry(id).or_insert_with(|| name.to_owned());
            }
            ordered.push((cat.order, name.to_owned()));
        }
        ordered.sort_by_key(|(order, _)| *order);

        Self {
            by_order,
            by_id,
            ordered,
        }
    }

    fn resolve_for_manga(&self, refs: &[i64]) -> Vec<String> {
        let mut seen = HashSet::with_capacity(refs.len());
        let mut out = Vec::with_capacity(refs.len());
        for &cat_ref in refs {
            let resolved = self
                .by_order
                .get(&cat_ref)
                .or_else(|| self.by_id.get(&cat_ref))
                .or_else(|| {
                    usize::try_from(cat_ref)
                        .ok()
                        .and_then(|idx| self.ordered.get(idx).map(|(_, name)| name))
                });
            if let Some(name) = resolved
                && seen.insert(name.to_ascii_lowercase())
            {
                out.push(name.clone());
            }
        }
        out
    }
}

#[derive(Debug, Clone)]
struct MatchTarget {
    id: i64,
    title: String,
    match_kind: TachibkMatchKind,
    priority: u8,
}

struct LibraryMatcher {
    by_norm_title: HashMap<String, MatchTarget>,
}

impl LibraryMatcher {
    fn from_candidates(candidates: &[MangaMatchCandidate]) -> Self {
        let mut by_norm_title: HashMap<String, MatchTarget> =
            HashMap::with_capacity(candidates.len() * 2);

        for row in candidates {
            let (match_kind, priority) = if row.site == TACHIBK_SITE_KEY {
                (TachibkMatchKind::CatalogEntry, 1)
            } else if row.has_local_files != 0 || is_local_site(&row.site) {
                (TachibkMatchKind::LocalLibrary, 3)
            } else {
                (TachibkMatchKind::WebLibrary, 2)
            };

            let target = MatchTarget {
                id: row.id,
                title: row.title.clone(),
                match_kind,
                priority,
            };

            Self::insert_key(&mut by_norm_title, &row.title, &target);
            if let Some(ref orig) = row.original_title {
                Self::insert_key(&mut by_norm_title, orig, &target);
            }
            if let Some(ref raw_alts) = row.alt_titles_json
                && let Ok(alts) = serde_json::from_str::<Vec<String>>(raw_alts)
            {
                for alt in alts {
                    Self::insert_key(&mut by_norm_title, &alt, &target);
                }
            }
        }

        Self { by_norm_title }
    }

    fn insert_key(map: &mut HashMap<String, MatchTarget>, raw_title: &str, target: &MatchTarget) {
        let key = normalize_match_title(raw_title);
        if key.is_empty() {
            return;
        }
        match map.get(&key) {
            Some(existing) if existing.priority >= target.priority => {}
            _ => {
                map.insert(key, target.clone());
            }
        }
    }

    fn find_match(&self, entry: &BackupManga) -> Option<&MatchTarget> {
        let primary = normalize_match_title(entry.display_title());
        if !primary.is_empty()
            && let Some(hit) = self.by_norm_title.get(&primary)
        {
            return Some(hit);
        }

        let orig = normalize_match_title(&entry.title);
        if !orig.is_empty()
            && orig != primary
            && let Some(hit) = self.by_norm_title.get(&orig)
        {
            return Some(hit);
        }

        for track in &entry.tracking {
            let track_key = normalize_match_title(&track.title);
            if !track_key.is_empty()
                && let Some(hit) = self.by_norm_title.get(&track_key)
            {
                return Some(hit);
            }
        }

        None
    }
}

fn best_last_read_secs(entry: &BackupManga) -> Option<i64> {
    entry
        .history
        .iter()
        .filter_map(|h| normalize_unix_secs(h.last_read))
        .max()
        .or_else(|| {
            entry
                .chapters
                .iter()
                .filter(|c| c.read || c.last_page_read > 0)
                .filter_map(|c| normalize_unix_secs(c.last_modified_at))
                .max()
        })
}

fn parsed_backup_chapter_number(ch: &BackupChapter) -> Option<f64> {
    if ch.chapter_number.is_finite() && ch.chapter_number >= 0.0 {
        Some(f64::from(ch.chapter_number))
    } else {
        extract_chapter_number(&ch.name).map(f64::from)
    }
}

fn sorted_backup_chapters(chapters: &[BackupChapter]) -> Vec<&BackupChapter> {
    let mut list: Vec<&BackupChapter> = chapters.iter().collect();
    list.sort_by(|a, b| {
        let na = parsed_backup_chapter_number(a);
        let nb = parsed_backup_chapter_number(b);
        match (na, nb) {
            (Some(x), Some(y)) => x
                .partial_cmp(&y)
                .unwrap_or(Ordering::Equal)
                .then_with(|| b.source_order.cmp(&a.source_order))
                .then_with(|| natural_cmp(&a.name, &b.name)),
            (Some(_), None) => Ordering::Less,
            (None, Some(_)) => Ordering::Greater,
            // In Mihon/Tachiyomi, higher `source_order` is usually older (ascending reading order).
            (None, None) => b
                .source_order
                .cmp(&a.source_order)
                .then_with(|| natural_cmp(&a.name, &b.name)),
        }
    });
    list
}

fn short_entry_hash(source: i64, url: &str, title: &str) -> String {
    let mut h = Sha256::new();
    h.update(source.to_le_bytes());
    h.update(b"|");
    h.update(url.as_bytes());
    h.update(b"|");
    h.update(title.as_bytes());
    let d = h.finalize();
    format!("{:02x}{:02x}{:02x}{:02x}", d[0], d[1], d[2], d[3])
}

fn split_people(raw: Option<&str>) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(val) = raw {
        for part in val.split([';', '\n']) {
            push_csv_unique(&mut out, part);
        }
    }
    out
}

fn merge_json_string_vecs(existing_json: Option<&str>, incoming: &[String]) -> Option<String> {
    let mut combined: Vec<String> = existing_json
        .and_then(|s| serde_json::from_str::<Vec<String>>(s).ok())
        .unwrap_or_default();
    let mut seen: HashSet<String> = combined.iter().map(|s| s.to_ascii_lowercase()).collect();
    for item in incoming {
        let trimmed = item.trim();
        if !trimmed.is_empty() && seen.insert(trimmed.to_ascii_lowercase()) {
            combined.push(trimmed.to_owned());
        }
    }
    if combined.is_empty() {
        None
    } else {
        serde_json::to_string(&combined).ok()
    }
}

fn validate_staged_upload_path(data_dir: &Path, upload_id: &str) -> Result<PathBuf, AppError> {
    if !upload_id.starts_with("stg_")
        || upload_id.contains("..")
        || upload_id.contains('/')
        || upload_id.contains('\\')
        || upload_id.len() > 240
    {
        return Err(AppError::InvalidUrl {
            message: "invalid backup staging identifier".to_owned(),
        });
    }
    Ok(data_dir.join("comics").join(".staging").join(upload_id))
}

// ---------------------------------------------------------------------------
// Preview Building
// ---------------------------------------------------------------------------

/// Parses a `.tachibk` / `.proto.gz` file at `path` and returns a structured preview with local
/// library reconciliation metadata.
///
/// # Errors
///
/// Returns [`AppError`] if the file cannot be decoded or querying the local library fails.
pub(crate) async fn preview_tachibk_from_path(
    state: &AppState,
    path: &Path,
) -> Result<TachibkBackupPreview, AppError> {
    let owned_path = path.to_path_buf();
    let (backup, file_size) =
        tokio::task::spawn_blocking(move || decode_tachibk_file_sync(&owned_path))
            .await
            .map_err(|e| AppError::Internal {
                message: format!("backup decode worker failed: {e}"),
            })??;

    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("backup.tachibk")
        .to_owned();

    build_preview(
        state,
        &backup,
        Some(path.to_string_lossy().into_owned()),
        None,
        file_name,
        file_size,
    )
    .await
}

/// Parses a staged `WebView` `.tachibk` / `.proto.gz` upload and returns a structured preview.
///
/// # Errors
///
/// Returns [`AppError`] if the staged file is invalid or decoding fails.
pub(crate) async fn preview_tachibk_from_upload(
    data_dir: &Path,
    state: &AppState,
    upload_id: &str,
) -> Result<TachibkBackupPreview, AppError> {
    let path = validate_staged_upload_path(data_dir, upload_id)?;
    let owned_path = path.clone();
    let decode_res = tokio::task::spawn_blocking(move || decode_tachibk_file_sync(&owned_path))
        .await
        .map_err(|e| AppError::Internal {
            message: format!("backup decode worker failed: {e}"),
        })?;

    let (backup, file_size) = match decode_res {
        Ok(pair) => pair,
        Err(err) => {
            let _ = tokio::fs::remove_file(&path).await;
            return Err(err);
        }
    };

    let clean_name = upload_id
        .split_once("__")
        .map_or(upload_id, |(_, rest)| rest)
        .to_owned();

    build_preview(
        state,
        &backup,
        None,
        Some(upload_id.to_owned()),
        clean_name,
        file_size,
    )
    .await
}

/// Deletes a temporary staged `.tachibk` upload when the user cancels the import preview modal.
///
/// # Errors
///
/// Returns [`AppError::InvalidUrl`] if `upload_id` fails path-safety validation.
pub(crate) async fn discard_tachibk_upload(
    data_dir: &Path,
    upload_id: &str,
) -> Result<(), AppError> {
    let path = validate_staged_upload_path(data_dir, upload_id)?;
    let _ = tokio::fs::remove_file(&path).await;
    Ok(())
}

async fn build_preview(
    state: &AppState,
    backup: &Backup,
    source_path: Option<String>,
    upload_id: Option<String>,
    file_name: String,
    file_size: u64,
) -> Result<TachibkBackupPreview, AppError> {
    let candidates = state.db.list_manga_match_candidates().await?;
    Ok(build_preview_with_candidates(
        backup,
        &candidates,
        &state.proxy_key,
        source_path,
        upload_id,
        file_name,
        file_size,
    ))
}

#[expect(
    clippy::too_many_lines,
    reason = "maps decoded protobuf entries, categories, sources, and local match stats into one preview DTO"
)]
pub(crate) fn build_preview_with_candidates(
    backup: &Backup,
    candidates: &[MangaMatchCandidate],
    proxy_key: &[u8; 32],
    source_path: Option<String>,
    upload_id: Option<String>,
    file_name: String,
    file_size: u64,
) -> TachibkBackupPreview {
    let cat_index = CategoryIndex::from_backup(&backup.backup_categories);
    let matcher = LibraryMatcher::from_candidates(candidates);
    let source_names: HashMap<i64, &str> = backup
        .backup_sources
        .iter()
        .filter(|s| !s.name.trim().is_empty())
        .map(|s| (s.source_id, s.name.trim()))
        .collect();

    let mut category_counts: HashMap<String, usize> = HashMap::new();
    let mut entries = Vec::with_capacity(backup.backup_manga.len());
    let mut matched_local_count = 0_usize;
    let mut matched_existing_count = 0_usize;
    let mut unmatched_count = 0_usize;
    let mut total_chapters = 0_usize;
    let mut total_read_chapters = 0_usize;

    for (index, item) in backup.backup_manga.iter().enumerate() {
        let title = item.display_title().to_owned();
        if title.is_empty() {
            continue;
        }
        let categories = cat_index.resolve_for_manga(&item.categories);
        for c in &categories {
            *category_counts.entry(c.to_ascii_lowercase()).or_insert(0) += 1;
        }

        let read_chapters = item.chapters.iter().filter(|c| c.read).count();
        let bookmarked_chapters = item.chapters.iter().filter(|c| c.bookmark).count();
        total_chapters = total_chapters.saturating_add(item.chapters.len());
        total_read_chapters = total_read_chapters.saturating_add(read_chapters);

        let hit_target = matcher.find_match(item);
        let (matched_manga_id, matched_manga_title, match_kind) = if let Some(hit) = hit_target {
            matched_existing_count += 1;
            if hit.match_kind == TachibkMatchKind::LocalLibrary {
                matched_local_count += 1;
            }
            (Some(hit.id), Some(hit.title.clone()), hit.match_kind)
        } else {
            unmatched_count += 1;
            (None, None, TachibkMatchKind::None)
        };

        let cover_token = sanitize_public_cover_url(item.thumbnail_url.as_deref()).map(|cover| {
            let referer = cover_referer_for(&item.url, &cover);
            encode_cover_token(
                &cover,
                &referer,
                matched_manga_id,
                Some(TACHIBK_SITE_KEY),
                proxy_key,
            )
        });

        let mut genres = Vec::with_capacity(item.genre.len());
        for g in &item.genre {
            push_csv_unique(&mut genres, g);
        }

        entries.push(TachibkMangaPreview {
            index,
            title,
            author: item
                .author
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_owned),
            artist: item
                .artist
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_owned),
            description: item
                .description
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_owned),
            genres,
            categories,
            status: map_mihon_status(item.status).map(str::to_owned),
            cover_token,
            source_name: source_names.get(&item.source).map(|s| (*s).to_owned()),
            favorite: item.is_favorite(),
            total_chapters: item.chapters.len(),
            read_chapters,
            bookmarked_chapters,
            last_read_at: best_last_read_secs(item),
            date_added: normalize_unix_secs(item.date_added),
            tracking_count: item.tracking.len(),
            matched_manga_id,
            matched_manga_title,
            match_kind,
        });
    }

    let mut seen_cats = HashSet::new();
    let categories: Vec<TachibkCategoryPreview> = cat_index
        .ordered
        .iter()
        .filter(|(_, name)| seen_cats.insert(name.to_ascii_lowercase()))
        .map(|(order, name)| TachibkCategoryPreview {
            name: name.clone(),
            order: *order,
            manga_count: category_counts
                .get(&name.to_ascii_lowercase())
                .copied()
                .unwrap_or(0),
        })
        .collect();

    TachibkBackupPreview {
        source_path,
        upload_id,
        file_name,
        file_size,
        total_manga: entries.len(),
        matched_local_count,
        matched_existing_count,
        unmatched_count,
        total_chapters,
        total_read_chapters,
        categories,
        entries,
    }
}

// ---------------------------------------------------------------------------
// Transactional Library Reconciliation & Import
// ---------------------------------------------------------------------------

/// Imports user-selected `.tachibk` entries into `SQLite` inside one atomic transaction:
/// - Matches existing local/library entries by normalized title or alias, merging metadata,
///   categories, bookmarks, chapter read progress, and last-read history.
/// - Creates metadata-only catalog entries (`site = "local:tachibk"`) for unmatched titles when
///   `request.import_metadata_only` is enabled.
///
/// # Errors
///
/// Returns [`AppError`] if the backup source cannot be decoded or the database transaction fails.
pub(crate) async fn import_tachibk_backup(
    data_dir: &Path,
    state: &AppState,
    request: &TachibkImportRequest,
) -> Result<TachibkImportReport, AppError> {
    let (backup_path, cleanup_upload) = if let Some(ref uid) = request.upload_id {
        (validate_staged_upload_path(data_dir, uid)?, true)
    } else if let Some(ref raw_path) = request.source_path {
        (PathBuf::from(raw_path), false)
    } else {
        return Err(AppError::InvalidUrl {
            message: "missing backup file path or staged upload identifier".to_owned(),
        });
    };

    let decode_path = backup_path.clone();
    let decode_res = tokio::task::spawn_blocking(move || decode_tachibk_file_sync(&decode_path))
        .await
        .map_err(|e| AppError::Internal {
            message: format!("backup decode worker failed: {e}"),
        })?;

    let (backup, _) = match decode_res {
        Ok(pair) => pair,
        Err(err) => {
            if cleanup_upload {
                let _ = tokio::fs::remove_file(&backup_path).await;
            }
            return Err(err);
        }
    };

    let report = apply_backup_to_db(&state.db, &backup, request).await;

    if cleanup_upload {
        let _ = tokio::fs::remove_file(&backup_path).await;
    }

    if let Ok(ref ok_report) = report {
        state.chapters.invalidate_all();
        queue_tachibk_cover_hydration(state, &ok_report.imported_manga_ids).await;
    }

    report
}

async fn queue_tachibk_cover_hydration(state: &AppState, imported_manga_ids: &[i64]) {
    if imported_manga_ids.is_empty() {
        return;
    }
    let mut targets = Vec::with_capacity(imported_manga_ids.len());
    for &manga_id in imported_manga_ids {
        let Ok(Some(row)) = state.db.get_manga_by_id(manga_id).await else {
            continue;
        };
        let Some(raw_cover) = row.cover_url.as_deref() else {
            continue;
        };
        if crate::services::comics::is_local_comic_url(raw_cover) || is_tachibk_url(raw_cover) {
            continue;
        }
        let Ok(cover_url) = Url::parse(raw_cover.trim()) else {
            continue;
        };
        if mangacore::http::validate_public_http_url(&cover_url).is_err() {
            continue;
        }
        let referer_str = cover_referer_for(&row.url, cover_url.as_str());
        let referer = Url::parse(&referer_str).unwrap_or_else(|_| cover_url.clone());
        targets.push(CoverHydrationTarget {
            cover_url,
            referer,
            site: Some(row.site),
            manga_id: Some(manga_id),
        });
    }
    state
        .images
        .spawn_batch_cover_hydration(state.registry.http().clone(), targets);
}

/// Core transactional reconciliation routine (also directly exercised by unit tests with in-memory `SQLite`).
///
/// # Errors
///
/// Returns [`AppError::Db`] on `SQLite` failure.
pub(crate) async fn apply_backup_to_db(
    db: &crate::repo::Database,
    backup: &Backup,
    request: &TachibkImportRequest,
) -> Result<TachibkImportReport, AppError> {
    let cat_index = CategoryIndex::from_backup(&backup.backup_categories);
    let candidates = db.list_manga_match_candidates().await?;
    let matcher = LibraryMatcher::from_candidates(&candidates);
    let selected_set: HashSet<usize> = request.selected_indices.iter().copied().collect();
    let now = now_unix_secs();

    let mut report = TachibkImportReport::default();
    let mut tx = db.pool().begin().await?;
    let mut seen_catalog_urls: HashSet<String> = HashSet::new();

    for (index, item) in backup.backup_manga.iter().enumerate() {
        if !selected_set.contains(&index) {
            continue;
        }
        let title = item.display_title().trim();
        if title.is_empty() {
            report.skipped_count += 1;
            continue;
        }

        let categories = if request.import_categories {
            cat_index.resolve_for_manga(&item.categories)
        } else {
            Vec::new()
        };

        if let Some(hit) = matcher.find_match(item) {
            merge_into_existing_entry(&mut tx, hit, item, &categories, request, now, &mut report)
                .await?;
            if hit.match_kind == TachibkMatchKind::LocalLibrary {
                report.linked_local_count += 1;
            } else {
                report.updated_existing_count += 1;
            }
            if !report.imported_manga_ids.contains(&hit.id) {
                report.imported_manga_ids.push(hit.id);
            }
        } else if request.import_metadata_only {
            let new_id = insert_catalog_entry(
                &mut tx,
                item,
                &categories,
                request,
                now,
                &mut seen_catalog_urls,
                &mut report,
            )
            .await?;
            report.created_catalog_count += 1;
            report.imported_manga_ids.push(new_id);
        } else {
            report.skipped_count += 1;
        }
    }

    tx.commit().await?;
    Ok(report)
}

#[derive(Debug, sqlx::FromRow)]
struct ExistingMetaRow {
    cover_url: Option<String>,
    description: Option<String>,
    status: Option<String>,
    authors_json: Option<String>,
    artists_json: Option<String>,
    genres_json: Option<String>,
    tags_json: Option<String>,
    alt_titles_json: Option<String>,
}

#[derive(Debug, sqlx::FromRow)]
struct ExistingChapterRow {
    url: String,
    name: String,
    number: Option<f64>,
    idx: i64,
    read: i64,
}

#[derive(Debug, sqlx::FromRow)]
struct ExistingHistoryRow {
    chapter_url: String,
    chapter_name: String,
    page_index: i64,
    read_at: i64,
}

#[expect(
    clippy::too_many_lines,
    reason = "merges metadata, categories, bookmarks, chapter read flags, and history in one transaction"
)]
async fn merge_into_existing_entry(
    tx: &mut sqlx::Transaction<'_, Sqlite>,
    target: &MatchTarget,
    item: &BackupManga,
    categories: &[String],
    request: &TachibkImportRequest,
    now: i64,
    report: &mut TachibkImportReport,
) -> Result<(), AppError> {
    let existing_meta: Option<ExistingMetaRow> = sqlx::query_as(
        "SELECT cover_url, description, status, authors_json, artists_json,
                genres_json, tags_json, alt_titles_json
         FROM manga WHERE id = ?",
    )
    .bind(target.id)
    .fetch_optional(&mut **tx)
    .await?;

    let Some(meta) = existing_meta else {
        return Ok(());
    };

    let incoming_cover = sanitize_public_cover_url(item.thumbnail_url.as_deref());
    let next_cover = meta.cover_url.or(incoming_cover);
    let incoming_desc = item
        .description
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned);
    let next_desc = meta
        .description
        .filter(|s| !s.trim().is_empty())
        .or(incoming_desc);
    let next_status = meta
        .status
        .filter(|s| !s.trim().is_empty())
        .or_else(|| map_mihon_status(item.status).map(str::to_owned));

    let incoming_authors = split_people(item.author.as_deref());
    let incoming_artists = split_people(item.artist.as_deref());
    let mut incoming_genres = Vec::with_capacity(item.genre.len());
    for g in &item.genre {
        push_csv_unique(&mut incoming_genres, g);
    }
    let incoming_alts = collect_alt_titles(item);

    let next_authors = merge_json_string_vecs(meta.authors_json.as_deref(), &incoming_authors);
    let next_artists = merge_json_string_vecs(meta.artists_json.as_deref(), &incoming_artists);
    let next_genres = merge_json_string_vecs(meta.genres_json.as_deref(), &incoming_genres);
    let next_tags = if categories.is_empty() {
        meta.tags_json
    } else {
        merge_json_string_vecs(meta.tags_json.as_deref(), categories)
    };
    let next_alts = merge_json_string_vecs(meta.alt_titles_json.as_deref(), &incoming_alts);

    sqlx::query(
        "UPDATE manga
         SET cover_url = ?,
             description = ?,
             status = ?,
             authors_json = ?,
             artists_json = ?,
             genres_json = ?,
             tags_json = ?,
             alt_titles_json = ?,
             updated_at = ?
         WHERE id = ?",
    )
    .bind(next_cover.as_deref())
    .bind(next_desc.as_deref())
    .bind(next_status.as_deref())
    .bind(next_authors.as_deref())
    .bind(next_artists.as_deref())
    .bind(next_genres.as_deref())
    .bind(next_tags.as_deref())
    .bind(next_alts.as_deref())
    .bind(now)
    .bind(target.id)
    .execute(&mut **tx)
    .await?;

    let has_bookmarked_chapter = item.chapters.iter().any(|c| c.bookmark);
    if request.import_bookmarks && (item.is_favorite() || has_bookmarked_chapter) {
        let added_ts = normalize_unix_secs(item.date_added).unwrap_or(now);
        let res = sqlx::query(
            "INSERT INTO bookmark (manga_id, created_at) VALUES (?, ?)
             ON CONFLICT(manga_id) DO NOTHING",
        )
        .bind(target.id)
        .bind(added_ts)
        .execute(&mut **tx)
        .await?;
        if res.rows_affected() > 0 {
            report.synced_bookmarks += 1;
        }
    }

    if !request.sync_reading_progress {
        return Ok(());
    }

    let existing_chapters: Vec<ExistingChapterRow> = sqlx::query_as(
        "SELECT url, name, number, idx, read FROM chapter WHERE manga_id = ? ORDER BY idx ASC",
    )
    .bind(target.id)
    .fetch_all(&mut **tx)
    .await?;

    if existing_chapters.is_empty() {
        return Ok(());
    }

    // Match read chapters from backup onto existing local/library chapters (monotonic union).
    let mut newly_marked_urls: HashSet<String> = HashSet::new();
    let mut read_chapter_matched_any = false;
    for bch in &item.chapters {
        if !bch.read {
            continue;
        }
        if let Some(matched) = find_matching_existing_chapter(&existing_chapters, bch) {
            read_chapter_matched_any = true;
            if matched.read == 0 && newly_marked_urls.insert(matched.url.clone()) {
                sqlx::query("UPDATE chapter SET read = 1 WHERE manga_id = ? AND url = ?")
                    .bind(target.id)
                    .bind(&matched.url)
                    .execute(&mut **tx)
                    .await?;
                report.synced_chapters_read += 1;
            }
        }
    }

    // Fallback: if backup had tracker progress (`last_chapter_read`) but no chapter entries,
    // mark existing chapters up through `last_chapter_read` as read.
    if !read_chapter_matched_any && item.chapters.is_empty() {
        let max_tracked = item
            .tracking
            .iter()
            .filter(|tr| tr.last_chapter_read.is_finite() && tr.last_chapter_read > 0.0)
            .map(|tr| f64::from(tr.last_chapter_read))
            .fold(0.0_f64, f64::max);
        if max_tracked > 0.0 {
            for ech in &existing_chapters {
                if let Some(n) = ech.number
                    && n > 0.0
                    && n <= max_tracked + 0.001
                    && ech.read == 0
                    && newly_marked_urls.insert(ech.url.clone())
                {
                    sqlx::query("UPDATE chapter SET read = 1 WHERE manga_id = ? AND url = ?")
                        .bind(target.id)
                        .bind(&ech.url)
                        .execute(&mut **tx)
                        .await?;
                    report.synced_chapters_read += 1;
                }
            }
        }
    }

    // Clear `has_new` if the latest chapter is now marked read.
    sqlx::query(
        "UPDATE manga SET has_new = 0
         WHERE id = ? AND EXISTS (
             SELECT 1 FROM chapter c
             WHERE c.manga_id = manga.id
               AND RTRIM(c.url, '/') = RTRIM(manga.latest_chapter_url, '/')
               AND c.read = 1
         )",
    )
    .bind(target.id)
    .execute(&mut **tx)
    .await?;

    // Reconcile reading history pointer (`chapter_url`, `page_index`, `read_at`).
    if let Some((matched_ch, page_index, read_at)) =
        pick_best_matched_history(item, &existing_chapters, now)
    {
        merge_history_pointer(
            tx,
            target.id,
            &existing_chapters,
            matched_ch,
            page_index,
            read_at,
            report,
        )
        .await?;
    }

    Ok(())
}

struct PreparedCatalogChapter<'a> {
    url: String,
    name: String,
    number: Option<f64>,
    idx: i64,
    read: i64,
    published_at: Option<String>,
    source_id: Option<&'a str>,
}

#[expect(
    clippy::too_many_lines,
    reason = "inserts metadata-only catalog manga, batched chapters, bookmark, and optional reading history"
)]
async fn insert_catalog_entry(
    tx: &mut sqlx::Transaction<'_, Sqlite>,
    item: &BackupManga,
    categories: &[String],
    request: &TachibkImportRequest,
    now: i64,
    seen_catalog_urls: &mut HashSet<String>,
    report: &mut TachibkImportReport,
) -> Result<i64, AppError> {
    let title = item.display_title().trim();
    let slug = slugify_series(title);
    let hash8 = short_entry_hash(item.source, &item.url, title);
    let mut base_key = format!("{slug}-{hash8}");
    let mut series_url = format!("{TACHIBK_URL_PREFIX}series/{base_key}");
    let mut suffix = 2_u32;
    while !seen_catalog_urls.insert(series_url.clone()) {
        base_key = format!("{slug}-{hash8}-{suffix}");
        series_url = format!("{TACHIBK_URL_PREFIX}series/{base_key}");
        suffix = suffix.saturating_add(1);
    }

    let source_id = format!("tachibk:{base_key}");
    let cover_url = sanitize_public_cover_url(item.thumbnail_url.as_deref());
    let description = item
        .description
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let status = map_mihon_status(item.status);
    let authors_json = merge_json_string_vecs(None, &split_people(item.author.as_deref()));
    let artists_json = merge_json_string_vecs(None, &split_people(item.artist.as_deref()));
    let mut genres_vec = Vec::with_capacity(item.genre.len());
    for g in &item.genre {
        push_csv_unique(&mut genres_vec, g);
    }
    let genres_json = merge_json_string_vecs(None, &genres_vec);
    let tags_json = merge_json_string_vecs(None, categories);
    let alt_titles_json = merge_json_string_vecs(None, &collect_alt_titles(item));
    let original_title = item
        .custom_title
        .as_deref()
        .map(str::trim)
        .filter(|ct| !ct.is_empty() && !ct.eq_ignore_ascii_case(item.title.trim()))
        .map(|_| item.title.trim());

    let sorted_chaps = sorted_backup_chapters(&item.chapters);
    let latest_ch_idx = sorted_chaps.len().checked_sub(1);
    let latest_ch_url =
        latest_ch_idx.map(|i| format!("{TACHIBK_URL_PREFIX}chapter/{base_key}/{i}"));
    let latest_ch_name = latest_ch_idx.map(|i| {
        let name = sorted_chaps[i].name.trim();
        if name.is_empty() {
            format!("Chapter {}", i + 1)
        } else {
            name.to_owned()
        }
    });
    let created_at = normalize_unix_secs(item.date_added).unwrap_or(now);
    let detected_layout = crate::services::layout_detect::classify_from_metadata(
        &genres_vec,
        categories,
        TACHIBK_SITE_KEY,
    )
    .and_then(crate::services::layout_detect::DetectedLayout::as_db_str);
    let detected_webtoon = i64::from(detected_layout == Some("webtoon"));

    let res = sqlx::query(
        "INSERT INTO manga
            (url, site, title, cover_url, latest_chapter_url, latest_chapter_name, has_new,
             detected_webtoon, detected_layout,
             source_id, canonical_url, alt_titles_json, original_title, authors_json,
             artists_json, description, genres_json, tags_json, status, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, 0, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&series_url)
    .bind(TACHIBK_SITE_KEY)
    .bind(title)
    .bind(cover_url.as_deref())
    .bind(latest_ch_url.as_deref())
    .bind(latest_ch_name.as_deref())
    .bind(detected_webtoon)
    .bind(detected_layout)
    .bind(&source_id)
    .bind(&series_url)
    .bind(alt_titles_json.as_deref())
    .bind(original_title)
    .bind(authors_json.as_deref())
    .bind(artists_json.as_deref())
    .bind(description)
    .bind(genres_json.as_deref())
    .bind(tags_json.as_deref())
    .bind(status)
    .bind(created_at)
    .bind(now)
    .execute(&mut **tx)
    .await?;

    let manga_id = res.last_insert_rowid();

    // Prepare chapter rows (chunked multi-row inserts for speed on 1,000+ chapter series).
    let mut prepared_chaps: Vec<PreparedCatalogChapter<'_>> =
        Vec::with_capacity(sorted_chaps.len());
    let mut orig_url_to_synth: HashMap<&str, (String, String, i64)> =
        HashMap::with_capacity(sorted_chaps.len());

    for (idx, bch) in sorted_chaps.iter().enumerate() {
        let ch_url = format!("{TACHIBK_URL_PREFIX}chapter/{base_key}/{idx}");
        let ch_name = if bch.name.trim().is_empty() {
            format!("Chapter {}", idx + 1)
        } else {
            bch.name.trim().to_owned()
        };
        let ch_num = parsed_backup_chapter_number(bch);
        let ch_read = i64::from(request.sync_reading_progress && bch.read);
        if ch_read == 1 {
            report.synced_chapters_read += 1;
        }
        let published_at = normalize_unix_secs(bch.date_upload).and_then(format_unix_date);
        let ch_source_id = bch.bookmark.then_some(TACHIBK_BOOKMARKED_CHAPTER_TAG);
        let idx_i64 = i64::try_from(idx).unwrap_or(i64::MAX);

        orig_url_to_synth.insert(
            bch.url.trim(),
            (ch_url.clone(), ch_name.clone(), bch.last_page_read.max(0)),
        );
        prepared_chaps.push(PreparedCatalogChapter {
            url: ch_url,
            name: ch_name,
            number: ch_num,
            idx: idx_i64,
            read: ch_read,
            published_at,
            source_id: ch_source_id,
        });
    }

    for chunk in prepared_chaps.chunks(100) {
        let mut qb: QueryBuilder<'_, Sqlite> = QueryBuilder::new(
            "INSERT INTO chapter \
             (manga_id, url, name, number, idx, read, published_at, source_id) ",
        );
        qb.push_values(chunk, |mut b, ch| {
            b.push_bind(manga_id)
                .push_bind(&ch.url)
                .push_bind(&ch.name)
                .push_bind(ch.number)
                .push_bind(ch.idx)
                .push_bind(ch.read)
                .push_bind(ch.published_at.as_deref())
                .push_bind(ch.source_id);
        });
        qb.build().execute(&mut **tx).await?;
    }

    // Resolve reading history for this newly created catalog entry.
    let mut wrote_history = false;
    if request.sync_reading_progress
        && let Some((ch_url, ch_name, page_idx, read_at)) =
            resolve_catalog_history(item, &sorted_chaps, &orig_url_to_synth, &base_key, now)
    {
        sqlx::query(
            "INSERT INTO history (manga_id, chapter_url, chapter_name, scroll_ratio, page_index, read_at)
             VALUES (?, ?, ?, 0.0, ?, ?)
             ON CONFLICT(manga_id) DO UPDATE SET
                chapter_url = excluded.chapter_url,
                chapter_name = excluded.chapter_name,
                page_index = excluded.page_index,
                read_at = excluded.read_at",
        )
        .bind(manga_id)
        .bind(&ch_url)
        .bind(&ch_name)
        .bind(page_idx)
        .bind(read_at)
        .execute(&mut **tx)
        .await?;
        report.synced_history += 1;
        wrote_history = true;
    }

    // Bookmark the catalog entry if favorites are imported OR if no history row was written
    // (guaranteeing every explicitly imported entry appears in the Library and survives pruning).
    let should_bookmark = (request.import_bookmarks && item.is_favorite()) || !wrote_history;
    if should_bookmark {
        let bres = sqlx::query(
            "INSERT INTO bookmark (manga_id, created_at) VALUES (?, ?)
             ON CONFLICT(manga_id) DO NOTHING",
        )
        .bind(manga_id)
        .bind(created_at)
        .execute(&mut **tx)
        .await?;
        if bres.rows_affected() > 0 {
            report.synced_bookmarks += 1;
        }
    }

    Ok(manga_id)
}

fn collect_alt_titles(item: &BackupManga) -> Vec<String> {
    let primary = item.display_title().trim();
    let mut out = Vec::new();
    let orig = item.title.trim();
    if !orig.is_empty() && !orig.eq_ignore_ascii_case(primary) {
        out.push(orig.to_owned());
    }
    for tr in &item.tracking {
        let tt = tr.title.trim();
        if !tt.is_empty()
            && !tt.eq_ignore_ascii_case(primary)
            && !out.iter().any(|e| e.eq_ignore_ascii_case(tt))
        {
            out.push(tt.to_owned());
        }
    }
    out
}

fn find_matching_existing_chapter<'a>(
    existing: &'a [ExistingChapterRow],
    backup_ch: &BackupChapter,
) -> Option<&'a ExistingChapterRow> {
    let b_url = backup_ch.url.trim().trim_end_matches('/');
    if !b_url.is_empty()
        && let Some(hit) = existing
            .iter()
            .find(|e| e.url.trim().trim_end_matches('/') == b_url)
    {
        return Some(hit);
    }

    if let Some(b_num) = parsed_backup_chapter_number(backup_ch)
        && let Some(hit) = existing
            .iter()
            .find(|e| e.number.is_some_and(|en| (en - b_num).abs() < 0.001))
    {
        return Some(hit);
    }

    let b_norm = normalize_match_title(&backup_ch.name);
    if !b_norm.is_empty()
        && let Some(hit) = existing
            .iter()
            .find(|e| normalize_match_title(&e.name) == b_norm)
    {
        return Some(hit);
    }

    None
}

fn pick_best_matched_history<'a>(
    item: &BackupManga,
    existing: &'a [ExistingChapterRow],
    now: i64,
) -> Option<(&'a ExistingChapterRow, i64, i64)> {
    // 1. Prefer the most recent `BackupHistory` entry whose chapter matches an existing row.
    let mut hist_sorted: Vec<&BackupHistory> = item.history.iter().collect();
    hist_sorted.sort_by_key(|a| std::cmp::Reverse(a.last_read));

    let chap_by_url: HashMap<&str, &BackupChapter> =
        item.chapters.iter().map(|c| (c.url.trim(), c)).collect();

    for h in hist_sorted {
        let ts = normalize_unix_secs(h.last_read).unwrap_or(now);
        if let Some(bch) = chap_by_url.get(h.url.trim())
            && let Some(matched) = find_matching_existing_chapter(existing, bch)
        {
            return Some((matched, bch.last_page_read.max(0), ts));
        }
        let h_norm = h.url.trim().trim_end_matches('/');
        if !h_norm.is_empty()
            && let Some(matched) = existing
                .iter()
                .find(|e| e.url.trim().trim_end_matches('/') == h_norm)
        {
            return Some((matched, 0, ts));
        }
    }

    // 2. Fallback to the furthest read/in-progress chapter from the backup that matches locally.
    let sorted = sorted_backup_chapters(&item.chapters);
    for bch in sorted.into_iter().rev() {
        if (bch.read || bch.last_page_read > 0)
            && let Some(matched) = find_matching_existing_chapter(existing, bch)
        {
            let ts = normalize_unix_secs(bch.last_modified_at)
                .or_else(|| normalize_unix_secs(item.last_modified_at))
                .unwrap_or(now);
            return Some((matched, bch.last_page_read.max(0), ts));
        }
    }

    None
}

async fn merge_history_pointer(
    tx: &mut sqlx::Transaction<'_, Sqlite>,
    manga_id: i64,
    existing_chapters: &[ExistingChapterRow],
    matched_ch: &ExistingChapterRow,
    incoming_page: i64,
    incoming_read_at: i64,
    report: &mut TachibkImportReport,
) -> Result<(), AppError> {
    let local_hist: Option<ExistingHistoryRow> = sqlx::query_as(
        "SELECT chapter_url, chapter_name, page_index, read_at FROM history WHERE manga_id = ?",
    )
    .bind(manga_id)
    .fetch_optional(&mut **tx)
    .await?;

    let should_write = match local_hist {
        None => true,
        Some(local) => {
            if local.chapter_url.trim_end_matches('/') == matched_ch.url.trim_end_matches('/') {
                incoming_page > local.page_index || incoming_read_at > local.read_at
            } else {
                let local_rank = chapter_rank(
                    existing_chapters,
                    &local.chapter_url,
                    Some(&local.chapter_name),
                );
                let incoming_rank = (
                    matched_ch
                        .number
                        .unwrap_or_else(|| f64::from(i32::try_from(matched_ch.idx).unwrap_or(0))),
                    matched_ch.idx,
                );
                match local_rank {
                    Some((loc_num, loc_idx)) => {
                        if (incoming_rank.0 - loc_num).abs() > f64::EPSILON {
                            incoming_rank.0 > loc_num
                        } else if incoming_rank.1 != loc_idx {
                            incoming_rank.1 > loc_idx
                        } else {
                            incoming_read_at > local.read_at
                        }
                    }
                    None => true,
                }
            }
        }
    };

    if should_write {
        sqlx::query(
            "INSERT INTO history (manga_id, chapter_url, chapter_name, scroll_ratio, page_index, read_at)
             VALUES (?, ?, ?, 0.0, ?, ?)
             ON CONFLICT(manga_id) DO UPDATE SET
                chapter_url = excluded.chapter_url,
                chapter_name = excluded.chapter_name,
                page_index = MAX(history.page_index, excluded.page_index),
                read_at = MAX(history.read_at, excluded.read_at)",
        )
        .bind(manga_id)
        .bind(&matched_ch.url)
        .bind(&matched_ch.name)
        .bind(incoming_page.max(0))
        .bind(incoming_read_at)
        .execute(&mut **tx)
        .await?;
        report.synced_history += 1;
    }

    Ok(())
}

fn chapter_rank(
    chapters: &[ExistingChapterRow],
    chapter_url: &str,
    chapter_name: Option<&str>,
) -> Option<(f64, i64)> {
    let target = chapter_url.trim().trim_end_matches('/');
    if let Some(ch) = chapters
        .iter()
        .find(|c| c.url.trim().trim_end_matches('/') == target)
    {
        let num = ch
            .number
            .unwrap_or_else(|| f64::from(i32::try_from(ch.idx).unwrap_or(0)));
        return Some((num, ch.idx));
    }
    chapter_name
        .and_then(extract_chapter_number)
        .map(|n| (f64::from(n), 0))
}

fn resolve_catalog_history(
    item: &BackupManga,
    sorted_chaps: &[&BackupChapter],
    orig_url_to_synth: &HashMap<&str, (String, String, i64)>,
    base_key: &str,
    now: i64,
) -> Option<(String, String, i64, i64)> {
    let mut hist_sorted: Vec<&BackupHistory> = item.history.iter().collect();
    hist_sorted.sort_by_key(|a| std::cmp::Reverse(a.last_read));

    for h in hist_sorted {
        let ts = normalize_unix_secs(h.last_read).unwrap_or(now);
        if let Some((synth_url, name, page_idx)) = orig_url_to_synth.get(h.url.trim()) {
            return Some((synth_url.clone(), name.clone(), *page_idx, ts));
        }
    }

    for (idx, bch) in sorted_chaps.iter().enumerate().rev() {
        if bch.read || bch.last_page_read > 0 {
            let synth_url = format!("{TACHIBK_URL_PREFIX}chapter/{base_key}/{idx}");
            let name = if bch.name.trim().is_empty() {
                format!("Chapter {}", idx + 1)
            } else {
                bch.name.trim().to_owned()
            };
            let ts = normalize_unix_secs(bch.last_modified_at)
                .or_else(|| normalize_unix_secs(item.last_modified_at))
                .or_else(|| normalize_unix_secs(item.date_added))
                .unwrap_or(now);
            return Some((synth_url, name, bch.last_page_read.max(0), ts));
        }
    }

    None
}

/// Builds a map of `(chapter_number, normalized_name) -> read_flag` from existing `tachibk.local`
/// placeholder chapters so attaching local archives later preserves read progress seamlessly.
#[must_use]
pub(crate) fn extract_tachibk_read_lookup(
    chapters: &[crate::repo::manga::ChapterRow],
) -> (BTreeMap<i64, bool>, HashSet<String>) {
    let mut by_milli_num: BTreeMap<i64, bool> = BTreeMap::new();
    let mut by_norm_name: HashSet<String> = HashSet::new();
    for ch in chapters {
        if ch.read == 0 || !is_tachibk_url(&ch.url) {
            continue;
        }
        if let Some(num) = ch.number
            && num.is_finite()
        {
            #[expect(
                clippy::as_conversions,
                clippy::cast_possible_truncation,
                reason = "chapter numbers quantized to 0.001 precision for deterministic BTreeMap key"
            )]
            let milli = (num * 1000.0).round() as i64;
            by_milli_num.insert(milli, true);
        }
        let norm = normalize_match_title(&ch.name);
        if !norm.is_empty() {
            by_norm_name.insert(norm);
        }
    }
    (by_milli_num, by_norm_name)
}

/// Checks whether a newly attached local chapter matches a previously read `tachibk.local` entry.
#[must_use]
pub(crate) fn matches_tachibk_read_lookup(
    number: Option<f32>,
    name: &str,
    by_milli_num: &BTreeMap<i64, bool>,
    by_norm_name: &HashSet<String>,
) -> bool {
    if let Some(num) = number
        && num.is_finite()
    {
        #[expect(
            clippy::as_conversions,
            clippy::cast_possible_truncation,
            reason = "chapter numbers quantized to 0.001 precision for lookup matching"
        )]
        let milli = (f64::from(num) * 1000.0).round() as i64;
        if by_milli_num.contains_key(&milli) {
            return true;
        }
    }
    let norm = normalize_match_title(name);
    !norm.is_empty() && by_norm_name.contains(&norm)
}

#[cfg(test)]
mod tests {
    use std::io::Write as _;

    use flate2::Compression;
    use flate2::write::GzEncoder;
    use mangacore::{ChapterRef, Manga, SiteId};

    use super::*;
    use crate::repo::Database;
    use crate::repo::bookmark::BookmarkSort;

    fn gzip_bytes(raw: &[u8]) -> Vec<u8> {
        let mut enc = GzEncoder::new(Vec::new(), Compression::default());
        enc.write_all(raw).expect("gzip write succeeds");
        enc.finish().expect("gzip finish succeeds")
    }

    #[expect(
        clippy::too_many_lines,
        reason = "synthetic Mihon/Tachiyomi protobuf test fixture covering categories, sources, chapters, history, and tracking"
    )]
    fn build_synthetic_backup() -> Backup {
        Backup {
            backup_categories: vec![
                BackupCategory {
                    name: "Reading".to_owned(),
                    order: 0,
                    id: Some(10),
                    flags: 0,
                },
                BackupCategory {
                    name: "Favorites".to_owned(),
                    order: 1,
                    id: Some(20),
                    flags: 0,
                },
            ],
            backup_sources: vec![
                BackupSource {
                    name: "Example Source".to_owned(),
                    source_id: 2_499_283_573_021_220_255_i64,
                },
                BackupSource {
                    name: "Local source".to_owned(),
                    source_id: 0,
                },
            ],
            backup_manga: vec![
                // Matches an existing local comic via normalized title (punctuation & underscore difference)
                BackupManga {
                    source: 2_499_283_573_021_220_255_i64,
                    url: "/manga/shadow-blade".to_owned(),
                    title: "Shadow_Blade!".to_owned(),
                    artist: Some("Studio Ink".to_owned()),
                    author: Some("A. Writer".to_owned()),
                    description: Some("Synthetic test synopsis for local matching.".to_owned()),
                    genre: vec!["Action".to_owned(), "Fantasy".to_owned()],
                    status: 2, // Completed
                    thumbnail_url: Some("https://cdn.example.com/shadow-cover.jpg".to_owned()),
                    date_added: 1_700_000_000_000,
                    viewer: 0,
                    chapters: vec![
                        BackupChapter {
                            url: "/chapter/sb-1".to_owned(),
                            name: "Ch. 1 - Prologue".to_owned(),
                            scanlator: Some("ScanGroup".to_owned()),
                            read: true,
                            bookmark: false,
                            last_page_read: 24,
                            date_fetch: 1_700_000_001_000,
                            date_upload: 1_699_000_000_000,
                            chapter_number: 1.0,
                            source_order: 2,
                            last_modified_at: 1_700_001_000_000,
                            version: 1,
                        },
                        BackupChapter {
                            url: "/chapter/sb-2".to_owned(),
                            name: "Ch. 2 - Awakening".to_owned(),
                            scanlator: None,
                            read: true,
                            bookmark: true,
                            last_page_read: 18,
                            date_fetch: 1_700_000_002_000,
                            date_upload: 1_699_500_000_000,
                            chapter_number: 2.0,
                            source_order: 1,
                            last_modified_at: 1_700_002_000_000,
                            version: 1,
                        },
                        BackupChapter {
                            url: "/chapter/sb-3".to_owned(),
                            name: "Ch. 3".to_owned(),
                            scanlator: None,
                            read: false,
                            bookmark: false,
                            last_page_read: 5,
                            date_fetch: 1_700_000_003_000,
                            date_upload: 1_699_900_000_000,
                            chapter_number: 3.0,
                            source_order: 0,
                            last_modified_at: 1_700_003_000_000,
                            version: 1,
                        },
                    ],
                    categories: vec![0, 1],
                    tracking: vec![BackupTracking {
                        sync_id: 2,
                        library_id: 1,
                        media_id_int: 105_398,
                        tracking_url: "https://tracker.example.com/manga/105398".to_owned(),
                        title: "Kage no Yaiba".to_owned(),
                        last_chapter_read: 2.0,
                        total_chapters: 179,
                        score: 9.0,
                        status: 2,
                        started_reading_date: 1_700_000_000_000,
                        finished_reading_date: 0,
                        media_id: 105_398,
                    }],
                    favorite: None, // Omitted on wire -> defaults to true in Mihon!
                    chapter_flags: 0,
                    viewer_flags: None,
                    history: vec![BackupHistory {
                        url: "/chapter/sb-3".to_owned(),
                        last_read: 1_710_000_000_000,
                        read_duration: 42_000,
                    }],
                    update_strategy: 0,
                    last_modified_at: 1_710_000_000_000,
                    favorite_modified_at: None,
                    excluded_scanlators: Vec::new(),
                    version: 1,
                    notes: String::new(),
                    initialized: true,
                    custom_title: None,
                },
                // Unmatched title -> imported as metadata-only catalog entry
                BackupManga {
                    source: 2_499_283_573_021_220_255_i64,
                    url: "/manga/starlight-horizon".to_owned(),
                    title: "Hoshizora no Chihei".to_owned(),
                    artist: Some("B. Artist".to_owned()),
                    author: Some("C. Author".to_owned()),
                    description: Some("Synthetic test synopsis for catalog-only entry.".to_owned()),
                    genre: vec!["Adventure".to_owned(), "Drama".to_owned()],
                    status: 1, // Ongoing
                    // SSRF loopback URL must be stripped cleanly during decode/import
                    thumbnail_url: Some("http://127.0.0.1:9999/secret.png".to_owned()),
                    date_added: 1_705_000_000_000,
                    viewer: 0,
                    chapters: vec![
                        BackupChapter {
                            url: "/chapter/sh-1".to_owned(),
                            name: "Chapter 1".to_owned(),
                            scanlator: None,
                            read: true,
                            bookmark: true,
                            last_page_read: 30,
                            date_fetch: 0,
                            date_upload: 1_600_000_000_000,
                            chapter_number: 1.0,
                            source_order: 1,
                            last_modified_at: 1_706_000_000_000,
                            version: 1,
                        },
                        BackupChapter {
                            url: "/chapter/sh-2".to_owned(),
                            name: "Chapter 2".to_owned(),
                            scanlator: None,
                            read: false,
                            bookmark: false,
                            last_page_read: 0,
                            date_fetch: 0,
                            date_upload: 1_600_500_000_000,
                            chapter_number: 2.0,
                            source_order: 0,
                            last_modified_at: 0,
                            version: 1,
                        },
                    ],
                    categories: vec![1],
                    tracking: Vec::new(),
                    favorite: Some(true),
                    chapter_flags: 0,
                    viewer_flags: None,
                    history: vec![BackupHistory {
                        url: "/chapter/sh-1".to_owned(),
                        last_read: 1_709_000_000_000,
                        read_duration: 15_000,
                    }],
                    update_strategy: 0,
                    last_modified_at: 1_709_000_000_000,
                    favorite_modified_at: None,
                    excluded_scanlators: Vec::new(),
                    version: 1,
                    notes: String::new(),
                    initialized: true,
                    custom_title: Some("Starlight Horizon: Extended".to_owned()),
                },
            ],
        }
    }

    #[test]
    fn decodes_gzip_tachibk_payload_and_rejects_tar_gz_and_bombs() {
        let backup = build_synthetic_backup();
        let proto_bytes = backup.encode_to_vec();
        let gz_bytes = gzip_bytes(&proto_bytes);

        let decoded = decode_tachibk_bytes(&gz_bytes).expect("synthetic tachibk must decode");
        assert_eq!(
            decoded.backup_manga.len(),
            2,
            "decoded backup must contain both synthetic manga"
        );
        assert_eq!(
            decoded.backup_categories.len(),
            2,
            "decoded backup must contain both categories"
        );
        assert!(
            decoded.backup_manga[0].is_favorite(),
            "omitted favorite field must default to true per Mihon spec"
        );
        assert_eq!(
            decoded.backup_manga[1].display_title(),
            "Starlight Horizon: Extended",
            "custom_title must take precedence over title"
        );

        // Verify `.tar.gz` / gzipped `.cbt` is NOT misclassified as a `.tachibk` backup!
        let mut tar_buf = Vec::new();
        {
            let mut builder = tar::Builder::new(&mut tar_buf);
            let payload = b"fake image bytes";
            let mut header = tar::Header::new_gnu();
            header.set_size(u64::try_from(payload.len()).expect("len fits u64"));
            header.set_cksum();
            builder
                .append_data(&mut header, "page_001.png", &payload[..])
                .expect("tar append succeeds");
            builder.finish().expect("tar finish succeeds");
        }
        let tar_gz = gzip_bytes(&tar_buf);
        assert!(
            matches!(
                decode_tachibk_bytes(&tar_gz),
                Err(AppError::UnsupportedFormat { .. })
            ),
            "gzipped TAR archive must be rejected as UnsupportedFormat, not parsed as tachibk"
        );

        // Verify on-disk magic-byte detection distinguishes `.tachibk` vs `.tar.gz` even when renamed!
        let dir =
            std::env::temp_dir().join(format!("rtmaga_tachibk_detect_{}", rand::random::<u64>()));
        std::fs::create_dir_all(&dir).expect("temp dir created");

        let renamed_backup = dir.join("backup_without_ext.bin");
        std::fs::write(&renamed_backup, &gz_bytes).expect("write renamed tachibk");
        assert_eq!(
            detect_format_magic(&renamed_backup).expect("tachibk magic detection succeeds"),
            DetectedFileKind::Tachibk,
            "GZIP protobuf backup must be detected as Tachibk regardless of extension"
        );

        let renamed_targz = dir.join("archive.tachibk");
        std::fs::write(&renamed_targz, &tar_gz).expect("write disguised tar.gz");
        assert!(
            !matches!(
                detect_format_magic(&renamed_targz),
                Ok(DetectedFileKind::Tachibk)
            ),
            "disguised .tar.gz named .tachibk must NOT be classified as Tachibk"
        );

        // Verify bounded GZIP decompression guard catches oversized streams.
        let zeros = vec![0_u8; 2048];
        let small_gz = gzip_bytes(&zeros);
        assert!(
            matches!(
                decompress_gzip_bounded(small_gz.as_slice(), 1024),
                Err(AppError::CorruptedData { .. })
            ),
            "decompression exceeding byte ceiling must abort with CorruptedData"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    #[expect(
        clippy::too_many_lines,
        reason = "end-to-end SQLite reconciliation test verifying preview, local linking, catalog creation, bookmarks, and history"
    )]
    async fn preview_and_import_reconcile_local_matches_and_create_catalog_entries() {
        let db = Database::open_in_memory()
            .await
            .expect("in-memory sqlite opens");

        // Seed an existing local comic entry ("Shadow Blade") with 3 unread chapters.
        let local_manga = Manga::minimal(
            Url::parse("https://comic.local/series/shadow-blade-abcd").expect("valid url"),
            SiteId::known("local:comic"),
            "Shadow Blade".to_owned(),
            Some(Url::parse("https://comic.local/cover/shadow-blade-abcd.img").expect("url")),
            vec![
                ChapterRef::simple(
                    Url::parse("https://comic.local/chapter/shadow-blade-abcd/c1").expect("url"),
                    "Chapter 1".to_owned(),
                    Some(1.0),
                ),
                ChapterRef::simple(
                    Url::parse("https://comic.local/chapter/shadow-blade-abcd/c2").expect("url"),
                    "Chapter 2".to_owned(),
                    Some(2.0),
                ),
                ChapterRef::simple(
                    Url::parse("https://comic.local/chapter/shadow-blade-abcd/c3").expect("url"),
                    "Chapter 3".to_owned(),
                    Some(3.0),
                ),
            ],
        );
        let local_id = db
            .upsert_manga(&local_manga)
            .await
            .expect("seed local manga");
        db.upsert_local_comic_chapter(
            local_id,
            "https://comic.local/chapter/shadow-blade-abcd/c1",
            "zip",
            "shadow/c1.cbz",
            "[]",
            1024,
        )
        .await
        .expect("seed local comic archive row");

        // Decode through full GZIP + protobuf round-trip.
        let backup = decode_tachibk_bytes(&gzip_bytes(&build_synthetic_backup().encode_to_vec()))
            .expect("round-trip decode succeeds");

        let candidates = db
            .list_manga_match_candidates()
            .await
            .expect("match candidates query succeeds");
        let preview = build_preview_with_candidates(
            &backup,
            &candidates,
            &[9_u8; 32],
            Some("test.tachibk".to_owned()),
            None,
            "test.tachibk".to_owned(),
            512,
        );

        assert_eq!(preview.total_manga, 2, "preview must list 2 manga");
        assert_eq!(
            preview.matched_local_count, 1,
            "Shadow_Blade! must match local Shadow Blade"
        );
        assert_eq!(
            preview.unmatched_count, 1,
            "Starlight Horizon must be flagged as unmatched metadata-only"
        );
        assert_eq!(
            preview.categories.len(),
            2,
            "both backup categories must appear in preview"
        );
        assert_eq!(
            preview.entries[0].matched_manga_id,
            Some(local_id),
            "matched_manga_id must point to local comic"
        );
        assert_eq!(
            preview.entries[0].match_kind,
            TachibkMatchKind::LocalLibrary,
            "matched entry must be classified as LocalLibrary"
        );
        assert_eq!(
            preview.entries[0].categories,
            vec!["Reading", "Favorites"],
            "category order IDs must resolve to category names"
        );
        assert!(
            preview.entries[1].cover_token.is_none(),
            "loopback SSRF cover URL must be stripped from preview"
        );

        // Execute transactional import of both entries.
        let req = TachibkImportRequest {
            source_path: None,
            upload_id: None,
            selected_indices: vec![0, 1],
            sync_reading_progress: true,
            import_metadata_only: true,
            import_categories: true,
            import_bookmarks: true,
        };
        let report = apply_backup_to_db(&db, &backup, &req)
            .await
            .expect("backup import succeeds");

        assert_eq!(report.linked_local_count, 1, "1 local match linked");
        assert_eq!(
            report.created_catalog_count, 1,
            "1 metadata-only catalog entry created"
        );
        assert_eq!(
            report.synced_chapters_read, 3,
            "2 local + 1 catalog chapters marked read"
        );
        assert_eq!(report.synced_bookmarks, 2, "both entries bookmarked");
        assert_eq!(report.synced_history, 2, "history synced for both entries");

        // Verify local comic preserved its local cover & site while merging metadata, categories, and read flags!
        let updated_local = db
            .get_manga_by_id(local_id)
            .await
            .expect("load updated local")
            .expect("local row exists");
        assert_eq!(
            updated_local.site, "local:comic",
            "local site must remain local:comic"
        );
        assert_eq!(
            updated_local.cover_url.as_deref(),
            Some("https://comic.local/cover/shadow-blade-abcd.img"),
            "existing local cover must not be overwritten"
        );
        assert_eq!(
            updated_local.status.as_deref(),
            Some("completed"),
            "status must be merged from backup"
        );
        let tags: Vec<String> = serde_json::from_str(
            updated_local
                .tags_json
                .as_deref()
                .expect("categories stored in tags_json"),
        )
        .expect("valid tags json");
        assert_eq!(
            tags,
            vec!["Reading", "Favorites"],
            "categories must be mapped into tags_json"
        );

        let local_chaps = db.list_chapters(local_id).await.expect("list local chaps");
        assert_eq!(local_chaps[0].read, 1, "local chapter 1 marked read");
        assert_eq!(local_chaps[1].read, 1, "local chapter 2 marked read");
        assert_eq!(local_chaps[2].read, 0, "local chapter 3 stays unread");

        let local_hist = db
            .get_history_for_manga(local_id)
            .await
            .expect("load local history")
            .expect("history created");
        assert_eq!(
            local_hist.chapter_url, "https://comic.local/chapter/shadow-blade-abcd/c3",
            "history must resume at matched Chapter 3"
        );
        assert_eq!(
            local_hist.page_index, 5,
            "lastPageRead synced to page_index"
        );
        assert_eq!(
            local_hist.read_at, 1_710_000_000,
            "millisecond timestamp converted to Unix seconds"
        );

        // Verify metadata-only catalog entry was created cleanly & excluded from remote updater!
        let catalog_id = report.imported_manga_ids[1];
        let catalog_row = db
            .get_manga_by_id(catalog_id)
            .await
            .expect("load catalog")
            .expect("catalog row exists");
        assert_eq!(
            catalog_row.site, TACHIBK_SITE_KEY,
            "metadata-only entry must use local:tachibk site"
        );
        assert_eq!(
            catalog_row.title, "Starlight Horizon: Extended",
            "catalog title must use custom_title"
        );
        assert!(
            catalog_row.cover_url.is_none(),
            "loopback cover URL must remain stripped in SQLite"
        );
        let catalog_chaps = db
            .list_chapters(catalog_id)
            .await
            .expect("list catalog chapters");
        assert_eq!(
            catalog_chaps.len(),
            2,
            "catalog entry stores 2 chapter stubs"
        );
        assert_eq!(catalog_chaps[0].read, 1, "catalog chapter 1 marked read");
        assert_eq!(
            catalog_chaps[0].source_id.as_deref(),
            Some(TACHIBK_BOOKMARKED_CHAPTER_TAG),
            "chapter bookmark marker preserved"
        );

        let bookmarks = db
            .list_bookmarks(BookmarkSort::Newest)
            .await
            .expect("list bookmarks");
        assert_eq!(
            bookmarks.len(),
            2,
            "both imported manga appear in Bookmarks"
        );

        let update_targets = db
            .list_targets_for_updates()
            .await
            .expect("list update targets");
        assert!(
            update_targets.is_empty(),
            "local:comic and local:tachibk entries must both be excluded from web update polling"
        );
    }

    #[tokio::test]
    async fn alias_matching_and_skip_metadata_only_option_work() {
        let db = Database::open_in_memory()
            .await
            .expect("in-memory sqlite opens");

        // Local comic stored under the synthetic romaji alias (matches via BackupTracking.title).
        let local_romaji = Manga::minimal(
            Url::parse("https://comic.local/series/kage-no-yaiba-1234").expect("valid url"),
            SiteId::known("local:comic"),
            "Kage no Yaiba".to_owned(),
            None,
            vec![ChapterRef::simple(
                Url::parse("https://comic.local/chapter/kage-no-yaiba-1234/c1").expect("valid url"),
                "Chapter 1".to_owned(),
                Some(1.0),
            )],
        );
        let local_id = db
            .upsert_manga(&local_romaji)
            .await
            .expect("seed romaji manga");
        db.upsert_local_comic_chapter(
            local_id,
            "https://comic.local/chapter/kage-no-yaiba-1234/c1",
            "zip",
            "kage/c1.cbz",
            "[]",
            512,
        )
        .await
        .expect("seed local archive");

        let backup = build_synthetic_backup();
        let req = TachibkImportRequest {
            source_path: None,
            upload_id: None,
            selected_indices: vec![0, 1],
            sync_reading_progress: true,
            import_metadata_only: false, // Unmatched entry must be skipped
            import_categories: true,
            import_bookmarks: false,
        };
        let report = apply_backup_to_db(&db, &backup, &req)
            .await
            .expect("import with metadata_only=false succeeds");

        assert_eq!(
            report.linked_local_count, 1,
            "tracker alias 'Kage no Yaiba' must match local comic"
        );
        assert_eq!(
            report.created_catalog_count, 0,
            "metadata-only entries must not be created when import_metadata_only is false"
        );
        assert_eq!(report.skipped_count, 1, "unmatched entry must be skipped");
        assert!(
            !db.is_bookmarked(local_id).await.expect("check bookmark"),
            "bookmark must not be created when import_bookmarks is false"
        );

        let chaps = db.list_chapters(local_id).await.expect("list chapters");
        assert_eq!(chaps[0].read, 1, "matched chapter 1 must be marked read");
    }
}
