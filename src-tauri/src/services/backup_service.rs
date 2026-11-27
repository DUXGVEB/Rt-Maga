//! Native `.rtmaga` binary backup export, 3-stage pre-flight preview, and atomic SQLite
//! Smart-Merge / Overwrite import service.
//!
//! All binary framing, LZ4 compression, CRC32 hashing, and filesystem I/O run off the async
//! runtime via `tokio::task::spawn_blocking`. Progress streams over Tauri `Channel` without
//! passing binary backup payloads through `WebView` Base64 JSON.

use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::BufWriter;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use sqlx::{Connection, FromRow};
use tauri::ipc::Channel;
use tauri::{AppHandle, Emitter, Manager};
use url::Url;

use crate::error::AppError;
use crate::repo::{Database, now_unix_secs};
use crate::services::app_release::CURRENT_APP_VERSION;
#[cfg(target_os = "android")]
use crate::services::rtmaga_codec::RTMAGA_MIME_TYPE;
use crate::services::rtmaga_codec::{
    BLOCK_MANGA, BLOCK_PREFS, BLOCK_TRACKERS, BackupBlockSelection, BackupHistoryPointer,
    BackupMangaEntry, BackupMeta, BackupPrefs, BackupTrackerEntry, CodecError, DecodedRtmagaBackup,
    DeltaChapter, DictInterner, RtmagaPayload, decode_backup, format_backup_filename,
    write_encoded_to,
};
#[cfg(test)]
use crate::services::rtmaga_codec::{RTMAGA_HEADER_LEN, encode_backup};
use crate::settings::Settings;
use crate::state::AppState;

/// Hard file size ceiling accepted before reading a `.rtmaga` file into memory (`24B + 32 MiB`).
const MAX_RTMAGA_FILE_BYTES: u64 = 24 + (32 * 1024 * 1024);

/// Conflict resolution strategy when importing a `.rtmaga` backup.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub(crate) enum BackupImportMode {
    /// Match manga by `(sourceId, mangaUrl)`, merge metadata, union bookmarks & chapter read
    /// flags, and take max `lastPageRead` / `lastReadAt` (LWW).
    #[default]
    SmartMerge,
    /// Clear existing records in the imported blocks and replace them atomically inside one
    /// SQLite transaction.
    Overwrite,
}

/// Identifies whether a `.rtmaga` path is a persistent user-selected file or an app-cache
/// temporary staging file (e.g., copied from Android SAF) that must be removed after import.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BackupFileOrigin {
    UserFile,
    TempStaging,
}

/// Real-time progress notification streamed over a Tauri `Channel` during export or import.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BackupProgressEvent {
    pub stage: String,
    pub percent: u8,
    pub current: usize,
    pub total: usize,
    pub detail: String,
}

/// Summary returned after exporting a `.rtmaga` backup.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BackupExportResult {
    pub saved: bool,
    pub cancelled: bool,
    pub file_name: String,
    pub display_path: Option<String>,
    pub file_size_bytes: u64,
    pub compressed_bytes: usize,
    pub uncompressed_bytes: usize,
    pub manga_count: usize,
    pub tracker_count: usize,
    pub included_prefs: bool,
    pub timestamp_ms: i64,
}

/// Validated metadata shown in the Svelte Import Preview modal prior to mutating SQLite.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
#[expect(
    clippy::struct_excessive_bools,
    reason = "JSON preview DTO carrying independent block presence flags for Svelte UI"
)]
pub(crate) struct BackupPreviewView {
    pub file_path: String,
    pub file_name: String,
    pub file_size_bytes: u64,
    pub compressed_bytes: usize,
    pub uncompressed_bytes: usize,
    pub format_version: u16,
    pub timestamp_ms: i64,
    pub app_version: String,
    pub device_name: String,
    pub checksum_hex: String,
    pub manga_count: usize,
    pub interacted_chapter_count: usize,
    pub tracker_count: usize,
    pub source_count: usize,
    pub category_count: usize,
    pub has_manga: bool,
    pub has_trackers: bool,
    pub has_prefs: bool,
    pub is_temp_file: bool,
}

/// Summary returned after atomically importing a `.rtmaga` backup.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BackupImportReport {
    pub mode: BackupImportMode,
    pub added_manga: usize,
    pub merged_manga: usize,
    pub merged_chapters: usize,
    pub restored_bookmarks: usize,
    pub restored_trackers: usize,
    pub restored_prefs: bool,
}

#[derive(Debug, FromRow)]
struct DbMangaExportRow {
    id: i64,
    url: String,
    site: String,
    title: String,
    cover_url: Option<String>,
    latest_chapter_url: Option<String>,
    latest_chapter_name: Option<String>,
    reader_mode_override: Option<String>,
    reader_settings_json: Option<String>,
    detected_webtoon: i64,
    hidden: i64,
    source_id: Option<String>,
    canonical_url: Option<String>,
    authors_json: Option<String>,
    artists_json: Option<String>,
    description: Option<String>,
    genres_json: Option<String>,
    tags_json: Option<String>,
    status: Option<String>,
    language: Option<String>,
    created_at: Option<i64>,
    bookmarked: i64,
    bookmarked_at: Option<i64>,
}

#[derive(Debug, FromRow)]
struct DbHistoryExportRow {
    manga_id: i64,
    chapter_url: String,
    chapter_name: String,
    scroll_ratio: f64,
    page_index: i64,
    read_at: i64,
}

#[derive(Debug, FromRow)]
struct DbReadChapterExportRow {
    manga_id: i64,
    url: String,
}

#[derive(Debug, FromRow)]
struct DbExistingMangaKeyRow {
    id: i64,
    site: String,
    url: String,
    canonical_url: Option<String>,
}

#[derive(Debug, FromRow)]
struct DbLocalHistoryRow {
    chapter_url: String,
    scroll_ratio: f64,
    page_index: i64,
    read_at: i64,
}

impl From<CodecError> for AppError {
    fn from(err: CodecError) -> Self {
        match err {
            CodecError::UnsupportedVersion { .. } | CodecError::UnsupportedFlags { .. } => {
                Self::UnsupportedFormat {
                    message: err.to_string(),
                }
            }
            CodecError::EncryptedUnsupported => Self::EncryptedArchive {
                message: err.to_string(),
            },
            CodecError::EncodeFailed(message) => Self::Internal { message },
            _ => Self::CorruptedData {
                message: err.to_string(),
            },
        }
    }
}

fn now_unix_ms() -> i64 {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    i64::try_from(millis).unwrap_or(i64::MAX)
}

fn secs_to_ms(secs: i64) -> i64 {
    secs.saturating_mul(1000)
}

fn ms_to_secs(ms: i64) -> i64 {
    if ms <= 0 { 0 } else { ms / 1000 }
}

fn emit_progress(
    channel: Option<&Channel<BackupProgressEvent>>,
    stage: &str,
    percent: u8,
    current: usize,
    total: usize,
    detail: impl Into<String>,
) {
    if let Some(ch) = channel {
        let _ = ch.send(BackupProgressEvent {
            stage: stage.to_owned(),
            percent: percent.min(100),
            current,
            total,
            detail: detail.into(),
        });
    }
}

fn parse_json_str_vec(raw: Option<String>) -> Vec<String> {
    raw.and_then(|s| serde_json::from_str::<Vec<String>>(&s).ok())
        .unwrap_or_default()
}

fn json_vec_opt(items: &[String]) -> Option<String> {
    let mut seen = HashSet::with_capacity(items.len());
    let mut compact: Vec<&str> = Vec::with_capacity(items.len());
    for item in items {
        let trimmed = item.trim();
        if trimmed.is_empty() {
            continue;
        }
        if seen.insert(trimmed.to_ascii_lowercase()) {
            compact.push(trimmed);
        }
    }
    if compact.is_empty() {
        None
    } else {
        serde_json::to_string(&compact).ok()
    }
}

fn norm_site_key(raw: &str) -> String {
    raw.trim().to_ascii_lowercase()
}

fn norm_url_key(raw: &str) -> &str {
    raw.trim().trim_end_matches('/')
}

fn is_valid_backup_url(raw: &str) -> bool {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return false;
    }
    let lower = trimmed.to_ascii_lowercase();
    if lower.starts_with("data:") || lower.starts_with("javascript:") || lower.starts_with("file:")
    {
        return false;
    }
    if crate::services::comics::is_local_comic_url(trimmed)
        || crate::services::comics::is_tachibk_url(trimmed)
    {
        return true;
    }
    Url::parse(trimmed).is_ok_and(|u| mangacore::http::validate_public_http_url(&u).is_ok())
}

fn is_valid_cover_url(raw: &str) -> bool {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed.len() > 2048 {
        return false;
    }
    if trimmed.to_ascii_lowercase().starts_with("data:") {
        return false;
    }
    is_valid_backup_url(trimmed)
}

fn sanitize_ratio(raw: f64) -> f64 {
    if raw.is_finite() {
        raw.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

fn derive_fallback_chapter_name(chapter_url: &str) -> String {
    let trimmed = norm_url_key(chapter_url);
    if let Some(num) = mangacore::parse::chapters::extract_chapter_number(trimmed) {
        if (num.fract()).abs() < f32::EPSILON {
            return format!("Chapter {}", num.trunc());
        }
        return format!("Chapter {num}");
    }
    trimmed
        .rsplit('/')
        .find(|seg| !seg.trim().is_empty())
        .map_or_else(|| "Chapter".to_owned(), str::to_owned)
}

/// Reads selected blocks from SQLite and constructs a compact, string-interned,
/// credential-stripped [`RtmagaPayload`].
///
/// # Errors
///
/// Returns [`AppError`] if no blocks are selected or a database query fails.
#[expect(
    clippy::too_many_lines,
    reason = "sequential multi-block SQLite snapshot assembly with string interning and delta chapter compaction"
)]
pub(crate) async fn build_payload_from_db(
    db: &Database,
    device_name: &str,
    selection: BackupBlockSelection,
    progress: Option<&Channel<BackupProgressEvent>>,
) -> Result<RtmagaPayload, AppError> {
    if !selection.any_selected() {
        return Err(AppError::InvalidUrl {
            message: "select at least one backup block to export".to_owned(),
        });
    }

    let exported_at_ms = now_unix_ms();
    emit_progress(progress, "reading_db", 10, 0, 100, "Reading local database");

    let mut interner = DictInterner::new();
    let mut manga_entries = Vec::new();

    if selection.manga {
        let rows: Vec<DbMangaExportRow> = sqlx::query_as(
            "SELECT
                m.id, m.url, m.site, m.title, m.cover_url,
                m.latest_chapter_url, m.latest_chapter_name,
                m.reader_mode_override, m.reader_settings_json,
                m.detected_webtoon, m.hidden, m.source_id, m.canonical_url,
                m.authors_json, m.artists_json, m.description,
                m.genres_json, m.tags_json, m.status, m.language, m.created_at,
                CASE WHEN b.manga_id IS NOT NULL THEN 1 ELSE 0 END AS bookmarked,
                b.created_at AS bookmarked_at
             FROM manga m
             LEFT JOIN bookmark b ON b.manga_id = m.id
             ORDER BY m.id ASC",
        )
        .fetch_all(db.pool())
        .await?;

        let history_rows: Vec<DbHistoryExportRow> = sqlx::query_as(
            "SELECT manga_id, chapter_url, chapter_name, scroll_ratio, page_index, read_at FROM history",
        )
        .fetch_all(db.pool())
        .await?;
        let mut history_by_manga: HashMap<i64, DbHistoryExportRow> =
            HashMap::with_capacity(history_rows.len());
        for h in history_rows {
            history_by_manga.insert(h.manga_id, h);
        }

        // Delta chapter storage: fetch ONLY interacted chapters (read = 1); in-progress history
        // chapters are merged right below.
        let read_rows: Vec<DbReadChapterExportRow> = sqlx::query_as(
            "SELECT manga_id, url FROM chapter WHERE read = 1 ORDER BY manga_id ASC, idx ASC",
        )
        .fetch_all(db.pool())
        .await?;
        let mut read_by_manga: HashMap<i64, Vec<String>> = HashMap::new();
        for ch in read_rows {
            read_by_manga.entry(ch.manga_id).or_default().push(ch.url);
        }

        let total_rows = rows.len();
        manga_entries.reserve(total_rows);

        for (idx, row) in rows.into_iter().enumerate() {
            let source_idx = interner.intern_source(&row.site);
            let genres = parse_json_str_vec(row.genres_json);
            let tags = parse_json_str_vec(row.tags_json);
            let genre_indices: Vec<u32> = genres
                .iter()
                .filter(|g| !g.trim().is_empty())
                .map(|g| interner.intern_category(g))
                .collect();
            let tag_indices: Vec<u32> = tags
                .iter()
                .filter(|t| !t.trim().is_empty())
                .map(|t| interner.intern_category(t))
                .collect();

            let hist_row = history_by_manga.remove(&row.id);
            let read_urls = read_by_manga.remove(&row.id).unwrap_or_default();

            let mut delta_chapters: Vec<DeltaChapter> =
                Vec::with_capacity(read_urls.len() + usize::from(hist_row.is_some()));
            let mut seen_norm_keys: HashSet<String> = HashSet::with_capacity(read_urls.len() + 1);

            for r_url in read_urls {
                let norm = norm_url_key(&r_url).to_owned();
                if !seen_norm_keys.insert(norm.clone()) {
                    continue;
                }
                let (last_page, last_read_at_ms) = match hist_row.as_ref() {
                    Some(h) if norm_url_key(&h.chapter_url) == norm => {
                        (h.page_index.max(0), secs_to_ms(h.read_at))
                    }
                    _ => (0, 0),
                };
                delta_chapters.push(DeltaChapter(r_url, true, last_page, last_read_at_ms));
            }

            let history_ptr = hist_row.map(|h| {
                let norm = norm_url_key(&h.chapter_url).to_owned();
                let read_ms = secs_to_ms(h.read_at);
                let page = h.page_index.max(0);
                if seen_norm_keys.insert(norm) {
                    delta_chapters.push(DeltaChapter(h.chapter_url.clone(), false, page, read_ms));
                }
                BackupHistoryPointer {
                    chapter_key: h.chapter_url,
                    chapter_name: h.chapter_name,
                    scroll_ratio: sanitize_ratio(h.scroll_ratio),
                    last_page_read: page,
                    last_read_at_ms: read_ms,
                }
            });

            // Never export binary data URIs or invalid cover blobs — store cover URLs only.
            let cover_url = row
                .cover_url
                .filter(|u| is_valid_cover_url(u))
                .map(|u| u.trim().to_owned());

            manga_entries.push(BackupMangaEntry {
                source_idx,
                url: row.url,
                canonical_url: row.canonical_url,
                source_id: row.source_id,
                title: row.title,
                cover_url,
                latest_chapter_url: row.latest_chapter_url,
                latest_chapter_name: row.latest_chapter_name,
                bookmarked: row.bookmarked != 0,
                bookmarked_at_ms: row.bookmarked_at.map(secs_to_ms),
                hidden: row.hidden != 0,
                reader_mode_override: row.reader_mode_override,
                reader_settings_json: row.reader_settings_json,
                detected_webtoon: row.detected_webtoon != 0,
                genre_indices,
                tag_indices,
                authors: parse_json_str_vec(row.authors_json),
                artists: parse_json_str_vec(row.artists_json),
                description: row.description,
                status: row.status,
                language: row.language,
                created_at_ms: row.created_at.map(secs_to_ms),
                chapters: delta_chapters,
                history: history_ptr,
            });

            if total_rows > 0 && (idx + 1 == total_rows || (idx + 1) % 100 == 0) {
                let pct = 10_u8.saturating_add(
                    u8::try_from(((idx + 1) * 35) / total_rows.max(1)).unwrap_or(35),
                );
                emit_progress(
                    progress,
                    "reading_db",
                    pct,
                    idx + 1,
                    total_rows,
                    format!("Packed {} / {} titles", idx + 1, total_rows),
                );
            }
        }
    }

    let mut trackers = Vec::new();
    if selection.trackers {
        let saved = db
            .list_saved_sources(crate::repo::sources::VisibilityFilter::All)
            .await?;
        trackers.reserve(saved.len());
        for s in saved {
            let icon_url = s
                .icon_url
                .filter(|u| is_valid_cover_url(u))
                .map(|u| u.trim().to_owned());
            trackers.push(BackupTrackerEntry {
                source_key: s.source_key,
                display_name: s.display_name,
                base_url: s.base_url,
                source_kind: s.source_kind,
                parser_ref: s.parser_ref,
                search_template: s.search_template,
                icon_url,
                is_enabled: s.is_enabled != 0,
                is_supplemental: s.is_supplemental != 0,
                manual_visibility: s.manual_visibility != 0,
            });
        }
    }

    let prefs = if selection.prefs {
        let safe_settings = db.load_settings().await?.sanitized_for_backup();
        let settings_json =
            serde_json::to_string(&safe_settings).map_err(|e| AppError::Internal {
                message: format!("failed to serialize sanitized settings: {e}"),
            })?;
        Some(BackupPrefs { settings_json })
    } else {
        None
    };

    emit_progress(
        progress,
        "encoding",
        55,
        0,
        100,
        "Compressing binary archive",
    );

    Ok(RtmagaPayload {
        meta: BackupMeta {
            app_version: CURRENT_APP_VERSION.to_owned(),
            device_name: device_name.to_owned(),
            exported_at_ms,
            blocks_mask: selection.to_mask(),
        },
        dict: interner.finish(),
        manga: manga_entries,
        trackers,
        prefs,
    })
}

/// Applies a validated [`DecodedRtmagaBackup`] into SQLite inside a single atomic
/// `BEGIN IMMEDIATE` transaction (automatically rolled back on any error).
///
/// # Errors
///
/// Returns [`AppError`] if the database transaction fails.
#[expect(
    clippy::too_many_lines,
    reason = "single atomic BEGIN IMMEDIATE import transaction reconciling manga, delta chapters, bookmarks, trackers, and prefs"
)]
pub(crate) async fn apply_backup_to_db(
    db: &Database,
    decoded: &DecodedRtmagaBackup,
    mode: BackupImportMode,
    progress: Option<&Channel<BackupProgressEvent>>,
) -> Result<BackupImportReport, AppError> {
    let payload = &decoded.payload;
    let mask = payload.meta.blocks_mask;
    let import_manga = (mask & BLOCK_MANGA) != 0 || !payload.manga.is_empty();
    let import_trackers = (mask & BLOCK_TRACKERS) != 0 || !payload.trackers.is_empty();
    let import_prefs = (mask & BLOCK_PREFS) != 0 && payload.prefs.is_some();

    let mut report = BackupImportReport {
        mode,
        ..BackupImportReport::default()
    };
    let now = now_unix_secs();

    emit_progress(
        progress,
        "importing_db",
        45,
        0,
        payload.manga.len(),
        "Starting atomic database transaction",
    );

    let mut conn = db.pool().acquire().await?;
    let mut tx = conn.begin_with(Cow::Borrowed("BEGIN IMMEDIATE")).await?;

    if mode == BackupImportMode::Overwrite {
        if import_manga {
            sqlx::query("DELETE FROM manga").execute(&mut *tx).await?;
            sqlx::query("UPDATE reader_session SET active = 0 WHERE id = 1")
                .execute(&mut *tx)
                .await?;
            sqlx::query("DELETE FROM settings WHERE key = 'last_reading_destination_v1'")
                .execute(&mut *tx)
                .await?;
        }
        if import_trackers {
            sqlx::query("DELETE FROM saved_source")
                .execute(&mut *tx)
                .await?;
        }
    }

    if import_manga {
        // Composite key index `(sourceId, mangaUrl)` + fallback `(sourceId, canonicalUrl)`
        let existing_rows: Vec<DbExistingMangaKeyRow> =
            sqlx::query_as("SELECT id, site, url, canonical_url FROM manga")
                .fetch_all(&mut *tx)
                .await?;
        let mut by_composite: HashMap<(String, String), i64> =
            HashMap::with_capacity(existing_rows.len() * 2);
        for row in existing_rows {
            let site_key = norm_site_key(&row.site);
            by_composite.insert(
                (site_key.clone(), norm_url_key(&row.url).to_owned()),
                row.id,
            );
            if let Some(canon) = row.canonical_url.as_deref() {
                let norm_c = norm_url_key(canon);
                if !norm_c.is_empty() {
                    by_composite
                        .entry((site_key, norm_c.to_owned()))
                        .or_insert(row.id);
                }
            }
        }

        let total_manga = payload.manga.len();
        for (idx, entry) in payload.manga.iter().enumerate() {
            let Some(raw_source) = usize::try_from(entry.source_idx)
                .ok()
                .and_then(|i| payload.dict.sources.get(i))
            else {
                continue;
            };
            let site_clean = raw_source.trim();
            let url_clean = entry.url.trim();
            if site_clean.is_empty()
                || entry.title.trim().is_empty()
                || !is_valid_backup_url(url_clean)
            {
                continue;
            }

            let site_key = norm_site_key(site_clean);
            let url_key = norm_url_key(url_clean).to_owned();
            let canon_clean = entry
                .canonical_url
                .as_deref()
                .map(str::trim)
                .filter(|u| is_valid_backup_url(u));

            let matched_id = by_composite
                .get(&(site_key.clone(), url_key.clone()))
                .copied()
                .or_else(|| {
                    canon_clean.and_then(|c| {
                        by_composite
                            .get(&(site_key.clone(), norm_url_key(c).to_owned()))
                            .copied()
                    })
                });

            let cover_clean = entry
                .cover_url
                .as_deref()
                .map(str::trim)
                .filter(|u| is_valid_cover_url(u));
            let latest_url_clean = entry
                .latest_chapter_url
                .as_deref()
                .map(str::trim)
                .filter(|u| is_valid_backup_url(u));
            let latest_name_clean = entry
                .latest_chapter_name
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty());

            let resolved_genres: Vec<String> = entry
                .genre_indices
                .iter()
                .filter_map(|&i| {
                    usize::try_from(i)
                        .ok()
                        .and_then(|u| payload.dict.categories.get(u))
                })
                .cloned()
                .collect();
            let resolved_tags: Vec<String> = entry
                .tag_indices
                .iter()
                .filter_map(|&i| {
                    usize::try_from(i)
                        .ok()
                        .and_then(|u| payload.dict.categories.get(u))
                })
                .cloned()
                .collect();

            let genres_json = json_vec_opt(&resolved_genres);
            let tags_json = json_vec_opt(&resolved_tags);
            let authors_json = json_vec_opt(&entry.authors);
            let artists_json = json_vec_opt(&entry.artists);
            let created_secs = entry.created_at_ms.map_or(now, ms_to_secs);

            let manga_id = if let Some(existing_id) = matched_id {
                sqlx::query(
                    "UPDATE manga
                     SET cover_url = COALESCE(cover_url, ?),
                         latest_chapter_url = COALESCE(latest_chapter_url, ?),
                         latest_chapter_name = COALESCE(latest_chapter_name, ?),
                         source_id = COALESCE(source_id, ?),
                         canonical_url = COALESCE(canonical_url, ?),
                         authors_json = COALESCE(authors_json, ?),
                         artists_json = COALESCE(artists_json, ?),
                         description = COALESCE(description, ?),
                         genres_json = COALESCE(genres_json, ?),
                         tags_json = COALESCE(tags_json, ?),
                         status = COALESCE(status, ?),
                         language = COALESCE(language, ?),
                         reader_mode_override = COALESCE(reader_mode_override, ?),
                         reader_settings_json = COALESCE(reader_settings_json, ?),
                         detected_webtoon = MAX(detected_webtoon, ?),
                         updated_at = MAX(updated_at, ?)
                     WHERE id = ?",
                )
                .bind(cover_clean)
                .bind(latest_url_clean)
                .bind(latest_name_clean)
                .bind(entry.source_id.as_deref())
                .bind(canon_clean)
                .bind(authors_json.as_deref())
                .bind(artists_json.as_deref())
                .bind(entry.description.as_deref())
                .bind(genres_json.as_deref())
                .bind(tags_json.as_deref())
                .bind(entry.status.as_deref())
                .bind(entry.language.as_deref())
                .bind(entry.reader_mode_override.as_deref())
                .bind(entry.reader_settings_json.as_deref())
                .bind(i64::from(entry.detected_webtoon))
                .bind(now)
                .bind(existing_id)
                .execute(&mut *tx)
                .await?;
                report.merged_manga += 1;
                existing_id
            } else {
                let res = sqlx::query(
                    "INSERT INTO manga
                        (url, site, title, cover_url, latest_chapter_url, latest_chapter_name,
                         has_new, reader_mode_override, reader_settings_json, detected_webtoon,
                         hidden, source_id, canonical_url, authors_json, artists_json, description,
                         genres_json, tags_json, status, language, created_at, updated_at)
                     VALUES (?, ?, ?, ?, ?, ?, 0, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                )
                .bind(url_clean)
                .bind(site_clean)
                .bind(entry.title.trim())
                .bind(cover_clean)
                .bind(latest_url_clean)
                .bind(latest_name_clean)
                .bind(entry.reader_mode_override.as_deref())
                .bind(entry.reader_settings_json.as_deref())
                .bind(i64::from(entry.detected_webtoon))
                .bind(i64::from(entry.hidden))
                .bind(entry.source_id.as_deref())
                .bind(canon_clean)
                .bind(authors_json.as_deref())
                .bind(artists_json.as_deref())
                .bind(entry.description.as_deref())
                .bind(genres_json.as_deref())
                .bind(tags_json.as_deref())
                .bind(entry.status.as_deref())
                .bind(entry.language.as_deref())
                .bind(created_secs)
                .bind(now)
                .execute(&mut *tx)
                .await?;
                let new_id = res.last_insert_rowid();
                by_composite.insert((site_key.clone(), url_key), new_id);
                if let Some(c) = canon_clean {
                    by_composite.insert((site_key, norm_url_key(c).to_owned()), new_id);
                }
                report.added_manga += 1;
                new_id
            };

            if entry.bookmarked {
                let bm_secs = entry.bookmarked_at_ms.map_or(now, ms_to_secs);
                let bm_res = sqlx::query(
                    "INSERT INTO bookmark (manga_id, created_at) VALUES (?, ?)
                     ON CONFLICT(manga_id) DO NOTHING",
                )
                .bind(manga_id)
                .bind(bm_secs)
                .execute(&mut *tx)
                .await?;
                if bm_res.rows_affected() > 0 {
                    report.restored_bookmarks += 1;
                }
            }

            let merged_ch_count =
                merge_delta_chapters_and_history(&mut tx, manga_id, entry).await?;
            report.merged_chapters += merged_ch_count;

            if total_manga > 0 && (idx + 1 == total_manga || (idx + 1) % 50 == 0) {
                let pct = 45_u8.saturating_add(
                    u8::try_from(((idx + 1) * 45) / total_manga.max(1)).unwrap_or(45),
                );
                emit_progress(
                    progress,
                    "importing_db",
                    pct,
                    idx + 1,
                    total_manga,
                    format!("Merged {} / {} titles", idx + 1, total_manga),
                );
            }
        }
    }

    if import_trackers {
        for tr in &payload.trackers {
            let base = tr.base_url.trim();
            let key = tr.source_key.trim();
            let name = tr.display_name.trim();
            if key.is_empty() || name.is_empty() || !is_valid_backup_url(base) {
                continue;
            }
            let icon = tr
                .icon_url
                .as_deref()
                .map(str::trim)
                .filter(|u| is_valid_cover_url(u));
            sqlx::query(
                "INSERT INTO saved_source
                    (source_key, display_name, base_url, source_kind, parser_ref, search_template,
                     icon_url, is_enabled, is_supplemental, manual_visibility, created_at, updated_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                 ON CONFLICT(source_key) DO UPDATE SET
                    display_name = excluded.display_name,
                    base_url = excluded.base_url,
                    source_kind = excluded.source_kind,
                    parser_ref = COALESCE(excluded.parser_ref, saved_source.parser_ref),
                    search_template = COALESCE(excluded.search_template, saved_source.search_template),
                    icon_url = COALESCE(excluded.icon_url, saved_source.icon_url),
                    is_enabled = excluded.is_enabled,
                    is_supplemental = excluded.is_supplemental,
                    manual_visibility = excluded.manual_visibility,
                    updated_at = excluded.updated_at",
            )
            .bind(key)
            .bind(name)
            .bind(base)
            .bind(tr.source_kind.trim())
            .bind(tr.parser_ref.as_deref())
            .bind(tr.search_template.as_deref())
            .bind(icon)
            .bind(i64::from(tr.is_enabled))
            .bind(i64::from(tr.is_supplemental))
            .bind(i64::from(tr.manual_visibility))
            .bind(now)
            .bind(now)
            .execute(&mut *tx)
            .await?;
            report.restored_trackers += 1;
        }
    }

    if import_prefs && let Some(ref prefs) = payload.prefs {
        let parsed = serde_json::from_str::<Settings>(&prefs.settings_json)
            .map_err(|e| AppError::CorruptedData {
                message: format!("invalid preferences block in backup: {e}"),
            })?
            .sanitized_for_backup();
        let clean_json = serde_json::to_string(&parsed).map_err(|e| AppError::Internal {
            message: format!("failed to re-serialize restored settings: {e}"),
        })?;
        sqlx::query(
            "INSERT INTO settings (key, value) VALUES ('app_settings_v1', ?)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        )
        .bind(clean_json)
        .execute(&mut *tx)
        .await?;
        report.restored_prefs = true;
    }

    tx.commit().await?;
    emit_progress(progress, "done", 100, 100, 100, "Backup restore complete");
    Ok(report)
}

async fn merge_delta_chapters_and_history(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    manga_id: i64,
    entry: &BackupMangaEntry,
) -> Result<usize, AppError> {
    let (merged_chapters, best_delta_progress) = merge_delta_chapters(tx, manga_id, entry).await?;
    reconcile_manga_history(tx, manga_id, entry, best_delta_progress).await?;
    Ok(merged_chapters)
}

async fn merge_delta_chapters<'a>(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    manga_id: i64,
    entry: &'a BackupMangaEntry,
) -> Result<(usize, Option<(&'a str, i64, i64)>), AppError> {
    let mut next_idx: i64 =
        sqlx::query_scalar::<_, Option<i64>>("SELECT MAX(idx) FROM chapter WHERE manga_id = ?")
            .bind(manga_id)
            .fetch_one(&mut **tx)
            .await?
            .map_or(0, |m| m + 1);

    let mut merged_chapters = 0_usize;
    let mut best_delta_progress: Option<(&'a str, i64, i64)> = None;

    for DeltaChapter(ch_key, is_read, last_page, last_read_ms) in &entry.chapters {
        let trimmed_key = ch_key.trim();
        if !is_valid_backup_url(trimmed_key) {
            continue;
        }
        let read_flag = i64::from(*is_read);
        let updated = sqlx::query(
            "UPDATE chapter
             SET read = MAX(read, ?)
             WHERE manga_id = ? AND RTRIM(url, '/') = RTRIM(?, '/')",
        )
        .bind(read_flag)
        .bind(manga_id)
        .bind(trimmed_key)
        .execute(&mut **tx)
        .await?;

        if updated.rows_affected() == 0 {
            let fallback_name = entry
                .history
                .as_ref()
                .filter(|h| norm_url_key(&h.chapter_key) == norm_url_key(trimmed_key))
                .map_or_else(
                    || derive_fallback_chapter_name(trimmed_key),
                    |h| h.chapter_name.clone(),
                );
            sqlx::query(
                "INSERT INTO chapter (manga_id, url, name, idx, read)
                 VALUES (?, ?, ?, ?, ?)
                 ON CONFLICT(manga_id, url) DO UPDATE SET read = MAX(chapter.read, excluded.read)",
            )
            .bind(manga_id)
            .bind(trimmed_key)
            .bind(fallback_name)
            .bind(next_idx)
            .bind(read_flag)
            .execute(&mut **tx)
            .await?;
            next_idx += 1;
        }
        merged_chapters += 1;

        let page_clamped = (*last_page).max(0);
        let read_secs = ms_to_secs(*last_read_ms);
        if page_clamped > 0 || read_secs > 0 {
            let replace_best = match best_delta_progress {
                None => true,
                Some((_, best_page, best_secs)) => {
                    read_secs > best_secs || (read_secs == best_secs && page_clamped > best_page)
                }
            };
            if replace_best {
                best_delta_progress = Some((trimmed_key, page_clamped, read_secs));
            }
        }
    }

    Ok((merged_chapters, best_delta_progress))
}

async fn reconcile_manga_history(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    manga_id: i64,
    entry: &BackupMangaEntry,
    best_delta_progress: Option<(&str, i64, i64)>,
) -> Result<(), AppError> {
    // Reconcile `history` row using explicit LWW rules:
    // - Same chapter: `lastPageRead = max(existing, incoming)`, `lastReadAt = max(existing, incoming)`.
    // - Different chapter: latest `lastReadAt` wins while preserving max timestamp.
    let incoming_hist = entry
        .history
        .as_ref()
        .filter(|h| is_valid_backup_url(&h.chapter_key))
        .map(|h| {
            let mut page = h.last_page_read.max(0);
            let mut read_secs = ms_to_secs(h.last_read_at_ms);
            if let Some((dk, dp, ds)) = best_delta_progress
                && norm_url_key(dk) == norm_url_key(&h.chapter_key)
            {
                page = page.max(dp);
                read_secs = read_secs.max(ds);
            }
            (
                h.chapter_key.trim().to_owned(),
                h.chapter_name.trim().to_owned(),
                sanitize_ratio(h.scroll_ratio),
                page,
                read_secs,
            )
        })
        .or_else(|| {
            best_delta_progress
                .map(|(k, p, s)| (k.to_owned(), derive_fallback_chapter_name(k), 0.0_f64, p, s))
        });

    let Some((inc_url, inc_name, inc_ratio, inc_page, inc_read_secs)) = incoming_hist else {
        return Ok(());
    };

    let local_hist: Option<DbLocalHistoryRow> = sqlx::query_as(
        "SELECT chapter_url, scroll_ratio, page_index, read_at FROM history WHERE manga_id = ?",
    )
    .bind(manga_id)
    .fetch_optional(&mut **tx)
    .await?;

    match local_hist {
        None => {
            sqlx::query(
                "INSERT INTO history (manga_id, chapter_url, chapter_name, scroll_ratio, page_index, read_at)
                 VALUES (?, ?, ?, ?, ?, ?)",
            )
            .bind(manga_id)
            .bind(&inc_url)
            .bind(&inc_name)
            .bind(inc_ratio)
            .bind(inc_page)
            .bind(inc_read_secs.max(1))
            .execute(&mut **tx)
            .await?;
        }
        Some(local) if norm_url_key(&local.chapter_url) == norm_url_key(&inc_url) => {
            sqlx::query(
                "UPDATE history
                 SET scroll_ratio = ?, page_index = ?, read_at = ?
                 WHERE manga_id = ?",
            )
            .bind(local.scroll_ratio.max(inc_ratio))
            .bind(local.page_index.max(inc_page))
            .bind(local.read_at.max(inc_read_secs))
            .bind(manga_id)
            .execute(&mut **tx)
            .await?;
        }
        Some(local)
            if inc_read_secs > local.read_at
                || (inc_read_secs == local.read_at && inc_page > local.page_index) =>
        {
            sqlx::query(
                "UPDATE history
                 SET chapter_url = ?, chapter_name = ?, scroll_ratio = ?, page_index = ?, read_at = ?
                 WHERE manga_id = ?",
            )
            .bind(&inc_url)
            .bind(&inc_name)
            .bind(inc_ratio)
            .bind(inc_page)
            .bind(local.read_at.max(inc_read_secs))
            .bind(manga_id)
            .execute(&mut **tx)
            .await?;
        }
        Some(_) => {}
    }

    Ok(())
}

/// Validates and decodes a `.rtmaga` file on disk inside `spawn_blocking` and builds a UI preview.
///
/// # Errors
///
/// Returns [`AppError`] if the file cannot be read, exceeds size bounds, or fails 3-stage pre-flight validation.
pub(crate) async fn preview_rtmaga_file(
    file_path: PathBuf,
    display_name: Option<String>,
    origin: BackupFileOrigin,
) -> Result<BackupPreviewView, AppError> {
    let is_temp_file = matches!(origin, BackupFileOrigin::TempStaging);
    tokio::task::spawn_blocking(move || {
        let meta = std::fs::metadata(&file_path).map_err(|e| AppError::NotFound {
            message: format!("cannot read backup file: {e}"),
        })?;
        let file_size_bytes = meta.len();
        if file_size_bytes > MAX_RTMAGA_FILE_BYTES {
            return Err(AppError::CorruptedData {
                message: format!(
                    "backup file exceeds maximum allowed size ({file_size_bytes} > {MAX_RTMAGA_FILE_BYTES} bytes)"
                ),
            });
        }
        let raw = std::fs::read(&file_path).map_err(|e| AppError::Internal {
            message: format!("failed to read backup bytes: {e}"),
        })?;
        let decoded = decode_backup(&raw)?;
        let interacted_chapter_count: usize = decoded
            .payload
            .manga
            .iter()
            .map(|m| m.chapters.len())
            .sum();
        let file_name = display_name
            .filter(|s| !s.trim().is_empty())
            .or_else(|| {
                file_path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .map(str::to_owned)
            })
            .unwrap_or_else(|| "backup.rtmaga".to_owned());
        let mask = decoded.payload.meta.blocks_mask;

        Ok(BackupPreviewView {
            file_path: file_path.to_string_lossy().into_owned(),
            file_name,
            file_size_bytes,
            compressed_bytes: decoded.compressed_bytes,
            uncompressed_bytes: decoded.uncompressed_bytes,
            format_version: decoded.header.version,
            timestamp_ms: decoded.header.timestamp_ms,
            app_version: decoded.payload.meta.app_version,
            device_name: decoded.payload.meta.device_name,
            checksum_hex: format!("{:08x}", decoded.header.checksum),
            manga_count: decoded.payload.manga.len(),
            interacted_chapter_count,
            tracker_count: decoded.payload.trackers.len(),
            source_count: decoded.payload.dict.sources.len(),
            category_count: decoded.payload.dict.categories.len(),
            has_manga: (mask & BLOCK_MANGA) != 0 || !decoded.payload.manga.is_empty(),
            has_trackers: (mask & BLOCK_TRACKERS) != 0 || !decoded.payload.trackers.is_empty(),
            has_prefs: (mask & BLOCK_PREFS) != 0 && decoded.payload.prefs.is_some(),
            is_temp_file,
        })
    })
    .await
    .map_err(|e| AppError::Internal {
        message: format!("backup preview worker panicked: {e}"),
    })?
}

/// Interactive platform-aware `.rtmaga` export (Windows Save dialog / Android SAF).
///
/// # Errors
///
/// Returns [`AppError`] on DB, codec, dialog, or filesystem error.
pub(crate) async fn export_rtmaga_interactive(
    app: &AppHandle,
    state: &AppState,
    selection: BackupBlockSelection,
    progress: Channel<BackupProgressEvent>,
) -> Result<BackupExportResult, AppError> {
    let custom_name = state.current_settings().await.device.custom_name;
    let device_name =
        crate::android_env::resolve_display_device_name(Some(app), custom_name.as_deref());
    let payload =
        build_payload_from_db(&state.db, &device_name, selection, Some(&progress)).await?;

    #[cfg(target_os = "android")]
    {
        export_rtmaga_android(app, state, payload, progress).await
    }

    #[cfg(not(target_os = "android"))]
    {
        export_rtmaga_desktop(app, payload, progress).await
    }
}

#[cfg(target_os = "android")]
async fn export_rtmaga_android(
    app: &AppHandle,
    state: &AppState,
    payload: RtmagaPayload,
    progress: Channel<BackupProgressEvent>,
) -> Result<BackupExportResult, AppError> {
    let file_name = format_backup_filename(payload.meta.exported_at_ms);
    let manga_count = payload.manga.len();
    let tracker_count = payload.trackers.len();
    let included_prefs = payload.prefs.is_some();
    let timestamp_ms = payload.meta.exported_at_ms;

    let cache_dir = app
        .path()
        .app_cache_dir()
        .unwrap_or_else(|_| state.data_dir.join("tmp"));
    let nonce: u64 = rand::random();
    let tmp_path = cache_dir.join(format!("rtmaga_export_{nonce:016x}.rtmaga"));

    let tmp_for_write = tmp_path.clone();
    let (compressed_bytes, uncompressed_bytes, file_size_bytes) =
        tokio::task::spawn_blocking(move || -> Result<(usize, usize, u64), AppError> {
            if let Some(parent) = tmp_for_write.parent() {
                std::fs::create_dir_all(parent).map_err(|e| AppError::Internal {
                    message: format!("failed to prepare temp backup dir: {e}"),
                })?;
            }
            let file = File::create(&tmp_for_write).map_err(|e| AppError::Internal {
                message: format!("failed to create temp backup file: {e}"),
            })?;
            let mut writer = BufWriter::with_capacity(64 * 1024, file);
            let (_hdr, comp, uncomp) = write_encoded_to(&payload, &mut writer)?;
            let len = std::fs::metadata(&tmp_for_write).map_or(0, |m| m.len());
            Ok((comp, uncomp, len))
        })
        .await
        .map_err(|e| AppError::Internal {
            message: format!("backup write worker failed: {e}"),
        })??;

    emit_progress(
        Some(&progress),
        "saving",
        85,
        85,
        100,
        "Opening Android file saver",
    );
    let saf_res =
        crate::android_env::export_backup_saf(app, &tmp_path, &file_name, RTMAGA_MIME_TYPE);
    let _ = tokio::fs::remove_file(&tmp_path).await;
    let reply = saf_res?;

    if reply.cancelled {
        return Ok(BackupExportResult {
            saved: false,
            cancelled: true,
            file_name,
            display_path: None,
            file_size_bytes: 0,
            compressed_bytes,
            uncompressed_bytes,
            manga_count,
            tracker_count,
            included_prefs,
            timestamp_ms,
        });
    }

    emit_progress(Some(&progress), "done", 100, 100, 100, "Backup exported");
    Ok(BackupExportResult {
        saved: true,
        cancelled: false,
        file_name,
        display_path: reply.uri,
        file_size_bytes,
        compressed_bytes,
        uncompressed_bytes,
        manga_count,
        tracker_count,
        included_prefs,
        timestamp_ms,
    })
}

#[cfg(not(target_os = "android"))]
fn write_backup_atomically_to_path(
    dest_path: &Path,
    payload: &RtmagaPayload,
) -> Result<(usize, usize, u64), AppError> {
    if let Some(parent) = dest_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| AppError::Internal {
            message: format!("failed to prepare destination folder: {e}"),
        })?;
    }
    let nonce: u64 = rand::random();
    let tmp_path = dest_path.with_extension(format!("rtmaga.tmp.{nonce:08x}"));
    let write_res = (|| -> Result<(usize, usize), AppError> {
        let file = File::create(&tmp_path).map_err(|e| AppError::Internal {
            message: format!("failed to create backup file: {e}"),
        })?;
        let mut writer = BufWriter::with_capacity(64 * 1024, file);
        let (_hdr, comp, uncomp) = write_encoded_to(payload, &mut writer)?;
        Ok((comp, uncomp))
    })();
    let (comp, uncomp) = match write_res {
        Ok(v) => v,
        Err(e) => {
            let _ = std::fs::remove_file(&tmp_path);
            return Err(e);
        }
    };
    if let Err(err) = std::fs::rename(&tmp_path, dest_path) {
        let _ = std::fs::remove_file(&tmp_path);
        return Err(AppError::Internal {
            message: format!("failed to commit backup file: {err}"),
        });
    }
    let size = std::fs::metadata(dest_path).map_or(0, |m| m.len());
    Ok((comp, uncomp, size))
}

#[cfg(not(target_os = "android"))]
async fn export_rtmaga_desktop(
    app: &AppHandle,
    payload: RtmagaPayload,
    progress: Channel<BackupProgressEvent>,
) -> Result<BackupExportResult, AppError> {
    use tauri_plugin_dialog::DialogExt;

    let file_name = format_backup_filename(payload.meta.exported_at_ms);
    let manga_count = payload.manga.len();
    let tracker_count = payload.trackers.len();
    let included_prefs = payload.prefs.is_some();
    let timestamp_ms = payload.meta.exported_at_ms;

    emit_progress(
        Some(&progress),
        "saving",
        70,
        70,
        100,
        "Choose destination file",
    );
    let app_clone = app.clone();
    let default_name = file_name.clone();
    let chosen = tokio::task::spawn_blocking(move || {
        app_clone
            .dialog()
            .file()
            .set_file_name(&default_name)
            .add_filter("RtMaga Backup", &["rtmaga"])
            .blocking_save_file()
    })
    .await
    .map_err(|e| AppError::Internal {
        message: format!("save dialog task failed: {e}"),
    })?;

    let Some(file_path_enum) = chosen else {
        return Ok(BackupExportResult {
            saved: false,
            cancelled: true,
            file_name,
            display_path: None,
            file_size_bytes: 0,
            compressed_bytes: 0,
            uncompressed_bytes: 0,
            manga_count,
            tracker_count,
            included_prefs,
            timestamp_ms,
        });
    };

    let mut dest_path = file_path_enum.into_path().map_err(|e| AppError::Internal {
        message: format!("selected destination is not a local filesystem path: {e}"),
    })?;
    if dest_path
        .extension()
        .and_then(|e| e.to_str())
        .is_none_or(|ext| !ext.eq_ignore_ascii_case("rtmaga"))
    {
        dest_path.set_extension("rtmaga");
    }

    emit_progress(
        Some(&progress),
        "writing",
        85,
        85,
        100,
        "Streaming backup to disk",
    );
    let dest_for_write = dest_path.clone();
    let (compressed_bytes, uncompressed_bytes, file_size_bytes) =
        tokio::task::spawn_blocking(move || {
            write_backup_atomically_to_path(&dest_for_write, &payload)
        })
        .await
        .map_err(|e| AppError::Internal {
            message: format!("backup stream write worker failed: {e}"),
        })??;

    let display_path = dest_path
        .to_string_lossy()
        .trim_start_matches(r"\\?\")
        .to_owned();
    let saved_name = dest_path
        .file_name()
        .and_then(|n| n.to_str())
        .map_or(file_name, str::to_owned);

    emit_progress(Some(&progress), "done", 100, 100, 100, "Backup exported");
    Ok(BackupExportResult {
        saved: true,
        cancelled: false,
        file_name: saved_name,
        display_path: Some(display_path),
        file_size_bytes,
        compressed_bytes,
        uncompressed_bytes,
        manga_count,
        tracker_count,
        included_prefs,
        timestamp_ms,
    })
}

/// Interactive platform-aware `.rtmaga` file picker + 3-stage pre-flight validation preview.
///
/// Returns `Ok(None)` if the user cancelled the OS file picker.
///
/// # Errors
///
/// Returns [`AppError`] if the picked file fails 3-stage validation or cannot be read.
pub(crate) async fn pick_and_preview_rtmaga(
    app: &AppHandle,
) -> Result<Option<BackupPreviewView>, AppError> {
    #[cfg(target_os = "android")]
    {
        let picked = crate::android_env::pick_backup_saf(app)?;
        if picked.cancelled {
            return Ok(None);
        }
        let Some(temp_path) = picked.path else {
            return Ok(None);
        };
        let path_buf = PathBuf::from(&temp_path);
        match preview_rtmaga_file(
            path_buf.clone(),
            picked.display_name,
            BackupFileOrigin::TempStaging,
        )
        .await
        {
            Ok(view) => Ok(Some(view)),
            Err(err) => {
                let _ = tokio::fs::remove_file(&path_buf).await;
                Err(err)
            }
        }
    }

    #[cfg(not(target_os = "android"))]
    {
        use tauri_plugin_dialog::DialogExt;

        let app_clone = app.clone();
        let picked = tokio::task::spawn_blocking(move || {
            app_clone
                .dialog()
                .file()
                .add_filter("RtMaga Backup", &["rtmaga"])
                .blocking_pick_file()
        })
        .await
        .map_err(|e| AppError::Internal {
            message: format!("open dialog task failed: {e}"),
        })?;

        let Some(file_path_enum) = picked else {
            return Ok(None);
        };
        let path_buf = file_path_enum.into_path().map_err(|e| AppError::Internal {
            message: format!("selected backup is not a local path: {e}"),
        })?;

        let preview = preview_rtmaga_file(path_buf, None, BackupFileOrigin::UserFile).await?;
        Ok(Some(preview))
    }
}

/// Validates, decodes, and atomically imports a `.rtmaga` file into SQLite, reloading live
/// settings if the `prefs` block was restored.
///
/// # Errors
///
/// Returns [`AppError`] if pre-flight validation, decoding, or the SQLite transaction fails.
pub(crate) async fn import_rtmaga_file(
    app: &AppHandle,
    state: &Arc<AppState>,
    file_path: &Path,
    mode: BackupImportMode,
    origin: BackupFileOrigin,
    progress: Channel<BackupProgressEvent>,
) -> Result<BackupImportReport, AppError> {
    let is_temp_file = matches!(origin, BackupFileOrigin::TempStaging);
    emit_progress(
        Some(&progress),
        "validating",
        15,
        0,
        100,
        "Validating magic, version, and CRC32 checksum",
    );

    let path_for_read = file_path.to_path_buf();
    let decoded_res =
        tokio::task::spawn_blocking(move || -> Result<DecodedRtmagaBackup, AppError> {
            let meta = std::fs::metadata(&path_for_read).map_err(|e| AppError::NotFound {
                message: format!("backup file not found: {e}"),
            })?;
            if meta.len() > MAX_RTMAGA_FILE_BYTES {
                return Err(AppError::CorruptedData {
                    message: "backup file exceeds maximum allowed size".to_owned(),
                });
            }
            let bytes = std::fs::read(&path_for_read).map_err(|e| AppError::Internal {
                message: format!("failed to read backup file: {e}"),
            })?;
            Ok(decode_backup(&bytes)?)
        })
        .await
        .map_err(|e| AppError::Internal {
            message: format!("backup decode worker failed: {e}"),
        })?;

    let decoded = match decoded_res {
        Ok(d) => d,
        Err(err) => {
            if is_temp_file {
                let _ = tokio::fs::remove_file(file_path).await;
            }
            return Err(err);
        }
    };

    let report_res = apply_backup_to_db(&state.db, &decoded, mode, Some(&progress)).await;
    if is_temp_file {
        let _ = tokio::fs::remove_file(file_path).await;
    }
    let report = report_res?;

    state.chapters.invalidate_all();
    state.catalogs.invalidate_all();

    if report.restored_prefs
        && let Ok(reloaded) = state.db.load_settings().await
    {
        {
            let mut guard = state.settings.write().await;
            *guard = reloaded.clone();
        }
        state.registry.http().set_doh_config(reloaded.doh.clone());
        state
            .registry
            .http()
            .set_proxy_config(reloaded.proxy.clone());
        state
            .registry
            .http()
            .set_max_conn_per_host(usize::from(reloaded.sources.max_conn_per_host));
        state
            .registry
            .http()
            .set_speed_limit_kbps(reloaded.sources.speed_limit_kbps);
        let _ = app.emit("settings://changed", &reloaded);
    }

    Ok(report)
}

/// Deletes a temporary Android SAF import file inside the app cache directory when the user
/// cancels the Import Preview modal.
pub(crate) async fn discard_temp_backup_file(app: &AppHandle, raw_path: &str) {
    let candidate = PathBuf::from(raw_path);
    let Ok(cache_dir) = app.path().app_cache_dir() else {
        return;
    };
    let Ok(canon_cache) = tokio::fs::canonicalize(&cache_dir).await else {
        return;
    };
    let Ok(canon_file) = tokio::fs::canonicalize(&candidate).await else {
        return;
    };
    if canon_file.starts_with(&canon_cache)
        && canon_file
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.starts_with("rtmaga_import_"))
    {
        let _ = tokio::fs::remove_file(&canon_file).await;
    }
}

/// Helper used by tests and in-memory workflows to export raw `.rtmaga` bytes directly from `db`.
#[cfg(test)]
pub(crate) async fn export_backup_bytes_from_db(
    db: &Database,
    device_name: &str,
    selection: BackupBlockSelection,
) -> Result<Vec<u8>, AppError> {
    let payload = build_payload_from_db(db, device_name, selection, None).await?;
    let (bytes, _) = encode_backup(&payload)?;
    Ok(bytes)
}

/// Helper used by tests to validate + import raw `.rtmaga` bytes directly into `db`.
#[cfg(test)]
pub(crate) async fn import_backup_bytes_into_db(
    db: &Database,
    bytes: &[u8],
    mode: BackupImportMode,
) -> Result<BackupImportReport, AppError> {
    let decoded = decode_backup(bytes)?;
    apply_backup_to_db(db, &decoded, mode, None).await
}

#[cfg(test)]
mod tests {
    use mangacore::{ChapterRef, Manga, SiteId};
    use url::Url;

    use super::*;
    use crate::repo::sources::{SourcePlacement, UpsertSavedSource};

    fn mk_chapter(url: &str, name: &str, num: f32) -> ChapterRef {
        ChapterRef::simple(
            Url::parse(url).expect("valid test url"),
            name.to_owned(),
            Some(num),
        )
    }

    fn mk_test_series() -> Manga {
        Manga::minimal(
            Url::parse("https://example.com/series/starlight").expect("valid url"),
            SiteId::generic("example.com"),
            "Starlight Chronicle".to_owned(),
            Some(Url::parse("https://cdn.example.com/starlight-cover.jpg").expect("valid cover")),
            vec![
                mk_chapter(
                    "https://example.com/series/starlight/ch-1",
                    "Chapter 1",
                    1.0,
                ),
                mk_chapter(
                    "https://example.com/series/starlight/ch-2",
                    "Chapter 2",
                    2.0,
                ),
                mk_chapter(
                    "https://example.com/series/starlight/ch-3",
                    "Chapter 3",
                    3.0,
                ),
            ],
        )
    }

    async fn seed_source_backup_db(series: &Manga) -> Database {
        let db_src = Database::open_in_memory().await.expect("open src db");
        let src_manga_id = db_src.upsert_manga(series).await.expect("upsert src manga");
        db_src
            .mark_chapter_read(src_manga_id, "https://example.com/series/starlight/ch-1")
            .await
            .expect("mark ch1 read");
        db_src
            .save_history(
                src_manga_id,
                "https://example.com/series/starlight/ch-2",
                "Chapter 2",
                0.65,
                14,
            )
            .await
            .expect("save src history");
        assert!(
            db_src
                .toggle_bookmark(src_manga_id)
                .await
                .expect("bookmark"),
            "source manga should be bookmarked"
        );

        let mut sensitive_settings = Settings::default();
        sensitive_settings.proxy.default_proxy.username = Some("secret-user".to_owned());
        sensitive_settings.proxy.default_proxy.password = Some("top-secret-pass".to_owned());
        sensitive_settings.doh.custom_profile_id = Some("acct-12345".to_owned());
        db_src
            .save_settings(&sensitive_settings)
            .await
            .expect("save sensitive settings");

        let _ = db_src
            .upsert_discovered_source(&UpsertSavedSource {
                source_key: "example.com",
                display_name: "Example Comics",
                base_url: "https://example.com",
                source_kind: "universal_heuristic",
                parser_ref: None,
                search_template: None,
                icon_url: None,
                detected_adult: false,
                placement: SourcePlacement::Auto,
                prefer_new_name: true,
                filter_schema: None,
            })
            .await
            .expect("upsert source");
        db_src
    }

    async fn seed_target_backup_db(series: &Manga) -> (Database, i64) {
        let db_dst = Database::open_in_memory().await.expect("open dst db");
        let dst_manga_id = db_dst.upsert_manga(series).await.expect("upsert dst manga");
        db_dst
            .mark_chapter_read(dst_manga_id, "https://example.com/series/starlight/ch-3")
            .await
            .expect("mark ch3 read locally");
        db_dst
            .save_history(
                dst_manga_id,
                "https://example.com/series/starlight/ch-2",
                "Chapter 2",
                0.20,
                4,
            )
            .await
            .expect("save dst history");
        (db_dst, dst_manga_id)
    }

    async fn assert_tampered_import_leaves_db_untouched(
        db_dst: &Database,
        dst_manga_id: i64,
        bytes: &[u8],
    ) {
        let mut tampered = bytes.to_vec();
        let tamper_idx = RTMAGA_HEADER_LEN + (tampered.len() - RTMAGA_HEADER_LEN) / 2;
        tampered[tamper_idx] ^= 0x80;

        let err = import_backup_bytes_into_db(db_dst, &tampered, BackupImportMode::SmartMerge)
            .await
            .expect_err("tampered backup must be rejected before DB mutation");
        assert!(
            matches!(err, AppError::CorruptedData { .. }),
            "tampered checksum must yield CorruptedData, got {err:?}"
        );

        let chaps = db_dst
            .list_chapters(dst_manga_id)
            .await
            .expect("list chaps");
        assert_eq!(chaps[0].read, 0, "ch-1 must remain unread");
        assert_eq!(chaps[2].read, 1, "ch-3 must remain read");
        let hist = db_dst
            .get_history_for_manga(dst_manga_id)
            .await
            .expect("get hist")
            .expect("history exists");
        assert_eq!(hist.page_index, 4, "page_index must stay 4");
        assert!(
            !db_dst
                .is_bookmarked(dst_manga_id)
                .await
                .expect("bookmark check"),
            "bookmark must remain false after rejected tamper"
        );
    }

    #[tokio::test]
    async fn export_tamper_fails_without_db_mutation_and_smart_merge_preserves_lww() {
        let series = mk_test_series();
        let db_src = seed_source_backup_db(&series).await;

        // 1. Export `.rtmaga` binary and verify credentials stripped & delta-only chapters stored.
        let bytes =
            export_backup_bytes_from_db(&db_src, "DesktopRig", BackupBlockSelection::default())
                .await
                .expect("export backup bytes");

        let decoded = decode_backup(&bytes).expect("decode exported backup");
        assert_eq!(decoded.payload.manga.len(), 1, "must export 1 manga");
        assert_eq!(
            decoded.payload.manga[0].chapters.len(),
            2,
            "delta storage must export ONLY interacted chapters (ch-1 read + ch-2 in-progress)"
        );
        let prefs_json = &decoded
            .payload
            .prefs
            .as_ref()
            .expect("prefs block must exist")
            .settings_json;
        assert!(
            !prefs_json.contains("secret-user")
                && !prefs_json.contains("top-secret-pass")
                && !prefs_json.contains("acct-12345"),
            "exported prefs must strip all auth credentials and account IDs"
        );

        // 2. Seed Target DB and verify 1-byte tamper fails checksum with ZERO DB mutation.
        let (db_dst, dst_manga_id) = seed_target_backup_db(&series).await;
        assert_tampered_import_leaves_db_untouched(&db_dst, dst_manga_id, &bytes).await;

        // 3. Valid Smart Merge -> preserves `isRead = existing || incoming` AND `max(lastPageRead)`.
        let report = import_backup_bytes_into_db(&db_dst, &bytes, BackupImportMode::SmartMerge)
            .await
            .expect("valid Smart Merge must succeed");
        assert_eq!(report.merged_manga, 1, "existing manga must merge by key");
        assert_eq!(report.added_manga, 0, "must not duplicate existing manga");
        assert_eq!(report.restored_bookmarks, 1, "bookmark must be unioned");
        assert_eq!(report.restored_trackers, 1, "saved source must be restored");
        assert!(report.restored_prefs, "sanitized prefs must be restored");

        let chaps_merged = db_dst
            .list_chapters(dst_manga_id)
            .await
            .expect("list merged chaps");
        assert_eq!(chaps_merged[0].read, 1, "ch-1 isRead false || true");
        assert_eq!(chaps_merged[2].read, 1, "ch-3 isRead true || false");
        let hist_merged = db_dst
            .get_history_for_manga(dst_manga_id)
            .await
            .expect("hist merged")
            .expect("hist exists");
        assert_eq!(hist_merged.page_index, 14, "max(4, 14) == 14");
        assert!(
            db_dst.is_bookmarked(dst_manga_id).await.expect("bm merged"),
            "bookmark union"
        );

        // 4. Reverse direction: higher local page_index (22) > incoming (14) stays 22.
        db_dst
            .save_history(
                dst_manga_id,
                "https://example.com/series/starlight/ch-2",
                "Chapter 2",
                0.90,
                22,
            )
            .await
            .expect("advance local page to 22");
        let _ = import_backup_bytes_into_db(&db_dst, &bytes, BackupImportMode::SmartMerge)
            .await
            .expect("second Smart Merge must succeed");
        let hist_second = db_dst
            .get_history_for_manga(dst_manga_id)
            .await
            .expect("hist second")
            .expect("hist exists");
        assert_eq!(hist_second.page_index, 22, "max(22, 14) == 22");
    }
}
