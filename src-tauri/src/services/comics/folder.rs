//! Directory, multi-chapter series folder (Mihon / Suwayomi / Kotatsu / `HakuNeko` layouts),
//! and loose multi-file image importer with O(1) direct file page reads.
//!
//! Privacy guarantee: ONLY inspects the exact directories or files explicitly chosen or dropped
//! by the user; never traverses parent folders or scans the device automatically.

use std::path::{Path, PathBuf};

use bytes::Bytes;
use mangacore::inspect_image_bytes;
use serde::{Deserialize, Serialize};

use crate::error::AppError;
use crate::services::comics::sort::{
    ComicMetadata, MAX_COMIC_PAGE_BYTES, MAX_METADATA_BYTES, MAX_PAGES_PER_CHAPTER,
    compute_images_fingerprint, is_junk_entry, is_supported_image_ext, natural_cmp,
    parse_comic_info_xml, parse_mihon_details_json,
};
use crate::services::comics::zip_epub::build_parsed_chapter;
use crate::services::comics::{
    ComicFormat, DetectedFileKind, PageDescriptor, ParsedComicChapter, detect_format_magic,
};

/// Indexed relative image filename inside an imported chapter folder under `<data_dir>/comics/`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LocalImageEntry {
    /// Safe filename relative to the stored chapter directory (no path separators or `..`).
    pub rel_name: String,
}

/// Discovered import unit produced when expanding a user-selected directory or loose image set.
#[derive(Debug)]
pub(crate) enum DirectoryImportUnit {
    /// A child comic archive or document file (`.cbz`, `.cbr`, `.cb7`, `.cbt`, `.epub`, `.pdf`).
    ArchiveFile {
        path: PathBuf,
        series_override: Option<String>,
        series_meta: Option<ComicMetadata>,
    },
    /// A naturally ordered chapter of loose image files.
    ImageBatch {
        image_paths: Vec<PathBuf>,
        series_title: String,
        chapter_hint: String,
        chapter_meta: ComicMetadata,
    },
}

/// Expands a single user-selected directory (max 2 levels: series folder -> chapter folders/archives)
/// into concrete [`DirectoryImportUnit`] items.
///
/// Supports:
/// - **Single chapter image folder** (images directly inside `dir`, optional `ComicInfo.xml`).
/// - **Series folder** (subdirectories of chapter images and/or child comic archives, optional
///   series-level `ComicInfo.xml` or Mihon/Tachiyomi `details.json`).
///
/// # Errors
///
/// Returns [`AppError`] if the directory cannot be read or contains no supported comics/images.
#[expect(
    clippy::too_many_lines,
    reason = "handles single-chapter image folders, series subfolders, and mixed archive folders"
)]
pub(crate) fn expand_directory_sync(
    dir: &Path,
    pinned_series: Option<&str>,
) -> Result<Vec<DirectoryImportUnit>, AppError> {
    let dir_name = dir
        .file_name()
        .and_then(|s| s.to_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("Local Comic")
        .to_owned();

    let series_meta = read_folder_metadata_sync(dir);
    let effective_series = pinned_series
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .or_else(|| series_meta.as_ref().and_then(|m| m.series.clone()))
        .unwrap_or_else(|| dir_name.clone());

    let mut child_files: Vec<PathBuf> = Vec::new();
    let mut child_dirs: Vec<PathBuf> = Vec::new();

    let entries = std::fs::read_dir(dir).map_err(|e| AppError::NotFound {
        message: format!("cannot read selected folder: {e}"),
    })?;

    for entry_res in entries {
        let Ok(entry) = entry_res else { continue };
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
            continue;
        };
        if is_junk_entry(name) {
            continue;
        }
        if path.is_dir() {
            child_dirs.push(path);
        } else if path.is_file() {
            child_files.push(path);
        }
    }

    child_files.sort_by(|a, b| {
        let na = a.file_name().and_then(|s| s.to_str()).unwrap_or("");
        let nb = b.file_name().and_then(|s| s.to_str()).unwrap_or("");
        natural_cmp(na, nb)
    });
    child_dirs.sort_by(|a, b| {
        let na = a.file_name().and_then(|s| s.to_str()).unwrap_or("");
        let nb = b.file_name().and_then(|s| s.to_str()).unwrap_or("");
        natural_cmp(na, nb)
    });

    let mut units: Vec<DirectoryImportUnit> = Vec::new();
    let mut direct_images: Vec<PathBuf> = Vec::new();

    for file_path in child_files {
        let lower_name = file_path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if lower_name == "comicinfo.xml" || lower_name == "details.json" {
            continue;
        }
        // Skip standalone series cover images when child chapter directories or archives exist.
        if is_cover_only_filename(&lower_name) && !child_dirs.is_empty() {
            continue;
        }

        match detect_format_magic(&file_path) {
            Ok(DetectedFileKind::Image) => direct_images.push(file_path),
            Ok(
                DetectedFileKind::Zip
                | DetectedFileKind::Epub
                | DetectedFileKind::Rar
                | DetectedFileKind::SevenZ
                | DetectedFileKind::Tar
                | DetectedFileKind::Pdf,
            ) => units.push(DirectoryImportUnit::ArchiveFile {
                path: file_path,
                series_override: Some(effective_series.clone()),
                series_meta: series_meta.clone(),
            }),
            _ => {}
        }
    }

    // If direct loose images exist inside the selected directory, import them as a chapter.
    if !direct_images.is_empty() {
        let mut meta = series_meta.clone().unwrap_or_default();
        if meta.series.is_none() {
            meta.series = Some(effective_series.clone());
        }
        units.push(DirectoryImportUnit::ImageBatch {
            image_paths: direct_images,
            series_title: effective_series.clone(),
            chapter_hint: dir_name,
            chapter_meta: meta,
        });
    }

    // Inspect immediate child subdirectories (e.g. `Series/Chapter 01/`, `Series/Chapter 02/`).
    for subdir in child_dirs {
        let sub_name = subdir
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("Chapter")
            .trim()
            .to_owned();
        let sub_meta = read_folder_metadata_sync(&subdir);

        let Ok(sub_entries) = std::fs::read_dir(&subdir) else {
            continue;
        };
        let mut sub_files: Vec<PathBuf> = Vec::new();
        for entry_res in sub_entries {
            let Ok(entry) = entry_res else { continue };
            let p = entry.path();
            if p.is_file()
                && let Some(n) = p.file_name().and_then(|s| s.to_str())
                && !is_junk_entry(n)
            {
                sub_files.push(p);
            }
        }
        sub_files.sort_by(|a, b| {
            let na = a.file_name().and_then(|s| s.to_str()).unwrap_or("");
            let nb = b.file_name().and_then(|s| s.to_str()).unwrap_or("");
            natural_cmp(na, nb)
        });

        let mut sub_images: Vec<PathBuf> = Vec::new();
        for sf in sub_files {
            let lower = sf
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();
            if lower == "comicinfo.xml" || lower == "details.json" {
                continue;
            }
            match detect_format_magic(&sf) {
                Ok(DetectedFileKind::Image) => sub_images.push(sf),
                Ok(
                    DetectedFileKind::Zip
                    | DetectedFileKind::Epub
                    | DetectedFileKind::Rar
                    | DetectedFileKind::SevenZ
                    | DetectedFileKind::Tar
                    | DetectedFileKind::Pdf,
                ) => units.push(DirectoryImportUnit::ArchiveFile {
                    path: sf,
                    series_override: Some(effective_series.clone()),
                    series_meta: series_meta.clone(),
                }),
                _ => {}
            }
        }

        if !sub_images.is_empty() {
            let mut merged = merge_metadata(series_meta.as_ref(), sub_meta.as_ref());
            merged.series = Some(effective_series.clone());
            units.push(DirectoryImportUnit::ImageBatch {
                image_paths: sub_images,
                series_title: effective_series.clone(),
                chapter_hint: sub_name,
                chapter_meta: merged,
            });
        }
    }

    if units.is_empty() {
        return Err(AppError::UnsupportedFormat {
            message: "selected folder contains no readable comic images or supported archives"
                .to_owned(),
        });
    }

    Ok(units)
}

/// Validates and indexes a naturally ordered list of loose image files into a [`ParsedComicChapter`]
/// along with the `(source_path, dest_rel_name)` copy plan.
///
/// # Errors
///
/// Returns [`AppError`] if no valid raster images exist or page 0 is unreadable.
pub(crate) fn inspect_loose_image_batch_sync(
    mut image_paths: Vec<PathBuf>,
    series_title: &str,
    chapter_hint: &str,
    chapter_meta: ComicMetadata,
) -> Result<(ParsedComicChapter, Vec<(PathBuf, String)>), AppError> {
    image_paths.sort_by(|a, b| {
        let na = a.file_name().and_then(|s| s.to_str()).unwrap_or("");
        let nb = b.file_name().and_then(|s| s.to_str()).unwrap_or("");
        natural_cmp(na, nb)
    });

    let mut valid_paths: Vec<PathBuf> = Vec::with_capacity(image_paths.len());
    for p in image_paths {
        if valid_paths.len() >= MAX_PAGES_PER_CHAPTER {
            break;
        }
        let Ok(meta) = std::fs::metadata(&p) else {
            continue;
        };
        if !meta.is_file() || meta.len() == 0 || meta.len() > MAX_COMIC_PAGE_BYTES {
            continue;
        }
        if matches!(detect_format_magic(&p), Ok(DetectedFileKind::Image)) {
            valid_paths.push(p);
        }
    }

    if valid_paths.is_empty() {
        return Err(AppError::UnsupportedFormat {
            message: "no valid raster comic images found in selection".to_owned(),
        });
    }

    let first_bytes = std::fs::read(&valid_paths[0]).map_err(|e| AppError::NotFound {
        message: format!("cannot read first comic image: {e}"),
    })?;
    let header_info = inspect_image_bytes(&first_bytes).map_err(|e| AppError::CorruptedData {
        message: format!("first selected image is corrupted or unsupported: {e:?}"),
    })?;
    let first_mime = header_info.format.mime_type().to_owned();

    let (fingerprint, total_size) = compute_images_fingerprint(&valid_paths)?;

    let mut copy_plan: Vec<(PathBuf, String)> = Vec::with_capacity(valid_paths.len());
    let mut descriptors: Vec<PageDescriptor> = Vec::with_capacity(valid_paths.len());

    for (idx, src) in valid_paths.into_iter().enumerate() {
        let ext = src
            .extension()
            .and_then(|s| s.to_str())
            .map(str::to_ascii_lowercase)
            .filter(|e| is_supported_image_ext(&format!(".{e}")))
            .unwrap_or_else(|| "img".to_owned());
        let rel_name = format!("p{idx:04}.{ext}");
        descriptors.push(PageDescriptor::LocalFile(LocalImageEntry {
            rel_name: rel_name.clone(),
        }));
        copy_plan.push((src, rel_name));
    }

    let parsed = build_parsed_chapter(
        ComicFormat::Folder,
        chapter_hint,
        Some(series_title),
        chapter_meta,
        descriptors,
        Bytes::from(first_bytes),
        first_mime,
        total_size,
        fingerprint,
    )?;

    Ok((parsed, copy_plan))
}

/// Reads a single stored loose-image page directly from `<chapter_dir>/<rel_name>` in O(1).
///
/// # Errors
///
/// Returns [`AppError`] on invalid relative filename, missing file, oversized image, or invalid image bytes.
pub(crate) async fn read_local_image_async(
    chapter_dir: &Path,
    rel_name: &str,
) -> Result<(Vec<u8>, String), AppError> {
    if rel_name.is_empty()
        || rel_name.contains("..")
        || rel_name.contains('/')
        || rel_name.contains('\\')
    {
        return Err(AppError::InvalidUrl {
            message: "disallowed local page filename".to_owned(),
        });
    }

    let full_path = chapter_dir.join(rel_name);
    let meta = tokio::fs::metadata(&full_path)
        .await
        .map_err(|e| AppError::NotFound {
            message: format!("local comic image missing on disk: {e}"),
        })?;
    if meta.len() == 0 || meta.len() > MAX_COMIC_PAGE_BYTES {
        return Err(AppError::CorruptedData {
            message: "local comic image exceeds maximum 32 MiB limit".to_owned(),
        });
    }

    let bytes = tokio::fs::read(&full_path)
        .await
        .map_err(|e| AppError::NotFound {
            message: format!("failed to read local comic image: {e}"),
        })?;
    let info = inspect_image_bytes(&bytes).map_err(|e| AppError::CorruptedData {
        message: format!("corrupted local comic image: {e:?}"),
    })?;
    Ok((bytes, info.format.mime_type().to_owned()))
}

fn read_folder_metadata_sync(dir: &Path) -> Option<ComicMetadata> {
    let mut merged: Option<ComicMetadata> = None;

    let details_path = dir.join("details.json");
    if let Ok(meta) = std::fs::metadata(&details_path)
        && meta.is_file()
        && meta.len() <= MAX_METADATA_BYTES
        && let Ok(bytes) = std::fs::read(&details_path)
    {
        merged = parse_mihon_details_json(&bytes);
    }

    // Case-insensitive search for ComicInfo.xml inside `dir`
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_file()
                && let Some(name) = p.file_name().and_then(|n| n.to_str())
                && name.eq_ignore_ascii_case("comicinfo.xml")
                && let Ok(meta) = p.metadata()
                && meta.len() <= MAX_METADATA_BYTES
                && let Ok(bytes) = std::fs::read(&p)
            {
                let ci = parse_comic_info_xml(&bytes);
                merged = Some(merge_metadata(merged.as_ref(), Some(&ci)));
                break;
            }
        }
    }

    merged
}

pub(crate) fn merge_metadata(
    base: Option<&ComicMetadata>,
    override_meta: Option<&ComicMetadata>,
) -> ComicMetadata {
    let mut out = base.cloned().unwrap_or_default();
    if let Some(ov) = override_meta {
        if ov.series.is_some() {
            out.series.clone_from(&ov.series);
        }
        if ov.title.is_some() {
            out.title.clone_from(&ov.title);
        }
        if ov.number.is_some() {
            out.number = ov.number;
        }
        if ov.volume.is_some() {
            out.volume.clone_from(&ov.volume);
        }
        if ov.summary.is_some() {
            out.summary.clone_from(&ov.summary);
        }
        if !ov.authors.is_empty() {
            out.authors.clone_from(&ov.authors);
        }
        if !ov.artists.is_empty() {
            out.artists.clone_from(&ov.artists);
        }
        if !ov.genres.is_empty() {
            out.genres.clone_from(&ov.genres);
        }
        if ov.language.is_some() {
            out.language.clone_from(&ov.language);
        }
    }
    out
}

fn is_cover_only_filename(lower_name: &str) -> bool {
    let stem = lower_name.rsplit_once('.').map_or(lower_name, |(s, _)| s);
    matches!(stem, "cover" | "folder" | "poster" | "thumb" | "thumbnail")
}
