//! User-designated `.cbz` comic archive importer, `ComicInfo.xml` metadata parser,
//! series/chapter grouper, and O(1) ZIP Central Directory page reader.
//!
//! Privacy & safety guarantees:
//! - Never scans directories or searches the user's device automatically; only reads files
//!   explicitly chosen or dropped by the user.
//! - Validates ZIP magic headers, blocks path traversal (`../`), enforces per-page uncompressed
//!   byte limits against Zip Bombs, verifies CRC-32 checksums, and validates image headers.

use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use bytes::Bytes;
use crc32fast::Hasher as Crc32Hasher;
use flate2::read::DeflateDecoder;
use mangacore::{Chapter, ChapterRef, Manga, PageImage, SiteId, inspect_image_bytes};
use quick_xml::Reader as XmlReader;
use quick_xml::events::Event;
use regex::Regex;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};
use url::Url;

use crate::cache::image::CacheEntryMeta;
use crate::error::AppError;
use crate::services::library::{FetchMode, MangaView, get_manga};
use crate::state::AppState;

/// Maximum single uncompressed image page size permitted inside a `.cbz` archive (32 MiB).
const MAX_CBZ_PAGE_BYTES: u64 = 32 * 1024 * 1024;

/// Maximum `.cbz` archive size permitted per imported file (1.5 GiB).
const MAX_CBZ_ARCHIVE_BYTES: u64 = 1536 * 1024 * 1024;

/// Maximum Central Directory tail search window (`64 KiB + 22 bytes`).
const EOCD_SEARCH_WINDOW: u64 = 65_557;

static CHAPTER_SUFFIX_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)[\s._-]*(?:vol(?:ume)?\.?\s*\d+[\s._-]*)?(?:chapter|chap|chương|chuong|ch|ep|tập|tap|c|#)\.?\s*(\d+(?:\.\d+)?)\b.*$",
    )
    .expect("valid cbz chapter suffix regex")
});
static VOLUME_ONLY_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)[\s._-]*(?:vol(?:ume)?|tập|tap|quyển|quyen)\.?\s*(\d+(?:\.\d+)?)\b.*$")
        .expect("valid cbz volume regex")
});
static TRAILING_NUM_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(?P<series>.+?)\s+0*(?P<num>\d{1,4}(?:\.\d+)?)$")
        .expect("valid trailing number regex")
});

/// Indexed location of a single image page inside an imported `.cbz` archive.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CbzPageEntry {
    /// Archive entry name (used for natural ordering and diagnostics).
    pub name: String,
    /// Compression method (`0` = Stored, `8` = Deflate).
    pub method: u16,
    /// Byte offset of the local file header inside the `.cbz` file.
    pub local_header_offset: u64,
    /// Compressed payload byte length.
    pub compressed_size: u64,
    /// Uncompressed payload byte length.
    pub uncompressed_size: u64,
    /// Expected CRC-32 checksum.
    pub crc32: u32,
}

#[derive(Debug, Default)]
struct ComicInfoMeta {
    series: Option<String>,
    title: Option<String>,
    number: Option<f32>,
    volume: Option<String>,
    summary: Option<String>,
    writer: Option<String>,
    penciller: Option<String>,
    genres: Vec<String>,
}

struct ParsedCbzArchive {
    series_title: String,
    chapter_title: String,
    chapter_number: Option<f32>,
    volume: Option<String>,
    summary: Option<String>,
    authors: Vec<String>,
    artists: Vec<String>,
    genres: Vec<String>,
    pages: Vec<CbzPageEntry>,
    first_page_bytes: Bytes,
    first_page_mime: String,
    file_size: u64,
    file_sha_short: String,
}

/// Opens the native Windows multi-file `.cbz` picker when explicitly clicked by the user.
/// Returns `Ok(None)` when the user cancels the dialog without selecting any file.
///
/// # Errors
///
/// Returns [`AppError`] if a chosen `.cbz` archive is malformed or storage fails.
#[cfg(target_os = "windows")]
pub(crate) async fn pick_and_import_cbz_windows(
    data_dir: PathBuf,
    state: &AppState,
    target_manga_id: Option<i64>,
) -> Result<Option<MangaView>, AppError> {
    let picked = tokio::task::spawn_blocking(open_win32_cbz_file_dialog)
        .await
        .map_err(|e| AppError::Internal {
            message: format!("file picker task failed: {e}"),
        })?;
    if picked.is_empty() {
        return Ok(None);
    }
    let view = import_cbz_paths(&data_dir, state, &picked, target_manga_id).await?;
    Ok(Some(view))
}

#[cfg(not(target_os = "windows"))]
#[expect(
    clippy::unused_async,
    reason = "matches Windows async IPC command signature; Android uses browser file input chunks"
)]
pub(crate) async fn pick_and_import_cbz_windows(
    _data_dir: PathBuf,
    _state: &AppState,
    _target_manga_id: Option<i64>,
) -> Result<Option<MangaView>, AppError> {
    Ok(None)
}

/// Initializes a temporary `.cbz` staging file in `<data_dir>/cbz/.staging/` for chunked `WebView`
/// `<input type="file">` uploads (primarily on Android).
///
/// # Errors
///
/// Returns [`AppError`] if the filename does not end with `.cbz` or creating the staging file fails.
pub(crate) async fn begin_cbz_staging(data_dir: &Path, filename: &str) -> Result<String, AppError> {
    let trimmed = filename.trim();
    if !Path::new(trimmed)
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("cbz"))
    {
        return Err(AppError::InvalidUrl {
            message: "only .cbz comic archive files are supported".to_owned(),
        });
    }
    let raw_stem = trimmed[..trimmed.len().saturating_sub(4)].trim();
    let safe_stem: String = raw_stem
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || matches!(c, ' ' | '-' | '_' | '.' | '(' | ')' | '#') {
                c
            } else {
                '_'
            }
        })
        .take(120)
        .collect();
    let clean_stem = if safe_stem.trim().is_empty() {
        "CBZ_Manga".to_owned()
    } else {
        safe_stem.trim().to_owned()
    };

    let nonce: u64 = rand::random();
    let upload_id = format!("stg_{nonce:016x}__{clean_stem}.cbz");
    let staging_dir = data_dir.join("cbz").join(".staging");
    tokio::fs::create_dir_all(&staging_dir)
        .await
        .map_err(|e| AppError::Internal {
            message: format!("failed to prepare CBZ staging directory: {e}"),
        })?;
    let dest = staging_dir.join(&upload_id);
    let _ = tokio::fs::File::create(&dest)
        .await
        .map_err(|e| AppError::Internal {
            message: format!("failed to create CBZ staging file: {e}"),
        })?;
    Ok(upload_id)
}

/// Appends a base64-encoded byte chunk to an active `.cbz` staging file.
///
/// # Errors
///
/// Returns [`AppError`] on invalid `upload_id`, invalid base64, or archive size overflow.
pub(crate) async fn append_cbz_staging_chunk(
    data_dir: &Path,
    upload_id: &str,
    chunk_b64: &str,
) -> Result<(), AppError> {
    validate_staging_id(upload_id)?;
    let bytes = BASE64_STANDARD
        .decode(chunk_b64.trim())
        .map_err(|e| AppError::InvalidUrl {
            message: format!("invalid base64 CBZ chunk: {e}"),
        })?;
    let path = data_dir.join("cbz").join(".staging").join(upload_id);
    let meta = tokio::fs::metadata(&path)
        .await
        .map_err(|e| AppError::NotFound {
            message: format!("CBZ staging session not found: {e}"),
        })?;
    let next_len = meta
        .len()
        .saturating_add(u64::try_from(bytes.len()).unwrap_or(u64::MAX));
    if next_len > MAX_CBZ_ARCHIVE_BYTES {
        let _ = tokio::fs::remove_file(&path).await;
        return Err(AppError::CorruptedData {
            message: "CBZ file exceeds maximum permitted 1.5 GiB limit".to_owned(),
        });
    }
    let mut file = tokio::fs::OpenOptions::new()
        .append(true)
        .open(&path)
        .await
        .map_err(|e| AppError::Internal {
            message: format!("failed to open CBZ staging file: {e}"),
        })?;
    file.write_all(&bytes)
        .await
        .map_err(|e| AppError::Internal {
            message: format!("failed to write CBZ staging chunk: {e}"),
        })?;
    Ok(())
}

/// Finalizes one or more staged `.cbz` uploads, imports them into `<data_dir>/cbz/`, and cleans up `.staging/`.
///
/// # Errors
///
/// Returns [`AppError`] if any staged archive is invalid or import fails.
pub(crate) async fn finish_cbz_staging_import(
    data_dir: &Path,
    state: &AppState,
    upload_ids: &[String],
    target_manga_id: Option<i64>,
) -> Result<MangaView, AppError> {
    let staging_dir = data_dir.join("cbz").join(".staging");
    let mut staged_paths = Vec::with_capacity(upload_ids.len());
    for uid in upload_ids {
        validate_staging_id(uid)?;
        staged_paths.push(staging_dir.join(uid));
    }

    let res = import_cbz_paths(data_dir, state, &staged_paths, target_manga_id).await;
    for p in &staged_paths {
        let _ = tokio::fs::remove_file(p).await;
    }
    res
}

fn validate_staging_id(upload_id: &str) -> Result<(), AppError> {
    if !upload_id.starts_with("stg_")
        || !Path::new(upload_id)
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("cbz"))
        || upload_id.contains("..")
        || upload_id.contains('/')
        || upload_id.contains('\\')
    {
        return Err(AppError::InvalidUrl {
            message: "invalid CBZ staging identifier".to_owned(),
        });
    }
    Ok(())
}

/// Imports one or more user-selected `.cbz` file paths into `<data_dir>/cbz/`, groups chapters
/// belonging to the same series, caches cover thumbnails, and returns the resulting [`MangaView`].
///
/// # Errors
///
/// Returns [`AppError`] if no valid `.cbz` files are provided, an archive is invalid/unsafe, or SQLite fails.
pub(crate) async fn import_cbz_paths(
    data_dir: &Path,
    state: &AppState,
    paths: &[PathBuf],
    target_manga_id: Option<i64>,
) -> Result<MangaView, AppError> {
    if paths.is_empty() {
        return Err(AppError::InvalidUrl {
            message: "no .cbz file selected".to_owned(),
        });
    }

    for p in paths {
        let is_cbz = p
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("cbz"));
        if !is_cbz {
            return Err(AppError::InvalidUrl {
                message: "only .cbz comic archive files are supported".to_owned(),
            });
        }
    }

    let pinned_series_title = if let Some(mid) = target_manga_id {
        let row = state
            .db
            .get_manga_by_id(mid)
            .await?
            .ok_or_else(|| AppError::NotFound {
                message: format!("manga {mid} not found"),
            })?;
        Some(row.title)
    } else {
        None
    };

    let mut groups: BTreeMap<String, Vec<(PathBuf, ParsedCbzArchive)>> = BTreeMap::new();
    for src_path in paths {
        let path_clone = src_path.clone();
        let override_series = pinned_series_title.clone();
        let parsed = tokio::task::spawn_blocking(move || {
            inspect_cbz_file_sync(&path_clone, override_series.as_deref())
        })
        .await
        .map_err(|e| AppError::Internal {
            message: format!("CBZ parser task failed: {e}"),
        })??;

        let slug = slugify_series(&parsed.series_title);
        groups
            .entry(slug)
            .or_default()
            .push((src_path.clone(), parsed));
    }

    let cbz_root = data_dir.join("cbz");
    tokio::fs::create_dir_all(&cbz_root)
        .await
        .map_err(|e| AppError::Internal {
            message: format!("failed to create internal CBZ storage dir: {e}"),
        })?;

    let max_mb = state.current_settings().await.cache.max_mb;
    let mut last_manga_id: Option<i64> = None;

    for (series_slug, mut items) in groups {
        items.sort_by(|a, b| compare_parsed_chapters(&a.1, &b.1));
        let manga_id = persist_cbz_series_group(
            &cbz_root,
            state,
            &series_slug,
            items,
            target_manga_id,
            max_mb,
        )
        .await?;
        last_manga_id = Some(manga_id);
    }

    let final_id = last_manga_id.ok_or_else(|| AppError::Internal {
        message: "no CBZ series imported".to_owned(),
    })?;
    get_manga(state, final_id, FetchMode::LocalOnly).await
}

#[expect(
    clippy::too_many_lines,
    reason = "copies imported CBZ chapters, merges existing series chapters, caches cover, and seeds history"
)]
async fn persist_cbz_series_group(
    cbz_root: &Path,
    state: &AppState,
    series_slug: &str,
    items: Vec<(PathBuf, ParsedCbzArchive)>,
    target_manga_id: Option<i64>,
    max_mb: u32,
) -> Result<i64, AppError> {
    let series_dir = cbz_root.join(series_slug);
    tokio::fs::create_dir_all(&series_dir)
        .await
        .map_err(|e| AppError::Internal {
            message: format!("failed to create CBZ series folder: {e}"),
        })?;

    let first = &items[0].1;
    let source_id = format!("cbz:{series_slug}");
    let existing_id = if let Some(tid) = target_manga_id {
        Some(tid)
    } else {
        state.db.find_manga_id_by_source_id(&source_id).await?
    };

    let series_url = Url::parse(&format!("https://cbz.local/series/{series_slug}"))
        .expect("static cbz series url is valid");
    let cover_url = Url::parse(&format!("https://cbz.local/cover/{series_slug}.img"))
        .expect("static cbz cover url is valid");

    // Merge existing chapters (if appending to an existing CBZ series) with newly imported ones.
    let mut merged_chapters: Vec<ChapterRef> = if let Some(eid) = existing_id {
        state
            .db
            .list_chapters(eid)
            .await?
            .into_iter()
            .filter_map(|r| {
                Some(ChapterRef {
                    url: Url::parse(&r.url).ok()?,
                    name: r.name,
                    number: r.number.map(|n| {
                        #[expect(
                            clippy::as_conversions,
                            clippy::cast_possible_truncation,
                            reason = "chapter number stored as f64 in SQLite"
                        )]
                        {
                            n as f32
                        }
                    }),
                    volume: r.volume,
                    published_at: r.published_at,
                    language: r.language,
                    source_id: r.source_id,
                })
            })
            .collect()
    } else {
        Vec::new()
    };

    let mut archive_records: Vec<(String, String, String, i64)> = Vec::with_capacity(items.len());

    for (src_path, parsed) in &items {
        let dest_filename = format!("{}.cbz", parsed.file_sha_short);
        let dest_path = series_dir.join(&dest_filename);
        if src_path != &dest_path {
            tokio::fs::copy(src_path, &dest_path)
                .await
                .map_err(|e| AppError::Internal {
                    message: format!("failed to copy CBZ file into app storage: {e}"),
                })?;
        }

        let chap_url_str = format!(
            "https://cbz.local/chapter/{series_slug}/{}",
            parsed.file_sha_short
        );
        let chap_url = Url::parse(&chap_url_str).expect("valid cbz chapter url");
        if let Some(existing_ch) = merged_chapters.iter_mut().find(|c| c.url == chap_url) {
            existing_ch.name.clone_from(&parsed.chapter_title);
            existing_ch.number = parsed.chapter_number;
            existing_ch.volume.clone_from(&parsed.volume);
        } else {
            merged_chapters.push(ChapterRef {
                url: chap_url,
                name: parsed.chapter_title.clone(),
                number: parsed.chapter_number,
                volume: parsed.volume.clone(),
                published_at: None,
                language: None,
                source_id: Some(parsed.file_sha_short.clone()),
            });
        }

        let rel_path = format!("{series_slug}/{dest_filename}");
        let pages_json = serde_json::to_string(&parsed.pages).map_err(|e| AppError::Internal {
            message: format!("failed to serialize CBZ page index: {e}"),
        })?;
        let fsize = i64::try_from(parsed.file_size).unwrap_or(i64::MAX);
        archive_records.push((chap_url_str, rel_path, pages_json, fsize));
    }

    merged_chapters.sort_by(|a, b| match (a.number, b.number) {
        (Some(na), Some(nb)) => na
            .partial_cmp(&nb)
            .unwrap_or(Ordering::Equal)
            .then_with(|| natural_cmp(&a.name, &b.name)),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => natural_cmp(&a.name, &b.name),
    });

    let mut manga = Manga::minimal(
        series_url.clone(),
        SiteId::known("local:cbz"),
        first.series_title.clone(),
        Some(cover_url.clone()),
        merged_chapters,
    );
    manga.source_id = Some(source_id);
    manga.description.clone_from(&first.summary);
    manga.authors.clone_from(&first.authors);
    manga.artists.clone_from(&first.artists);
    manga.genres.clone_from(&first.genres);

    let manga_id = state.db.upsert_manga_with_id(existing_id, &manga).await?;

    // Cache the extracted first page as the persistent cover thumbnail (`is_cover = 1`).
    let cover_key = crate::cache::ImageCache::key_for_url(cover_url.as_str());
    state
        .images
        .put(
            &cover_key,
            &first.first_page_bytes,
            &first.first_page_mime,
            max_mb,
            CacheEntryMeta {
                site: Some("local:cbz"),
                manga_id: Some(manga_id),
                is_cover: true,
                chapter_url: None,
            },
        )
        .await;

    for (chap_url_str, rel_path, pages_json, fsize) in archive_records {
        state
            .db
            .upsert_cbz_chapter_archive(manga_id, &chap_url_str, &rel_path, &pages_json, fsize)
            .await?;
    }

    // Seed an initial reading history entry if none exists yet so the imported CBZ manga
    // appears immediately on the Home screen.
    if state.db.get_history_for_manga(manga_id).await?.is_none()
        && let Some(first_ch) = manga.chapters.first()
    {
        state
            .db
            .save_history(manga_id, first_ch.url.as_str(), &first_ch.name, 0.0, 0)
            .await?;
    }

    state.chapters.invalidate_all();
    Ok(manga_id)
}

/// Returns `true` when `url` is a local `https://cbz.local/...` CBZ chapter or image URL.
#[must_use]
pub(crate) fn is_cbz_url(url: &str) -> bool {
    url.starts_with("https://cbz.local/")
}

/// Resolves a local `https://cbz.local/chapter/...` URL into a [`mangacore::Chapter`] backed by
/// indexed page URLs (`https://cbz.local/page/<manga_id>/<idx>?ch=...`) in `< 1ms`.
///
/// # Errors
///
/// Returns [`AppError::NotFound`] or [`AppError::CorruptedData`] if the archive record is missing.
pub(crate) async fn load_cbz_chapter(
    state: &AppState,
    chapter_url: &Url,
) -> Result<(Chapter, i64), AppError> {
    let (manga_id, _rel_path, pages_json) = state
        .db
        .get_cbz_archive_by_chapter_url(chapter_url.as_str())
        .await?
        .ok_or_else(|| AppError::NotFound {
            message: "CBZ archive chapter not found in local storage".to_owned(),
        })?;

    let pages: Vec<CbzPageEntry> =
        serde_json::from_str(&pages_json).map_err(|e| AppError::CorruptedData {
            message: format!("corrupted CBZ page index: {e}"),
        })?;
    if pages.is_empty() {
        return Err(AppError::CorruptedData {
            message: "CBZ chapter contains zero pages".to_owned(),
        });
    }

    let encoded_ch: String =
        url::form_urlencoded::byte_serialize(chapter_url.as_str().as_bytes()).collect();
    let images: Vec<PageImage> = (0..pages.len())
        .map(|idx| {
            let page_url = Url::parse(&format!(
                "https://cbz.local/page/{manga_id}/{idx}?ch={encoded_ch}"
            ))
            .expect("valid cbz page url");
            PageImage {
                url: page_url,
                fallbacks: Vec::new(),
                referer: chapter_url.clone(),
            }
        })
        .collect();

    let chapters = state.db.list_chapters(manga_id).await?;
    let pos = chapters.iter().position(|c| c.url == chapter_url.as_str());
    let name = pos.map_or_else(|| "CBZ Chapter".to_owned(), |i| chapters[i].name.clone());
    let prev = pos
        .filter(|&i| i > 0)
        .and_then(|i| Url::parse(&chapters[i - 1].url).ok());
    let next = pos
        .filter(|&i| i + 1 < chapters.len())
        .and_then(|i| Url::parse(&chapters[i + 1].url).ok());
    let manga_url = state
        .db
        .get_manga_by_id(manga_id)
        .await?
        .and_then(|m| Url::parse(&m.url).ok());

    Ok((
        Chapter {
            url: chapter_url.clone(),
            name,
            images,
            prev,
            next,
            manga_url,
        },
        manga_id,
    ))
}

/// Reads and decompresses a single image page (`https://cbz.local/page/<manga_id>/<idx>?ch=...`)
/// directly from its byte offset inside `<data_dir>/cbz/<rel_path>`.
///
/// # Errors
///
/// Returns [`AppError`] if the `.cbz` file is missing, tampered, or contains an invalid image.
pub(crate) async fn read_cbz_page_bytes(
    data_dir: &Path,
    state: &AppState,
    page_url: &Url,
) -> Result<(Vec<u8>, String), AppError> {
    let segs: Vec<&str> = page_url
        .path_segments()
        .map(|s| s.filter(|p| !p.is_empty()).collect())
        .unwrap_or_default();
    let ["page", manga_id_str, idx_str] = segs.as_slice() else {
        return Err(AppError::InvalidUrl {
            message: "malformed CBZ page URL".to_owned(),
        });
    };
    let _manga_id = manga_id_str
        .parse::<i64>()
        .map_err(|_| AppError::InvalidUrl {
            message: "invalid CBZ manga id".to_owned(),
        })?;
    let page_idx = idx_str.parse::<usize>().map_err(|_| AppError::InvalidUrl {
        message: "invalid CBZ page index".to_owned(),
    })?;
    let chapter_url = page_url
        .query_pairs()
        .find(|(k, _)| k == "ch")
        .map(|(_, v)| v.into_owned())
        .ok_or_else(|| AppError::InvalidUrl {
            message: "missing CBZ chapter query param".to_owned(),
        })?;

    let (_mid, rel_path, pages_json) = state
        .db
        .get_cbz_archive_by_chapter_url(&chapter_url)
        .await?
        .ok_or_else(|| AppError::NotFound {
            message: "CBZ chapter archive entry missing".to_owned(),
        })?;
    if rel_path.contains("..") {
        return Err(AppError::InvalidUrl {
            message: "disallowed CBZ relative path".to_owned(),
        });
    }

    let pages: Vec<CbzPageEntry> =
        serde_json::from_str(&pages_json).map_err(|e| AppError::CorruptedData {
            message: format!("corrupted CBZ page table: {e}"),
        })?;
    let entry = pages
        .get(page_idx)
        .cloned()
        .ok_or_else(|| AppError::NotFound {
            message: format!("CBZ page index {page_idx} out of range"),
        })?;

    let archive_path = data_dir.join("cbz").join(&rel_path);
    let mut file = tokio::fs::File::open(&archive_path)
        .await
        .map_err(|e| AppError::NotFound {
            message: format!("CBZ archive file missing on disk: {e}"),
        })?;

    file.seek(std::io::SeekFrom::Start(entry.local_header_offset))
        .await
        .map_err(|e| AppError::CorruptedData {
            message: format!("failed to seek CBZ local header: {e}"),
        })?;
    let mut local_hdr = [0_u8; 30];
    file.read_exact(&mut local_hdr)
        .await
        .map_err(|e| AppError::CorruptedData {
            message: format!("truncated CBZ local header: {e}"),
        })?;
    if local_hdr[0..4] != [0x50, 0x4B, 0x03, 0x04] {
        return Err(AppError::CorruptedData {
            message: "invalid ZIP local file header signature".to_owned(),
        });
    }
    let name_len = u64::from(u16::from_le_bytes([local_hdr[26], local_hdr[27]]));
    let extra_len = u64::from(u16::from_le_bytes([local_hdr[28], local_hdr[29]]));
    let data_offset = entry
        .local_header_offset
        .saturating_add(30)
        .saturating_add(name_len)
        .saturating_add(extra_len);

    if entry.compressed_size > MAX_CBZ_PAGE_BYTES || entry.uncompressed_size > MAX_CBZ_PAGE_BYTES {
        return Err(AppError::CorruptedData {
            message: "CBZ page exceeds maximum permitted 32 MiB limit".to_owned(),
        });
    }

    file.seek(std::io::SeekFrom::Start(data_offset))
        .await
        .map_err(|e| AppError::CorruptedData {
            message: format!("failed to seek CBZ page payload: {e}"),
        })?;
    let comp_len = usize::try_from(entry.compressed_size).unwrap_or(0);
    let mut compressed = vec![0_u8; comp_len];
    file.read_exact(&mut compressed)
        .await
        .map_err(|e| AppError::CorruptedData {
            message: format!("truncated CBZ page payload: {e}"),
        })?;

    tokio::task::spawn_blocking(move || decode_and_verify_zip_entry(&entry, &compressed))
        .await
        .map_err(|e| AppError::Internal {
            message: format!("CBZ decode worker failed: {e}"),
        })?
}

/// Removes all stored `.cbz` files associated with `manga_id` from `<data_dir>/cbz/` when the
/// user permanently deletes a manga.
pub(crate) async fn delete_cbz_files_for_manga(data_dir: &Path, state: &AppState, manga_id: i64) {
    let Ok(rel_paths) = state.db.list_cbz_rel_paths_for_manga(manga_id).await else {
        return;
    };
    let cbz_root = data_dir.join("cbz");
    let mut parent_dirs = Vec::new();
    for rel in rel_paths {
        if rel.contains("..") {
            continue;
        }
        let full = cbz_root.join(&rel);
        if let Some(p) = full.parent() {
            parent_dirs.push(p.to_path_buf());
        }
        let _ = tokio::fs::remove_file(&full).await;
    }
    for dir in parent_dirs {
        if dir != cbz_root {
            let _ = tokio::fs::remove_dir(&dir).await;
        }
    }
}

#[expect(
    clippy::too_many_lines,
    reason = "validates ZIP header, parses central directory, ComicInfo.xml, cover image, and filename stem"
)]
fn inspect_cbz_file_sync(
    path: &Path,
    override_series: Option<&str>,
) -> Result<ParsedCbzArchive, AppError> {
    use std::io::{Read as _, Seek as _, SeekFrom};

    let mut file = std::fs::File::open(path).map_err(|e| AppError::NotFound {
        message: format!("cannot open .cbz file: {e}"),
    })?;
    let meta = file.metadata().map_err(|e| AppError::Internal {
        message: format!("cannot stat .cbz file: {e}"),
    })?;
    let file_size = meta.len();
    if !(22..=MAX_CBZ_ARCHIVE_BYTES).contains(&file_size) {
        return Err(AppError::CorruptedData {
            message: "invalid or oversized .cbz archive".to_owned(),
        });
    }

    let mut head_magic = [0_u8; 4];
    file.read_exact(&mut head_magic)
        .map_err(|_| AppError::CorruptedData {
            message: "unable to read .cbz header".to_owned(),
        })?;
    if head_magic != [0x50, 0x4B, 0x03, 0x04] {
        return Err(AppError::CorruptedData {
            message: "selected file is not a valid ZIP/.cbz archive".to_owned(),
        });
    }

    // Compute a fast deterministic digest from file size + head + tail for chapter deduplication.
    let file_sha_short = compute_fast_archive_fingerprint(&mut file, file_size)?;

    let raw_entries = read_zip_central_directory(&mut file, file_size)?;
    let mut comic_info = ComicInfoMeta::default();
    let mut pages = Vec::new();

    for entry in raw_entries {
        let lower_name = entry.name.to_ascii_lowercase();
        let file_part = lower_name.rsplit('/').next().unwrap_or(&lower_name);
        if file_part.starts_with('.') || lower_name.contains("__macosx") {
            continue;
        }
        if file_part == "comicinfo.xml" && entry.uncompressed_size <= 2 * 1024 * 1024 {
            if let Ok(xml_bytes) = read_entry_raw_sync(&mut file, &entry) {
                comic_info = parse_comic_info_xml(&xml_bytes);
            }
            continue;
        }
        if is_supported_image_ext(file_part) {
            if entry.uncompressed_size == 0 || entry.uncompressed_size > MAX_CBZ_PAGE_BYTES {
                continue;
            }
            pages.push(entry);
        }
    }

    pages.sort_by(|a, b| natural_cmp(&a.name, &b.name));
    if pages.is_empty() {
        return Err(AppError::CorruptedData {
            message: "no readable comic images found inside .cbz archive".to_owned(),
        });
    }

    let first_bytes = read_entry_raw_sync(&mut file, &pages[0])?;
    let header_info = inspect_image_bytes(&first_bytes).map_err(|e| AppError::CorruptedData {
        message: format!("first page in .cbz is not a valid image: {e:?}"),
    })?;
    let first_mime = header_info.format.mime_type().to_owned();

    let raw_stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("CBZ Manga")
        .trim();
    let stem = if let Some(rest) = raw_stem.strip_prefix("stg_")
        && let Some((hex_part, actual_stem)) = rest.split_once("__")
        && hex_part.len() == 16
        && !actual_stem.trim().is_empty()
    {
        actual_stem.trim()
    } else {
        raw_stem
    };
    let (inferred_series, inferred_chap_label, inferred_chap_num, inferred_vol) =
        infer_series_and_chapter_from_stem(stem);

    let series_title = override_series
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .or(comic_info.series)
        .unwrap_or(inferred_series);

    let chapter_number = comic_info.number.or(inferred_chap_num);
    let volume = comic_info.volume.or(inferred_vol);
    let chapter_title = comic_info
        .title
        .filter(|t| !t.is_empty())
        .unwrap_or(inferred_chap_label);

    let mut authors = Vec::new();
    if let Some(w) = comic_info.writer {
        authors.push(w);
    }
    let mut artists = Vec::new();
    if let Some(p) = comic_info.penciller {
        artists.push(p);
    }

    let _ = file.seek(SeekFrom::Start(0));

    Ok(ParsedCbzArchive {
        series_title,
        chapter_title,
        chapter_number,
        volume,
        summary: comic_info.summary,
        authors,
        artists,
        genres: comic_info.genres,
        pages,
        first_page_bytes: Bytes::from(first_bytes),
        first_page_mime: first_mime,
        file_size,
        file_sha_short,
    })
}

fn compute_fast_archive_fingerprint(
    file: &mut std::fs::File,
    file_size: u64,
) -> Result<String, AppError> {
    use std::io::{Read as _, Seek as _, SeekFrom};

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

fn read_zip_central_directory(
    file: &mut std::fs::File,
    file_size: u64,
) -> Result<Vec<CbzPageEntry>, AppError> {
    use std::io::{Read as _, Seek as _, SeekFrom};

    let tail_size = usize::try_from(file_size.min(EOCD_SEARCH_WINDOW)).unwrap_or(22);
    let tail_start = file_size.saturating_sub(u64::try_from(tail_size).unwrap_or(0));
    file.seek(SeekFrom::Start(tail_start))
        .map_err(|e| AppError::CorruptedData {
            message: format!("seek EOCD failed: {e}"),
        })?;
    let mut tail = vec![0_u8; tail_size];
    file.read_exact(&mut tail)
        .map_err(|e| AppError::CorruptedData {
            message: format!("read EOCD failed: {e}"),
        })?;

    let eocd_pos = (0..=tail.len().saturating_sub(22))
        .rev()
        .find(|&i| tail[i..i + 4] == [0x50, 0x4B, 0x05, 0x06])
        .ok_or_else(|| AppError::CorruptedData {
            message: "ZIP End of Central Directory not found in .cbz".to_owned(),
        })?;

    let mut cd_size = u64::from(u32::from_le_bytes([
        tail[eocd_pos + 12],
        tail[eocd_pos + 13],
        tail[eocd_pos + 14],
        tail[eocd_pos + 15],
    ]));
    let mut cd_offset = u64::from(u32::from_le_bytes([
        tail[eocd_pos + 16],
        tail[eocd_pos + 17],
        tail[eocd_pos + 18],
        tail[eocd_pos + 19],
    ]));

    // ZIP64 EOCD Locator (`0x07064b50` immediately preceding EOCD by 20 bytes)
    if eocd_pos >= 20 && tail[eocd_pos - 20..eocd_pos - 16] == [0x50, 0x4B, 0x06, 0x07] {
        let z64_offset = u64::from_le_bytes([
            tail[eocd_pos - 12],
            tail[eocd_pos - 11],
            tail[eocd_pos - 10],
            tail[eocd_pos - 9],
            tail[eocd_pos - 8],
            tail[eocd_pos - 7],
            tail[eocd_pos - 6],
            tail[eocd_pos - 5],
        ]);
        if z64_offset + 56 <= file_size {
            file.seek(SeekFrom::Start(z64_offset))
                .map_err(|e| AppError::CorruptedData {
                    message: e.to_string(),
                })?;
            let mut z64_hdr = [0_u8; 56];
            file.read_exact(&mut z64_hdr)
                .map_err(|e| AppError::CorruptedData {
                    message: e.to_string(),
                })?;
            if z64_hdr[0..4] == [0x50, 0x4B, 0x06, 0x06] {
                cd_size = u64::from_le_bytes([
                    z64_hdr[40],
                    z64_hdr[41],
                    z64_hdr[42],
                    z64_hdr[43],
                    z64_hdr[44],
                    z64_hdr[45],
                    z64_hdr[46],
                    z64_hdr[47],
                ]);
                cd_offset = u64::from_le_bytes([
                    z64_hdr[48],
                    z64_hdr[49],
                    z64_hdr[50],
                    z64_hdr[51],
                    z64_hdr[52],
                    z64_hdr[53],
                    z64_hdr[54],
                    z64_hdr[55],
                ]);
            }
        }
    }

    if cd_size > 32 * 1024 * 1024 || cd_offset.saturating_add(cd_size) > file_size {
        return Err(AppError::CorruptedData {
            message: "ZIP Central Directory bounds invalid".to_owned(),
        });
    }

    file.seek(SeekFrom::Start(cd_offset))
        .map_err(|e| AppError::CorruptedData {
            message: e.to_string(),
        })?;
    let cd_len = usize::try_from(cd_size).unwrap_or(0);
    let mut cd_buf = vec![0_u8; cd_len];
    file.read_exact(&mut cd_buf)
        .map_err(|e| AppError::CorruptedData {
            message: e.to_string(),
        })?;

    parse_central_directory_entries(&cd_buf)
}

fn parse_central_directory_entries(cd_buf: &[u8]) -> Result<Vec<CbzPageEntry>, AppError> {
    let mut out = Vec::new();
    let mut cursor = 0_usize;

    while cursor + 46 <= cd_buf.len() {
        if cd_buf[cursor..cursor + 4] != [0x50, 0x4B, 0x01, 0x02] {
            break;
        }
        let flags = u16::from_le_bytes([cd_buf[cursor + 8], cd_buf[cursor + 9]]);
        if (flags & 0x0001) != 0 {
            return Err(AppError::CorruptedData {
                message: "encrypted .cbz archives are not supported".to_owned(),
            });
        }
        let method = u16::from_le_bytes([cd_buf[cursor + 10], cd_buf[cursor + 11]]);
        let crc32 = u32::from_le_bytes([
            cd_buf[cursor + 16],
            cd_buf[cursor + 17],
            cd_buf[cursor + 18],
            cd_buf[cursor + 19],
        ]);
        let mut comp_size = u64::from(u32::from_le_bytes([
            cd_buf[cursor + 20],
            cd_buf[cursor + 21],
            cd_buf[cursor + 22],
            cd_buf[cursor + 23],
        ]));
        let mut uncomp_size = u64::from(u32::from_le_bytes([
            cd_buf[cursor + 24],
            cd_buf[cursor + 25],
            cd_buf[cursor + 26],
            cd_buf[cursor + 27],
        ]));
        let name_len = usize::from(u16::from_le_bytes([
            cd_buf[cursor + 28],
            cd_buf[cursor + 29],
        ]));
        let extra_len = usize::from(u16::from_le_bytes([
            cd_buf[cursor + 30],
            cd_buf[cursor + 31],
        ]));
        let comment_len = usize::from(u16::from_le_bytes([
            cd_buf[cursor + 32],
            cd_buf[cursor + 33],
        ]));
        let mut local_offset = u64::from(u32::from_le_bytes([
            cd_buf[cursor + 42],
            cd_buf[cursor + 43],
            cd_buf[cursor + 44],
            cd_buf[cursor + 45],
        ]));

        let name_start = cursor + 46;
        let extra_start = name_start.saturating_add(name_len);
        let next_cursor = extra_start
            .saturating_add(extra_len)
            .saturating_add(comment_len);
        if next_cursor > cd_buf.len() {
            break;
        }

        let raw_name = String::from_utf8_lossy(&cd_buf[name_start..extra_start]).replace('\\', "/");
        if raw_name.contains("..") || raw_name.ends_with('/') {
            cursor = next_cursor;
            continue;
        }

        // Parse ZIP64 extra field (`0x0001`) if 32-bit fields overflowed (`0xFFFF_FFFF`).
        if uncomp_size == u64::from(u32::MAX)
            || comp_size == u64::from(u32::MAX)
            || local_offset == u64::from(u32::MAX)
        {
            parse_zip64_extra(
                &cd_buf[extra_start..extra_start + extra_len],
                &mut uncomp_size,
                &mut comp_size,
                &mut local_offset,
            );
        }

        if matches!(method, 0 | 8) && out.len() < 4096 {
            out.push(CbzPageEntry {
                name: raw_name,
                method,
                local_header_offset: local_offset,
                compressed_size: comp_size,
                uncompressed_size: uncomp_size,
                crc32,
            });
        }
        cursor = next_cursor;
    }

    Ok(out)
}

fn parse_zip64_extra(
    extra: &[u8],
    uncomp_size: &mut u64,
    comp_size: &mut u64,
    local_offset: &mut u64,
) {
    let mut idx = 0_usize;
    while idx + 4 <= extra.len() {
        let header_id = u16::from_le_bytes([extra[idx], extra[idx + 1]]);
        let data_sz = usize::from(u16::from_le_bytes([extra[idx + 2], extra[idx + 3]]));
        let data_start = idx + 4;
        let data_end = data_start.saturating_add(data_sz);
        if data_end > extra.len() {
            break;
        }
        if header_id == 0x0001 {
            let mut p = data_start;
            if *uncomp_size == u64::from(u32::MAX) && p + 8 <= data_end {
                if let Ok(arr) = extra[p..p + 8].try_into() {
                    *uncomp_size = u64::from_le_bytes(arr);
                }
                p += 8;
            }
            if *comp_size == u64::from(u32::MAX) && p + 8 <= data_end {
                if let Ok(arr) = extra[p..p + 8].try_into() {
                    *comp_size = u64::from_le_bytes(arr);
                }
                p += 8;
            }
            if *local_offset == u64::from(u32::MAX)
                && p + 8 <= data_end
                && let Ok(arr) = extra[p..p + 8].try_into()
            {
                *local_offset = u64::from_le_bytes(arr);
            }
            break;
        }
        idx = data_end;
    }
}

fn read_entry_raw_sync(
    file: &mut std::fs::File,
    entry: &CbzPageEntry,
) -> Result<Vec<u8>, AppError> {
    use std::io::{Read as _, Seek as _, SeekFrom};

    file.seek(SeekFrom::Start(entry.local_header_offset))
        .map_err(|e| AppError::CorruptedData {
            message: e.to_string(),
        })?;
    let mut local_hdr = [0_u8; 30];
    file.read_exact(&mut local_hdr)
        .map_err(|e| AppError::CorruptedData {
            message: e.to_string(),
        })?;
    if local_hdr[0..4] != [0x50, 0x4B, 0x03, 0x04] {
        return Err(AppError::CorruptedData {
            message: "invalid local ZIP header".to_owned(),
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
            message: e.to_string(),
        })?;
    let comp_len = usize::try_from(entry.compressed_size).unwrap_or(0);
    let mut comp_buf = vec![0_u8; comp_len];
    file.read_exact(&mut comp_buf)
        .map_err(|e| AppError::CorruptedData {
            message: e.to_string(),
        })?;
    let (decoded, _mime) = decode_raw_zip_payload(entry, &comp_buf)?;
    Ok(decoded)
}

fn decode_raw_zip_payload(
    entry: &CbzPageEntry,
    compressed: &[u8],
) -> Result<(Vec<u8>, Option<String>), AppError> {
    let decoded = match entry.method {
        0 => compressed.to_vec(),
        8 => {
            let cap = usize::try_from(entry.uncompressed_size.min(MAX_CBZ_PAGE_BYTES)).unwrap_or(0);
            let mut out = Vec::with_capacity(cap);
            let decoder = DeflateDecoder::new(compressed);
            decoder
                .take(MAX_CBZ_PAGE_BYTES + 1)
                .read_to_end(&mut out)
                .map_err(|e| AppError::CorruptedData {
                    message: format!("deflate decompression failed: {e}"),
                })?;
            if u64::try_from(out.len()).unwrap_or(u64::MAX) > MAX_CBZ_PAGE_BYTES {
                return Err(AppError::CorruptedData {
                    message: "CBZ entry exceeded maximum uncompressed page size".to_owned(),
                });
            }
            out
        }
        other => {
            return Err(AppError::CorruptedData {
                message: format!("unsupported ZIP compression method {other}"),
            });
        }
    };

    let mut crc = Crc32Hasher::new();
    crc.update(&decoded);
    if entry.crc32 != 0 && crc.finalize() != entry.crc32 {
        return Err(AppError::CorruptedData {
            message: "CBZ entry CRC-32 verification failed".to_owned(),
        });
    }
    Ok((decoded, None))
}

fn decode_and_verify_zip_entry(
    entry: &CbzPageEntry,
    compressed: &[u8],
) -> Result<(Vec<u8>, String), AppError> {
    let (bytes, _) = decode_raw_zip_payload(entry, compressed)?;
    let info = inspect_image_bytes(&bytes).map_err(|e| AppError::CorruptedData {
        message: format!("invalid image in CBZ page: {e:?}"),
    })?;
    Ok((bytes, info.format.mime_type().to_owned()))
}

fn parse_comic_info_xml(xml_bytes: &[u8]) -> ComicInfoMeta {
    let mut reader = XmlReader::from_reader(xml_bytes);
    reader.config_mut().trim_text(true);
    let mut meta = ComicInfoMeta::default();
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
                            "volume" => {
                                if text != "-1" {
                                    meta.volume = Some(text.to_owned());
                                }
                            }
                            "summary" => meta.summary = Some(text.to_owned()),
                            "writer" => meta.writer = Some(text.to_owned()),
                            "penciller" => meta.penciller = Some(text.to_owned()),
                            "genre" => {
                                meta.genres = text
                                    .split(',')
                                    .map(str::trim)
                                    .filter(|s| !s.is_empty())
                                    .map(str::to_owned)
                                    .collect();
                            }
                            _ => {}
                        }
                    }
                }
            }
            Ok(Event::End(_)) => {
                current_tag.clear();
            }
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
        buf.clear();
    }
    meta
}

fn infer_series_and_chapter_from_stem(stem: &str) -> (String, String, Option<f32>, Option<String>) {
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

fn compare_parsed_chapters(a: &ParsedCbzArchive, b: &ParsedCbzArchive) -> Ordering {
    match (a.chapter_number, b.chapter_number) {
        (Some(na), Some(nb)) => na
            .partial_cmp(&nb)
            .unwrap_or(Ordering::Equal)
            .then_with(|| natural_cmp(&a.chapter_title, &b.chapter_title)),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => natural_cmp(&a.chapter_title, &b.chapter_title),
    }
}

fn slugify_series(title: &str) -> String {
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

fn is_supported_image_ext(filename_lower: &str) -> bool {
    [".jpg", ".jpeg", ".png", ".webp", ".gif", ".avif", ".bmp"]
        .iter()
        .any(|ext| filename_lower.ends_with(ext))
}

/// Natural alphanumeric comparator so `"page_2.jpg"` sorts before `"page_10.jpg"`.
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

#[cfg(target_os = "windows")]
fn open_win32_cbz_file_dialog() -> Vec<PathBuf> {
    use std::ffi::c_void;

    const OFN_ALLOWMULTISELECT: u32 = 0x0000_0200;
    const OFN_FILEMUSTEXIST: u32 = 0x0000_1000;
    const OFN_PATHMUSTEXIST: u32 = 0x0000_0800;
    const OFN_NOCHANGEDIR: u32 = 0x0000_0008;
    const OFN_EXPLORER: u32 = 0x0008_0000;

    #[repr(C)]
    struct OpenFileNameW {
        struct_size: u32,
        hwnd_owner: *mut c_void,
        instance: *mut c_void,
        filter: *const u16,
        custom_filter: *mut u16,
        max_cust_filter: u32,
        filter_index: u32,
        file: *mut u16,
        max_file: u32,
        file_title: *mut u16,
        max_file_title: u32,
        initial_dir: *const u16,
        title: *const u16,
        flags: u32,
        file_offset: u16,
        file_extension: u16,
        def_ext: *const u16,
        cust_data: usize,
        hook: *const c_void,
        template_name: *const u16,
        pv_reserved: *mut c_void,
        dw_reserved: u32,
        flags_ex: u32,
    }

    #[link(name = "comdlg32")]
    unsafe extern "system" {
        fn GetOpenFileNameW(ofn: *mut OpenFileNameW) -> i32;
    }

    let filter: Vec<u16> = "CBZ Comic Archives (*.cbz)\0*.cbz\0\0"
        .encode_utf16()
        .collect();
    let title: Vec<u16> = "Select .cbz Comic File(s)\0".encode_utf16().collect();
    let def_ext: Vec<u16> = "cbz\0".encode_utf16().collect();
    let mut file_buf = vec![0_u16; 32_768];

    let struct_sz = u32::try_from(std::mem::size_of::<OpenFileNameW>()).unwrap_or(0);
    let max_file_u32 = u32::try_from(file_buf.len()).unwrap_or(32_768);
    let mut ofn = OpenFileNameW {
        struct_size: struct_sz,
        hwnd_owner: std::ptr::null_mut(),
        instance: std::ptr::null_mut(),
        filter: filter.as_ptr(),
        custom_filter: std::ptr::null_mut(),
        max_cust_filter: 0,
        filter_index: 1,
        file: file_buf.as_mut_ptr(),
        max_file: max_file_u32,
        file_title: std::ptr::null_mut(),
        max_file_title: 0,
        initial_dir: std::ptr::null(),
        title: title.as_ptr(),
        flags: OFN_EXPLORER
            | OFN_ALLOWMULTISELECT
            | OFN_FILEMUSTEXIST
            | OFN_PATHMUSTEXIST
            | OFN_NOCHANGEDIR,
        file_offset: 0,
        file_extension: 0,
        def_ext: def_ext.as_ptr(),
        cust_data: 0,
        hook: std::ptr::null(),
        template_name: std::ptr::null(),
        pv_reserved: std::ptr::null_mut(),
        dw_reserved: 0,
        flags_ex: 0,
    };

    // SAFETY: `ofn` and all referenced UTF-16 buffers outlive the synchronous Win32 modal dialog call.
    let accepted = unsafe { GetOpenFileNameW(&raw mut ofn) };
    if accepted == 0 {
        return Vec::new();
    }

    parse_multiselect_utf16_buffer(&file_buf)
}

#[cfg(any(target_os = "windows", test))]
fn parse_multiselect_utf16_buffer(buf: &[u16]) -> Vec<PathBuf> {
    let mut parts: Vec<String> = Vec::new();
    let mut start = 0_usize;
    for (idx, &ch) in buf.iter().enumerate() {
        if ch == 0 {
            if idx == start {
                break;
            }
            parts.push(String::from_utf16_lossy(&buf[start..idx]));
            start = idx + 1;
        }
    }
    match parts.as_slice() {
        [] => Vec::new(),
        [single] => vec![PathBuf::from(single)],
        [dir, files @ ..] => {
            let base = PathBuf::from(dir);
            files.iter().map(|f| base.join(f)).collect()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn natural_sort_and_series_inference_work() {
        let mut names = vec!["page_10.jpg", "page_2.jpg", "page_1.jpg", "page_20.jpg"];
        names.sort_by(|a, b| natural_cmp(a, b));
        assert_eq!(
            names,
            vec!["page_1.jpg", "page_2.jpg", "page_10.jpg", "page_20.jpg"]
        );

        let (s1, c1, n1, _) = infer_series_and_chapter_from_stem("Sky Blade - Chap 12.5");
        assert_eq!(s1, "Sky Blade");
        assert_eq!(c1, "Chapter 12.5");
        assert_eq!(n1, Some(12.5));

        let (s2, _, n2, v2) = infer_series_and_chapter_from_stem("Iron Fortress Vol.03");
        assert_eq!(s2, "Iron Fortress");
        assert_eq!(n2, Some(3.0));
        assert_eq!(v2.as_deref(), Some("03"));
    }
}
