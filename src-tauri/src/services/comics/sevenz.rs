//! Pure-Rust `.cb7`/`.7z` comic archive indexer and lazy per-page reader backed by `sevenz-rust2`
//! (with C `bzip2` disabled so Android NDK cross-compilation stays 100% pure Rust).

use std::path::Path;

use bytes::Bytes;
use mangacore::inspect_image_bytes;
use sevenz_rust2::{ArchiveReader, Error as SevenZError, Password};

use crate::error::AppError;
use crate::services::comics::sort::{
    ComicMetadata, MAX_COMIC_ARCHIVE_BYTES, MAX_COMIC_PAGE_BYTES, MAX_METADATA_BYTES,
    MAX_PAGES_PER_CHAPTER, clean_stem_from_path, compute_fast_file_fingerprint, is_junk_entry,
    is_supported_image_ext, natural_cmp, parse_comic_info_xml,
};
use crate::services::comics::zip_epub::build_parsed_chapter;
use crate::services::comics::{ArchivePageEntry, ComicFormat, PageDescriptor, ParsedComicChapter};

/// Indexes a `.cb7` or `.7z` archive, extracting only `ComicInfo.xml` (if present) and cover page 0.
///
/// # Errors
///
/// Returns [`AppError::EncryptedArchive`] if password-protected, [`AppError::UnsupportedFormat`]
/// if no images are present, or [`AppError::CorruptedData`] on malformed 7z structures.
pub(crate) fn inspect_sevenz_sync(
    path: &Path,
    override_series: Option<&str>,
) -> Result<ParsedComicChapter, AppError> {
    let mut raw_file = std::fs::File::open(path).map_err(|e| AppError::NotFound {
        message: format!("cannot open 7z/CB7 archive: {e}"),
    })?;
    let file_size = raw_file
        .metadata()
        .map_err(|e| AppError::Internal {
            message: format!("cannot stat 7z/CB7 archive: {e}"),
        })?
        .len();
    if !(32..=MAX_COMIC_ARCHIVE_BYTES).contains(&file_size) {
        return Err(AppError::CorruptedData {
            message: "invalid or oversized 7z/CB7 archive".to_owned(),
        });
    }
    let file_sha_short = compute_fast_file_fingerprint(&mut raw_file, file_size)?;
    drop(raw_file);

    let mut reader = ArchiveReader::open(path, Password::empty()).map_err(map_sevenz_error)?;

    let mut comic_info_name: Option<String> = None;
    let mut pages: Vec<ArchivePageEntry> = Vec::new();

    for (idx, entry) in reader.archive().files.iter().enumerate() {
        if entry.is_directory || !entry.has_stream || entry.is_anti_item {
            continue;
        }
        let normalized_name = entry.name.replace('\\', "/");
        if is_junk_entry(&normalized_name) {
            continue;
        }
        let lower = normalized_name.to_ascii_lowercase();
        let file_part = lower.rsplit('/').next().unwrap_or(&lower);
        if file_part == "comicinfo.xml"
            && entry.size > 0
            && entry.size <= MAX_METADATA_BYTES
            && comic_info_name.is_none()
        {
            comic_info_name = Some(entry.name.clone());
            continue;
        }
        if is_supported_image_ext(file_part)
            && entry.size > 0
            && entry.size <= MAX_COMIC_PAGE_BYTES
            && pages.len() < MAX_PAGES_PER_CHAPTER
        {
            pages.push(ArchivePageEntry {
                name: entry.name.clone(),
                index: idx,
                uncompressed_size: entry.size,
            });
        }
    }

    pages.sort_by(|a, b| natural_cmp(&a.name, &b.name));
    if pages.is_empty() {
        return Err(AppError::UnsupportedFormat {
            message: "no readable comic images found inside 7z/CB7 archive".to_owned(),
        });
    }

    let comic_info = if let Some(ref xml_name) = comic_info_name {
        reader
            .read_file(xml_name)
            .ok()
            .map(|b| parse_comic_info_xml(&b))
            .unwrap_or_default()
    } else {
        ComicMetadata::default()
    };

    let first_bytes = reader.read_file(&pages[0].name).map_err(map_sevenz_error)?;
    if u64::try_from(first_bytes.len()).unwrap_or(u64::MAX) > MAX_COMIC_PAGE_BYTES {
        return Err(AppError::CorruptedData {
            message: "first page in 7z/CB7 exceeds maximum 32 MiB limit".to_owned(),
        });
    }
    let header_info = inspect_image_bytes(&first_bytes).map_err(|e| AppError::CorruptedData {
        message: format!("first page in 7z/CB7 archive is not a valid image: {e:?}"),
    })?;
    let first_mime = header_info.format.mime_type().to_owned();

    let stem = clean_stem_from_path(path, "CB7 Archive");
    build_parsed_chapter(
        ComicFormat::SevenZ,
        &stem,
        override_series,
        comic_info,
        pages
            .into_iter()
            .map(PageDescriptor::ArchiveEntry)
            .collect(),
        Bytes::from(first_bytes),
        first_mime,
        file_size,
        file_sha_short,
    )
}

/// Lazily extracts and validates a single named image page from a `.cb7`/`.7z` archive.
///
/// # Errors
///
/// Returns [`AppError`] if the archive is missing, encrypted, exceeds size limits, or corrupted.
pub(crate) fn read_sevenz_entry_sync(
    archive_path: &Path,
    entry_name: &str,
) -> Result<(Vec<u8>, String), AppError> {
    let mut reader =
        ArchiveReader::open(archive_path, Password::empty()).map_err(map_sevenz_error)?;

    // Enforce 32 MiB page size guard before decompressing (7z bomb protection).
    let declared_size = reader
        .archive()
        .files
        .iter()
        .find(|f| f.name == entry_name)
        .map(|f| f.size)
        .ok_or_else(|| AppError::NotFound {
            message: format!("7z/CB7 page entry '{entry_name}' not found"),
        })?;
    if declared_size == 0 || declared_size > MAX_COMIC_PAGE_BYTES {
        return Err(AppError::CorruptedData {
            message: "7z/CB7 page exceeds maximum permitted 32 MiB limit".to_owned(),
        });
    }

    let bytes = reader.read_file(entry_name).map_err(map_sevenz_error)?;
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > MAX_COMIC_PAGE_BYTES {
        return Err(AppError::CorruptedData {
            message: "decoded 7z/CB7 page exceeded maximum 32 MiB limit".to_owned(),
        });
    }

    let info = inspect_image_bytes(&bytes).map_err(|e| AppError::CorruptedData {
        message: format!("invalid image in 7z/CB7 page: {e:?}"),
    })?;
    Ok((bytes, info.format.mime_type().to_owned()))
}

fn map_sevenz_error(err: SevenZError) -> AppError {
    match err {
        SevenZError::PasswordRequired | SevenZError::MaybeBadPassword(_) => {
            AppError::EncryptedArchive {
                message: "password-protected / encrypted 7z or CB7 archives are not supported"
                    .to_owned(),
            }
        }
        SevenZError::UnsupportedCompressionMethod(method) => AppError::UnsupportedFormat {
            message: format!("unsupported 7z compression method ({method})"),
        },
        SevenZError::Unsupported(msg) => AppError::UnsupportedFormat {
            message: format!("unsupported 7z archive feature: {msg}"),
        },
        SevenZError::FileNotFound => AppError::NotFound {
            message: "requested page not found in 7z archive".to_owned(),
        },
        other => AppError::CorruptedData {
            message: format!("corrupted or unreadable 7z/CB7 archive: {other}"),
        },
    }
}
