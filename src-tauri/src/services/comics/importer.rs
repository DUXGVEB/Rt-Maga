//! Unified multi-format comic importer, chunked `WebView` staging uploader, local chapter loader,
//! lazy on-demand `img://` page streaming reader, and permanent disk cleanup.

use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use mangacore::{Chapter, ChapterRef, Manga, PageImage, SiteId};
use tokio::io::AsyncWriteExt;
use url::Url;

use crate::cache::ImageCache;
use crate::cache::image::CacheEntryMeta;
use crate::error::AppError;
use crate::services::comics::folder::{
    DirectoryImportUnit, expand_directory_sync, inspect_loose_image_batch_sync, merge_metadata,
    read_local_image_async,
};
use crate::services::comics::pdf::{evict_cached_pdfs_for_prefix, render_pdf_page_sync};
use crate::services::comics::rar::{evict_cached_rars_for_prefix, read_rar_entry_sync};
use crate::services::comics::sevenz::read_sevenz_entry_sync;
use crate::services::comics::sort::{
    ComicMetadata, MAX_COMIC_ARCHIVE_BYTES, is_junk_entry, natural_cmp, slugify_series,
    strip_staging_prefix,
};
use crate::services::comics::tar_src::read_tar_entry_async;
use crate::services::comics::zip_epub::read_zip_entry_async;
use crate::services::comics::{
    ComicFormat, DetectedFileKind, PageDescriptor, ParsedComicChapter,
    deserialize_page_descriptors, detect_format_magic, inspect_single_archive_sync,
};
use crate::services::library::{FetchMode, MangaView, get_manga};
use crate::state::AppState;

enum PreparedChapterPayload {
    ArchiveCopy(PathBuf),
    ImageCopies(Vec<(PathBuf, String)>),
}

struct PreparedChapter {
    parsed: ParsedComicChapter,
    payload: PreparedChapterPayload,
}

/// Opens the native Windows multi-format file picker when clicked directly via IPC fallback.
/// Returns `Ok(None)` when the user cancels without picking any file.
///
/// # Errors
///
/// Returns [`AppError`] if a chosen file is invalid or storage fails.
#[cfg(target_os = "windows")]
pub(crate) async fn pick_and_import_comics_windows(
    data_dir: PathBuf,
    state: &AppState,
    target_manga_id: Option<i64>,
) -> Result<Option<MangaView>, AppError> {
    let picked = tokio::task::spawn_blocking(open_win32_comic_file_dialog)
        .await
        .map_err(|e| AppError::Internal {
            message: format!("file picker task failed: {e}"),
        })?;
    if picked.is_empty() {
        return Ok(None);
    }
    let view = import_comic_paths(&data_dir, state, &picked, target_manga_id).await?;
    Ok(Some(view))
}

#[cfg(not(target_os = "windows"))]
#[expect(
    clippy::unused_async,
    reason = "matches Windows async IPC command signature; Android uses browser file input chunks"
)]
pub(crate) async fn pick_and_import_comics_windows(
    _data_dir: PathBuf,
    _state: &AppState,
    _target_manga_id: Option<i64>,
) -> Result<Option<MangaView>, AppError> {
    Ok(None)
}

/// Initializes a temporary staging file in `<data_dir>/comics/.staging/` for chunked `WebView`
/// `<input type="file">` uploads (primarily on Android).
///
/// Accepts all supported comic archives, documents, loose images, and optional folder-relative
/// paths (e.g. `"MySeries/Chapter 1/001.jpg"` from `webkitRelativePath`).
///
/// # Errors
///
/// Returns [`AppError`] if the filename is empty or creating the staging file fails.
pub(crate) async fn begin_comic_staging(
    data_dir: &Path,
    filename: &str,
) -> Result<String, AppError> {
    let normalized = filename.trim().replace('\\', "/");
    if normalized.is_empty() || is_junk_entry(&normalized) {
        return Err(AppError::UnsupportedFormat {
            message: "empty or unsupported filename".to_owned(),
        });
    }

    let path_obj = Path::new(&normalized);
    let raw_ext = path_obj
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .filter(|e| e.len() <= 10 && e.chars().all(|c| c.is_ascii_alphanumeric()))
        .unwrap_or_else(|| "bin".to_owned());

    // Preserve up to 2 relative path segments (e.g. `Series__Chapter__page`) for folder uploads
    // encoded safely inside the staging filename so directory grouping survives WebView upload.
    let raw_stem = path_obj
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Comic")
        .trim();
    let folder_prefix = path_obj
        .parent()
        .and_then(|p| p.to_str())
        .map(str::trim)
        .filter(|s| !s.is_empty() && *s != ".")
        .map(|parent| {
            parent
                .split('/')
                .filter(|seg| !seg.is_empty() && *seg != "." && *seg != "..")
                .map(sanitize_filename_segment)
                .collect::<Vec<_>>()
                .join("~")
        })
        .filter(|s| !s.is_empty());

    let clean_stem = sanitize_filename_segment(raw_stem);
    let stem_token = if clean_stem.is_empty() {
        "Comic".to_owned()
    } else {
        clean_stem
    };

    let nonce: u64 = rand::random();
    let upload_id = match folder_prefix {
        Some(dir_tag) => format!("stg_{nonce:016x}__dir_{dir_tag}__{stem_token}.{raw_ext}"),
        None => format!("stg_{nonce:016x}__{stem_token}.{raw_ext}"),
    };

    let staging_dir = data_dir.join("comics").join(".staging");
    tokio::fs::create_dir_all(&staging_dir)
        .await
        .map_err(|e| AppError::Internal {
            message: format!("failed to prepare comic staging directory: {e}"),
        })?;
    let dest = staging_dir.join(&upload_id);
    let _ = tokio::fs::File::create(&dest)
        .await
        .map_err(|e| AppError::Internal {
            message: format!("failed to create comic staging file: {e}"),
        })?;
    Ok(upload_id)
}

/// Appends a base64-encoded byte chunk to an active staging file.
///
/// # Errors
///
/// Returns [`AppError`] on invalid `upload_id`, invalid base64, or size overflow.
pub(crate) async fn append_comic_staging_chunk(
    data_dir: &Path,
    upload_id: &str,
    chunk_b64: &str,
) -> Result<(), AppError> {
    validate_staging_id(upload_id)?;
    let bytes = BASE64_STANDARD
        .decode(chunk_b64.trim())
        .map_err(|e| AppError::CorruptedData {
            message: format!("invalid base64 upload chunk: {e}"),
        })?;
    let path = data_dir.join("comics").join(".staging").join(upload_id);
    let meta = tokio::fs::metadata(&path)
        .await
        .map_err(|e| AppError::NotFound {
            message: format!("comic staging session not found: {e}"),
        })?;
    let next_len = meta
        .len()
        .saturating_add(u64::try_from(bytes.len()).unwrap_or(u64::MAX));
    if next_len > MAX_COMIC_ARCHIVE_BYTES {
        let _ = tokio::fs::remove_file(&path).await;
        return Err(AppError::CorruptedData {
            message: "uploaded file exceeds maximum permitted 1.5 GiB limit".to_owned(),
        });
    }
    let mut file = tokio::fs::OpenOptions::new()
        .append(true)
        .open(&path)
        .await
        .map_err(|e| AppError::Internal {
            message: format!("failed to open comic staging file: {e}"),
        })?;
    file.write_all(&bytes)
        .await
        .map_err(|e| AppError::Internal {
            message: format!("failed to write comic staging chunk: {e}"),
        })?;
    Ok(())
}

/// Finalizes one or more staged uploads (archives, PDFs, EPUBs, loose images, or folder uploads),
/// imports them into `<data_dir>/comics/`, and cleans up `.staging/`.
///
/// # Errors
///
/// Returns [`AppError`] if any staged file is invalid or import fails.
pub(crate) async fn finish_comic_staging_import(
    data_dir: &Path,
    state: &AppState,
    upload_ids: &[String],
    target_manga_id: Option<i64>,
) -> Result<MangaView, AppError> {
    let staging_dir = data_dir.join("comics").join(".staging");
    let mut staged_paths = Vec::with_capacity(upload_ids.len());
    let mut virtual_folder_dir: Option<PathBuf> = None;

    for uid in upload_ids {
        validate_staging_id(uid)?;
        let src = staging_dir.join(uid);
        if let Some((dir_segments, leaf_name)) = parse_staged_folder_segments(uid) {
            let root = virtual_folder_dir.get_or_insert_with(|| {
                let batch_nonce: u64 = rand::random();
                staging_dir.join(format!("batch_{batch_nonce:016x}"))
            });
            let mut target_dir = root.clone();
            for seg in &dir_segments {
                target_dir = target_dir.join(seg);
            }
            tokio::fs::create_dir_all(&target_dir)
                .await
                .map_err(|e| AppError::Internal {
                    message: format!("failed to reconstruct staged folder layout: {e}"),
                })?;
            let dst = target_dir.join(leaf_name);
            tokio::fs::rename(&src, &dst)
                .await
                .map_err(|e| AppError::Internal {
                    message: format!("failed to move staged folder file: {e}"),
                })?;
        } else {
            staged_paths.push(src);
        }
    }

    let mut import_inputs = staged_paths.clone();
    if let Some(ref vdir) = virtual_folder_dir {
        // If every staged folder file lived under one shared top-level folder, pass that folder
        // directly so series/chapter names infer cleanly from the user's folder name.
        let mut top_entries = Vec::new();
        if let Ok(mut rd) = tokio::fs::read_dir(vdir).await {
            while let Ok(Some(ent)) = rd.next_entry().await {
                top_entries.push(ent.path());
            }
        }
        if top_entries.len() == 1 && top_entries[0].is_dir() {
            import_inputs.push(top_entries[0].clone());
        } else {
            import_inputs.push(vdir.clone());
        }
    }

    let res = import_comic_paths(data_dir, state, &import_inputs, target_manga_id).await;
    for p in &staged_paths {
        let _ = tokio::fs::remove_file(p).await;
    }
    if let Some(vdir) = virtual_folder_dir {
        let _ = tokio::fs::remove_dir_all(vdir).await;
    }
    res
}

fn validate_staging_id(upload_id: &str) -> Result<(), AppError> {
    if !upload_id.starts_with("stg_")
        || upload_id.contains("..")
        || upload_id.contains('/')
        || upload_id.contains('\\')
        || upload_id.len() > 240
    {
        return Err(AppError::InvalidUrl {
            message: "invalid comic staging identifier".to_owned(),
        });
    }
    Ok(())
}

fn sanitize_filename_segment(raw: &str) -> String {
    raw.chars()
        .map(|c| {
            if c.is_alphanumeric() || matches!(c, ' ' | '-' | '_' | '.' | '(' | ')' | '#') {
                c
            } else {
                '_'
            }
        })
        .take(80)
        .collect::<String>()
        .trim()
        .to_owned()
}

fn parse_staged_folder_segments(upload_id: &str) -> Option<(Vec<String>, String)> {
    let rest = upload_id.strip_prefix("stg_")?;
    let (hex_part, after_hex) = rest.split_once("__")?;
    if hex_part.len() != 16 {
        return None;
    }
    let after_dir = after_hex.strip_prefix("dir_")?;
    let (encoded_dirs, leaf) = after_dir.split_once("__")?;
    let segs: Vec<String> = encoded_dirs
        .split('~')
        .map(str::trim)
        .filter(|s| !s.is_empty() && !s.contains(".."))
        .take(2)
        .map(str::to_owned)
        .collect();
    if segs.is_empty() || leaf.trim().is_empty() {
        return None;
    }
    Some((segs, leaf.trim().to_owned()))
}

/// Imports one or more user-selected files and/or directories (`.cbz`, `.zip`, `.cbr`, `.rar`,
/// `.cb7`, `.7z`, `.cbt`, `.tar`, `.epub`, `.pdf`, image folders, or loose raster images) into
/// `<data_dir>/comics/`, grouping chapters by series and returning the resulting [`MangaView`].
///
/// # Errors
///
/// Returns [`AppError`] if no valid comic inputs are provided, a file is encrypted/corrupted/unsupported,
/// or SQLite/disk operations fail.
pub(crate) async fn import_comic_paths(
    data_dir: &Path,
    state: &AppState,
    paths: &[PathBuf],
    target_manga_id: Option<i64>,
) -> Result<MangaView, AppError> {
    if paths.is_empty() {
        return Err(AppError::UnsupportedFormat {
            message: "no comic files or folders selected".to_owned(),
        });
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

    let paths_owned = paths.to_vec();
    let pinned_clone = pinned_series_title.clone();
    let prepared_chapters = tokio::task::spawn_blocking(move || {
        prepare_import_chapters_sync(&paths_owned, pinned_clone.as_deref())
    })
    .await
    .map_err(|e| AppError::Internal {
        message: format!("comic parser worker failed: {e}"),
    })??;

    let mut groups: BTreeMap<String, Vec<PreparedChapter>> = BTreeMap::new();
    for item in prepared_chapters {
        let slug = slugify_series(&item.parsed.series_title);
        groups.entry(slug).or_default().push(item);
    }

    let comics_root = data_dir.join("comics");
    tokio::fs::create_dir_all(&comics_root)
        .await
        .map_err(|e| AppError::Internal {
            message: format!("failed to create internal comic storage dir: {e}"),
        })?;

    let max_mb = state.current_settings().await.cache.max_mb;
    let mut last_manga_id: Option<i64> = None;

    for (series_slug, mut items) in groups {
        items.sort_by(|a, b| compare_parsed_chapters(&a.parsed, &b.parsed));
        let manga_id = persist_series_group(
            data_dir,
            &comics_root,
            state,
            &series_slug,
            items,
            target_manga_id,
            max_mb,
        )
        .await?;
        last_manga_id = Some(manga_id);
    }

    let final_id = last_manga_id.ok_or_else(|| AppError::UnsupportedFormat {
        message: "no readable comic series could be imported".to_owned(),
    })?;
    get_manga(state, final_id, FetchMode::LocalOnly).await
}

#[expect(
    clippy::too_many_lines,
    reason = "expands directories, archive files, and loose image batches into prepared chapters"
)]
fn prepare_import_chapters_sync(
    paths: &[PathBuf],
    pinned_series: Option<&str>,
) -> Result<Vec<PreparedChapter>, AppError> {
    let mut prepared: Vec<PreparedChapter> = Vec::new();
    let mut loose_images: Vec<PathBuf> = Vec::new();

    for path in paths {
        let detected = detect_format_magic(path)?;
        match detected {
            DetectedFileKind::Directory => {
                let units = expand_directory_sync(path, pinned_series)?;
                for unit in units {
                    match unit {
                        DirectoryImportUnit::ArchiveFile {
                            path: arc_path,
                            series_override,
                            series_meta,
                        } => {
                            let effective_override = pinned_series.or(series_override.as_deref());
                            let mut parsed =
                                inspect_single_archive_sync(&arc_path, effective_override)?;
                            if let Some(sm) = series_meta {
                                apply_series_fallback_meta(&mut parsed, &sm);
                            }
                            prepared.push(PreparedChapter {
                                parsed,
                                payload: PreparedChapterPayload::ArchiveCopy(arc_path),
                            });
                        }
                        DirectoryImportUnit::ImageBatch {
                            image_paths,
                            series_title,
                            chapter_hint,
                            chapter_meta,
                        } => {
                            let effective_series =
                                pinned_series.unwrap_or(series_title.as_str()).to_owned();
                            let (parsed, copy_plan) = inspect_loose_image_batch_sync(
                                image_paths,
                                &effective_series,
                                &chapter_hint,
                                chapter_meta,
                            )?;
                            prepared.push(PreparedChapter {
                                parsed,
                                payload: PreparedChapterPayload::ImageCopies(copy_plan),
                            });
                        }
                    }
                }
            }
            DetectedFileKind::Image => {
                loose_images.push(path.clone());
            }
            DetectedFileKind::Tachibk => {
                return Err(AppError::UnsupportedFormat {
                    message: "Mihon/Tachiyomi backup (.tachibk/.proto.gz) must be imported via the backup preview modal".to_owned(),
                });
            }
            DetectedFileKind::Zip
            | DetectedFileKind::Epub
            | DetectedFileKind::Rar
            | DetectedFileKind::SevenZ
            | DetectedFileKind::Tar
            | DetectedFileKind::Pdf => {
                let parsed = inspect_single_archive_sync(path, pinned_series)?;
                prepared.push(PreparedChapter {
                    parsed,
                    payload: PreparedChapterPayload::ArchiveCopy(path.clone()),
                });
            }
        }
    }

    if !loose_images.is_empty() {
        let inferred_series = pinned_series.map_or_else(
            || {
                loose_images[0]
                    .parent()
                    .and_then(|p| p.file_name())
                    .and_then(|s| s.to_str())
                    .filter(|s| !s.is_empty() && !s.starts_with('.'))
                    .map_or_else(
                        || {
                            let raw = loose_images[0]
                                .file_stem()
                                .and_then(|s| s.to_str())
                                .unwrap_or("Local Images");
                            strip_staging_prefix(raw).to_owned()
                        },
                        str::to_owned,
                    )
            },
            str::to_owned,
        );
        let chapter_hint = if loose_images.len() == 1 {
            let raw = loose_images[0]
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("Chapter 1");
            strip_staging_prefix(raw).to_owned()
        } else {
            inferred_series.clone()
        };
        let (parsed, copy_plan) = inspect_loose_image_batch_sync(
            loose_images,
            &inferred_series,
            &chapter_hint,
            ComicMetadata::default(),
        )?;
        prepared.push(PreparedChapter {
            parsed,
            payload: PreparedChapterPayload::ImageCopies(copy_plan),
        });
    }

    if prepared.is_empty() {
        return Err(AppError::UnsupportedFormat {
            message: "no supported comic archives, documents, or images found".to_owned(),
        });
    }

    Ok(prepared)
}

fn apply_series_fallback_meta(parsed: &mut ParsedComicChapter, series_meta: &ComicMetadata) {
    let base = ComicMetadata {
        series: Some(parsed.series_title.clone()),
        title: Some(parsed.chapter_title.clone()),
        number: parsed.chapter_number,
        volume: parsed.volume.clone(),
        summary: parsed.summary.clone(),
        authors: parsed.authors.clone(),
        artists: parsed.artists.clone(),
        genres: parsed.genres.clone(),
        language: parsed.language.clone(),
    };
    let merged = merge_metadata(Some(series_meta), Some(&base));
    parsed.summary = merged.summary;
    parsed.authors = merged.authors;
    parsed.artists = merged.artists;
    parsed.genres = merged.genres;
    parsed.language = merged.language;
}

#[expect(
    clippy::too_many_lines,
    reason = "copies imported chapters, merges existing chapters, caches cover, and seeds history"
)]
async fn persist_series_group(
    data_dir: &Path,
    comics_root: &Path,
    state: &AppState,
    series_slug: &str,
    items: Vec<PreparedChapter>,
    target_manga_id: Option<i64>,
    max_mb: u32,
) -> Result<i64, AppError> {
    let series_dir = comics_root.join(series_slug);
    tokio::fs::create_dir_all(&series_dir)
        .await
        .map_err(|e| AppError::Internal {
            message: format!("failed to create comic series folder: {e}"),
        })?;

    let first = &items[0].parsed;
    let source_id = format!("local:{series_slug}");
    let legacy_source_id = format!("cbz:{series_slug}");
    let mut existing_id = if let Some(tid) = target_manga_id {
        Some(tid)
    } else if let Some(id) = state.db.find_manga_id_by_source_id(&source_id).await? {
        Some(id)
    } else {
        state
            .db
            .find_manga_id_by_source_id(&legacy_source_id)
            .await?
    };

    if existing_id.is_none() {
        let norm_title =
            crate::services::comics::tachibk::normalize_match_title(&first.series_title);
        if !norm_title.is_empty() {
            existing_id = state
                .db
                .list_manga_match_candidates()
                .await?
                .into_iter()
                .find(|c| {
                    c.site == crate::services::comics::tachibk::TACHIBK_SITE_KEY
                        && crate::services::comics::tachibk::normalize_match_title(&c.title)
                            == norm_title
                })
                .map(|c| c.id);
        }
    }

    // Preserve existing local manga URL/site if appending chapters to a pre-existing entry.
    // If upgrading a metadata-only `.tachibk` catalog entry, graduate its site to `local:comic`.
    let mut upgraded_from_tachibk = false;
    let (series_url, cover_url, site_key) = if let Some(eid) = existing_id
        && let Some(row) = state.db.get_manga_by_id(eid).await?
    {
        upgraded_from_tachibk = row.site == crate::services::comics::tachibk::TACHIBK_SITE_KEY;
        let u = Url::parse(&row.url).unwrap_or_else(|_| {
            Url::parse(&format!("https://comic.local/series/{series_slug}"))
                .expect("static local series url is valid")
        });
        let c = row
            .cover_url
            .as_deref()
            .and_then(|s| Url::parse(s).ok())
            .unwrap_or_else(|| {
                Url::parse(&format!("https://comic.local/cover/{series_slug}.img"))
                    .expect("static local cover url is valid")
            });
        let effective_site = if upgraded_from_tachibk {
            "local:comic".to_owned()
        } else {
            row.site
        };
        (u, c, effective_site)
    } else {
        let u = Url::parse(&format!("https://comic.local/series/{series_slug}"))
            .expect("static local series url is valid");
        let c = Url::parse(&format!("https://comic.local/cover/{series_slug}.img"))
            .expect("static local cover url is valid");
        (u, c, "local:comic".to_owned())
    };

    let existing_chapter_rows = if let Some(eid) = existing_id {
        state.db.list_chapters(eid).await?
    } else {
        Vec::new()
    };
    let (tachibk_read_nums, tachibk_read_names) = if upgraded_from_tachibk {
        crate::services::comics::tachibk::extract_tachibk_read_lookup(&existing_chapter_rows)
    } else {
        (BTreeMap::new(), std::collections::HashSet::new())
    };

    let mut merged_chapters: Vec<ChapterRef> = existing_chapter_rows
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
        .collect();

    let mut chapter_records: Vec<(String, String, String, String, i64)> =
        Vec::with_capacity(items.len());
    let mut restored_read_urls: Vec<String> = Vec::new();
    let mut replaced_tachibk_urls: Vec<(String, String, String)> = Vec::new();

    for item in &items {
        let parsed = &item.parsed;
        let rel_path = match &item.payload {
            PreparedChapterPayload::ArchiveCopy(src_path) => {
                let ext = parsed.format.storage_extension();
                let dest_filename = format!("{}.{ext}", parsed.file_sha_short);
                let dest_path = series_dir.join(&dest_filename);
                if src_path != &dest_path {
                    tokio::fs::copy(src_path, &dest_path).await.map_err(|e| {
                        AppError::Internal {
                            message: format!("failed to copy comic archive into app storage: {e}"),
                        }
                    })?;
                }
                format!("{series_slug}/{dest_filename}")
            }
            PreparedChapterPayload::ImageCopies(copy_plan) => {
                let chapter_dir = series_dir.join(&parsed.file_sha_short);
                tokio::fs::create_dir_all(&chapter_dir)
                    .await
                    .map_err(|e| AppError::Internal {
                        message: format!("failed to create comic chapter image folder: {e}"),
                    })?;
                for (src_img, rel_name) in copy_plan {
                    let dst_img = chapter_dir.join(rel_name);
                    if src_img != &dst_img {
                        tokio::fs::copy(src_img, &dst_img).await.map_err(|e| {
                            AppError::Internal {
                                message: format!("failed to copy comic page image: {e}"),
                            }
                        })?;
                    }
                }
                format!("{series_slug}/{}", parsed.file_sha_short)
            }
        };

        let chap_url_str = format!(
            "https://comic.local/chapter/{series_slug}/{}",
            parsed.file_sha_short
        );
        let chap_url = Url::parse(&chap_url_str).expect("valid local chapter url");

        // When upgrading a metadata-only `.tachibk` catalog entry, replace any matching
        // `https://tachibk.local/chapter/...` stub with this real local comic chapter.
        if upgraded_from_tachibk {
            let norm_new_name =
                crate::services::comics::tachibk::normalize_match_title(&parsed.chapter_title);
            let matched_stub_pos = merged_chapters.iter().position(|c| {
                if !crate::services::comics::is_tachibk_url(c.url.as_str()) {
                    return false;
                }
                if let (Some(na), Some(nb)) = (c.number, parsed.chapter_number)
                    && (na - nb).abs() < 0.001
                {
                    return true;
                }
                !norm_new_name.is_empty()
                    && crate::services::comics::tachibk::normalize_match_title(&c.name)
                        == norm_new_name
            });
            if let Some(pos) = matched_stub_pos {
                let removed = merged_chapters.remove(pos);
                replaced_tachibk_urls.push((
                    removed.url.into(),
                    chap_url_str.clone(),
                    parsed.chapter_title.clone(),
                ));
            }
            if crate::services::comics::tachibk::matches_tachibk_read_lookup(
                parsed.chapter_number,
                &parsed.chapter_title,
                &tachibk_read_nums,
                &tachibk_read_names,
            ) {
                restored_read_urls.push(chap_url_str.clone());
            }
        }

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
                language: parsed.language.clone(),
                source_id: Some(parsed.file_sha_short.clone()),
            });
        }

        let pages_json = serde_json::to_string(&parsed.pages).map_err(|e| AppError::Internal {
            message: format!("failed to serialize comic page index: {e}"),
        })?;
        let fsize = i64::try_from(parsed.file_size).unwrap_or(i64::MAX);
        chapter_records.push((
            chap_url_str,
            parsed.format.as_storage_str().to_owned(),
            rel_path,
            pages_json,
            fsize,
        ));
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
        SiteId::known(site_key.clone()),
        first.series_title.clone(),
        Some(cover_url.clone()),
        merged_chapters,
    );
    manga.source_id = Some(source_id);
    manga.description.clone_from(&first.summary);
    manga.authors.clone_from(&first.authors);
    manga.artists.clone_from(&first.artists);
    manga.genres.clone_from(&first.genres);
    manga.language.clone_from(&first.language);

    let manga_id = state.db.upsert_manga_with_id(existing_id, &manga).await?;

    if crate::services::comics::is_local_comic_url(cover_url.as_str()) {
        let cover_key = ImageCache::key_for_url(cover_url.as_str());
        state
            .images
            .put_cover(
                &cover_key,
                &first.first_page_bytes,
                &first.first_page_mime,
                CacheEntryMeta {
                    site: Some(&site_key),
                    manga_id: Some(manga_id),
                    is_cover: true,
                    chapter_url: None,
                },
            )
            .await;
    } else if !crate::services::comics::is_tachibk_url(cover_url.as_str()) {
        state.images.spawn_cover_prefetch(
            state.registry.http().clone(),
            cover_url.clone(),
            series_url.clone(),
            Some(site_key.clone()),
            Some(manga_id),
            max_mb,
        );
    }

    let first_chapter_probe = chapter_records
        .first()
        .map(|(ch_url, fmt, _, _, _)| (ch_url.clone(), ComicFormat::from_storage_str(fmt)));

    for (chap_url_str, format_str, rel_path, pages_json, fsize) in chapter_records {
        state
            .db
            .upsert_local_comic_chapter(
                manga_id,
                &chap_url_str,
                &format_str,
                &rel_path,
                &pages_json,
                fsize,
            )
            .await?;
    }

    // Probe the first 4–6 page headers of chapter 1 and persist Manga vs. Webtoon layout
    // classification immediately on import (reusing page 0 bytes already extracted above).
    let mut sampled_pages: Vec<Vec<u8>> =
        Vec::with_capacity(crate::services::layout_detect::MAX_SNIFF_PAGES);
    if !first.first_page_bytes.is_empty() {
        sampled_pages.push(first.first_page_bytes.to_vec());
    }
    if let Some((first_ch_url, first_fmt)) = first_chapter_probe {
        let max_pages = match first_fmt {
            ComicFormat::Pdf => 3,
            _ => crate::services::layout_detect::MAX_SNIFF_PAGES,
        };
        let encoded_ch: String =
            url::form_urlencoded::byte_serialize(first_ch_url.as_bytes()).collect();
        let target_count = first.pages.len().min(max_pages);
        for page_idx in 1..target_count {
            if let Ok(page_url) = Url::parse(&format!(
                "https://comic.local/page/{manga_id}/{page_idx}?ch={encoded_ch}"
            )) && let Ok((bytes, _)) = read_local_page_bytes(data_dir, state, &page_url).await
            {
                sampled_pages.push(bytes);
            }
        }
    }
    let _ = crate::services::layout_detect::classify_manga_on_import(
        state,
        manga_id,
        &manga.genres,
        &manga.tags,
        &site_key,
        &sampled_pages,
    )
    .await;

    for read_url in restored_read_urls {
        state.db.mark_chapter_read(manga_id, &read_url).await?;
    }

    if !replaced_tachibk_urls.is_empty()
        && let Some(hist) = state.db.get_history_for_manga(manga_id).await?
        && let Some((_, new_url, new_name)) = replaced_tachibk_urls.iter().find(|(old_u, _, _)| {
            old_u.trim_end_matches('/') == hist.chapter_url.trim_end_matches('/')
        })
    {
        state
            .db
            .save_history(
                manga_id,
                new_url,
                new_name,
                hist.scroll_ratio,
                hist.page_index,
            )
            .await?;
    }

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

/// Resolves a local `https://comic.local/chapter/...` (or legacy `https://cbz.local/chapter/...`)
/// URL into a [`mangacore::Chapter`] backed by indexed page URLs in `< 1ms`.
///
/// # Errors
///
/// Returns [`AppError::NotFound`] or [`AppError::CorruptedData`] if the local chapter record is missing.
pub(crate) async fn load_local_chapter(
    state: &AppState,
    chapter_url: &Url,
) -> Result<(Chapter, i64), AppError> {
    let (manga_id, _format_str, _rel_path, pages_json) = state
        .db
        .get_local_comic_chapter_by_url(chapter_url.as_str())
        .await?
        .ok_or_else(|| AppError::NotFound {
            message: "local comic chapter not found in storage".to_owned(),
        })?;

    let pages = deserialize_page_descriptors(&pages_json)?;
    if pages.is_empty() {
        return Err(AppError::CorruptedData {
            message: "local comic chapter contains zero pages".to_owned(),
        });
    }

    let encoded_ch: String =
        url::form_urlencoded::byte_serialize(chapter_url.as_str().as_bytes()).collect();
    let images: Vec<PageImage> = (0..pages.len())
        .map(|idx| {
            let page_url = Url::parse(&format!(
                "https://comic.local/page/{manga_id}/{idx}?ch={encoded_ch}"
            ))
            .expect("valid local comic page url");
            PageImage {
                url: page_url,
                fallbacks: Vec::new(),
                referer: chapter_url.clone(),
            }
        })
        .collect();

    let chapters = state.db.list_chapters(manga_id).await?;
    let pos = chapters.iter().position(|c| c.url == chapter_url.as_str());
    let name = pos.map_or_else(|| "Local Chapter".to_owned(), |i| chapters[i].name.clone());
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

/// Lazily reads, extracts, or renders a single local comic page
/// (`https://comic.local/page/<manga_id>/<idx>?ch=...` or legacy `https://cbz.local/page/...`).
///
/// - O(1) seek formats (`Zip`, `Epub`, `Tar`, `Folder`) read straight from disk in `< 1ms`.
/// - CPU-intensive/compressed formats (`Pdf`, `Rar`, `SevenZ`) decode on demand and cache each
///   rendered/extracted page transiently in `state.images` (`is_cover = false`, auto-purged on
///   chapter exit and app close) so scrolling back/forth is instant without holding archives in RAM.
///
/// # Errors
///
/// Returns [`AppError`] if the stored source is missing, encrypted, or corrupted.
#[expect(
    clippy::too_many_lines,
    reason = "dispatches lazy page decoding across ZIP, TAR, RAR, 7z, PDF, and local image formats"
)]
pub(crate) async fn read_local_page_bytes(
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
            message: "malformed local comic page URL".to_owned(),
        });
    };
    let manga_id = manga_id_str
        .parse::<i64>()
        .map_err(|_| AppError::InvalidUrl {
            message: "invalid local comic manga id".to_owned(),
        })?;
    let page_idx = idx_str.parse::<usize>().map_err(|_| AppError::InvalidUrl {
        message: "invalid local comic page index".to_owned(),
    })?;
    let chapter_url = page_url
        .query_pairs()
        .find(|(k, _)| k == "ch")
        .map(|(_, v)| v.into_owned())
        .ok_or_else(|| AppError::InvalidUrl {
            message: "missing local comic chapter query param".to_owned(),
        })?;

    let (_mid, format_str, rel_path, pages_json) = state
        .db
        .get_local_comic_chapter_by_url(&chapter_url)
        .await?
        .ok_or_else(|| AppError::NotFound {
            message: "local comic chapter entry missing".to_owned(),
        })?;
    if rel_path.contains("..") {
        return Err(AppError::InvalidUrl {
            message: "disallowed local comic relative path".to_owned(),
        });
    }

    let pages = deserialize_page_descriptors(&pages_json)?;
    let descriptor = pages
        .get(page_idx)
        .cloned()
        .ok_or_else(|| AppError::NotFound {
            message: format!("comic page index {page_idx} out of range"),
        })?;

    let format = ComicFormat::from_storage_str(&format_str);
    let source_path = resolve_local_storage_path(data_dir, &rel_path).await?;

    let use_transient_cache = matches!(
        format,
        ComicFormat::Pdf | ComicFormat::Rar | ComicFormat::SevenZ
    );
    let cache_key = ImageCache::key_for_url(page_url.as_str());
    let cache_meta = CacheEntryMeta {
        site: Some("local:comic"),
        manga_id: Some(manga_id),
        is_cover: false,
        chapter_url: Some(&chapter_url),
    };

    if use_transient_cache && let Some(hit) = state.images.get(&cache_key, cache_meta.clone()).await
    {
        return Ok((hit.bytes.to_vec(), hit.content_type));
    }

    let (bytes, mime) = match descriptor {
        PageDescriptor::ZipEntry(entry) => read_zip_entry_async(&source_path, entry).await?,
        PageDescriptor::TarEntry(entry) => read_tar_entry_async(&source_path, entry).await?,
        PageDescriptor::LocalFile(entry) => {
            read_local_image_async(&source_path, &entry.rel_name).await?
        }
        PageDescriptor::PdfPage(entry) => {
            let p = source_path.clone();
            tokio::task::spawn_blocking(move || render_pdf_page_sync(&p, entry.page_index))
                .await
                .map_err(|e| AppError::Internal {
                    message: format!("PDF render worker failed: {e}"),
                })??
        }
        PageDescriptor::ArchiveEntry(entry) => {
            let p = source_path.clone();
            match format {
                ComicFormat::Rar => {
                    tokio::task::spawn_blocking(move || read_rar_entry_sync(&p, entry.index))
                        .await
                        .map_err(|e| AppError::Internal {
                            message: format!("RAR decode worker failed: {e}"),
                        })??
                }
                ComicFormat::SevenZ => {
                    tokio::task::spawn_blocking(move || read_sevenz_entry_sync(&p, &entry.name))
                        .await
                        .map_err(|e| AppError::Internal {
                            message: format!("7z decode worker failed: {e}"),
                        })??
                }
                _ => {
                    return Err(AppError::CorruptedData {
                        message: format!(
                            "unexpected archive descriptor for format '{}'",
                            format.as_storage_str()
                        ),
                    });
                }
            }
        }
    };

    if use_transient_cache {
        let max_mb = state.current_settings().await.cache.max_mb;
        let cached_bytes = bytes::Bytes::copy_from_slice(&bytes);
        state
            .images
            .put(&cache_key, &cached_bytes, &mime, max_mb, cache_meta)
            .await;
    }

    Ok((bytes, mime))
}

async fn resolve_local_storage_path(data_dir: &Path, rel_path: &str) -> Result<PathBuf, AppError> {
    let comics_candidate = data_dir.join("comics").join(rel_path);
    if tokio::fs::try_exists(&comics_candidate)
        .await
        .unwrap_or(false)
    {
        return Ok(comics_candidate);
    }
    let legacy_cbz_candidate = data_dir.join("cbz").join(rel_path);
    if tokio::fs::try_exists(&legacy_cbz_candidate)
        .await
        .unwrap_or(false)
    {
        return Ok(legacy_cbz_candidate);
    }
    Err(AppError::NotFound {
        message: "local comic file or folder is missing on disk".to_owned(),
    })
}

/// Removes all stored local comic files/folders associated with `manga_id` from both
/// `<data_dir>/comics/` and legacy `<data_dir>/cbz/` on permanent manga deletion.
pub(crate) async fn delete_local_files_for_manga(data_dir: &Path, state: &AppState, manga_id: i64) {
    let Ok(rel_paths) = state
        .db
        .list_local_comic_rel_paths_for_manga(manga_id)
        .await
    else {
        return;
    };

    for root_name in ["comics", "cbz"] {
        let root = data_dir.join(root_name);
        let mut parent_dirs = Vec::new();
        for rel in &rel_paths {
            if rel.contains("..") {
                continue;
            }
            let full = root.join(rel);
            evict_cached_pdfs_for_prefix(&full);
            evict_cached_rars_for_prefix(&full);
            if let Some(p) = full.parent() {
                parent_dirs.push(p.to_path_buf());
            }
            if let Ok(meta) = tokio::fs::metadata(&full).await {
                if meta.is_dir() {
                    let _ = tokio::fs::remove_dir_all(&full).await;
                } else {
                    let _ = tokio::fs::remove_file(&full).await;
                }
            }
        }
        for dir in parent_dirs {
            if dir != root {
                let _ = tokio::fs::remove_dir(&dir).await;
            }
        }
    }
}

fn compare_parsed_chapters(a: &ParsedComicChapter, b: &ParsedComicChapter) -> Ordering {
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

#[cfg(target_os = "windows")]
fn open_win32_comic_file_dialog() -> Vec<PathBuf> {
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

    let filter: Vec<u16> = concat!(
        "All Supported Comics & Images (*.cbz;*.cbr;*.cb7;*.cbt;*.zip;*.rar;*.7z;*.tar;*.epub;*.pdf;*.jpg;*.jpeg;*.png;*.webp;*.avif)\0",
        "*.cbz;*.cbr;*.cb7;*.cbt;*.zip;*.rar;*.7z;*.tar;*.epub;*.pdf;*.jpg;*.jpeg;*.png;*.webp;*.avif;*.gif;*.bmp\0",
        "Comic Archives (*.cbz;*.cbr;*.cb7;*.cbt;*.zip;*.rar;*.7z;*.tar)\0",
        "*.cbz;*.cbr;*.cb7;*.cbt;*.zip;*.rar;*.7z;*.tar\0",
        "Ebooks & PDF (*.epub;*.pdf)\0*.epub;*.pdf\0",
        "Images (*.jpg;*.jpeg;*.png;*.webp;*.avif;*.gif;*.bmp)\0*.jpg;*.jpeg;*.png;*.webp;*.avif;*.gif;*.bmp\0\0"
    )
    .encode_utf16()
    .collect();
    let title: Vec<u16> = "Select Comic File(s) or Image(s)\0"
        .encode_utf16()
        .collect();
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
pub(crate) fn parse_multiselect_utf16_buffer(buf: &[u16]) -> Vec<PathBuf> {
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
