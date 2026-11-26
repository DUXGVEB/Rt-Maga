//! Pure-Rust `.cbr`/`.rar` comic archive indexer and lazy per-page reader backed by `rars`
//! (supports RAR 1.3–4.x, RAR5, and RAR7 with zero C/C++ dependencies).

use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, Mutex};

use bytes::Bytes;
use mangacore::inspect_image_bytes;
use rars::{Archive, ArchiveReadOptions, ArchiveReader, Error as RarError, ErrorKind};

use crate::error::AppError;
use crate::services::comics::sort::{
    ComicMetadata, MAX_COMIC_ARCHIVE_BYTES, MAX_COMIC_PAGE_BYTES, MAX_METADATA_BYTES,
    MAX_PAGES_PER_CHAPTER, clean_stem_from_path, compute_fast_file_fingerprint, is_junk_entry,
    is_supported_image_ext, natural_cmp, parse_comic_info_xml,
};
use crate::services::comics::zip_epub::build_parsed_chapter;
use crate::services::comics::{ArchivePageEntry, ComicFormat, PageDescriptor, ParsedComicChapter};

/// Maximum number of parsed RAR archive handles kept warm in memory for sequential page reads.
const MAX_CACHED_RAR_HANDLES: usize = 2;

type CachedRarEntry = (PathBuf, u64, Arc<Archive>);

static OPEN_RAR_CACHE: LazyLock<Mutex<Vec<CachedRarEntry>>> =
    LazyLock::new(|| Mutex::new(Vec::with_capacity(MAX_CACHED_RAR_HANDLES)));

/// Indexes a `.cbr` or `.rar` archive, reading only `ComicInfo.xml` (if present) and cover page 0.
///
/// # Errors
///
/// Returns [`AppError::EncryptedArchive`] if password-protected, [`AppError::UnsupportedFormat`]
/// if no comic images are found, or [`AppError::CorruptedData`] on invalid RAR headers.
pub(crate) fn inspect_rar_sync(
    path: &Path,
    override_series: Option<&str>,
) -> Result<ParsedComicChapter, AppError> {
    let mut raw_file = std::fs::File::open(path).map_err(|e| AppError::NotFound {
        message: format!("cannot open RAR/CBR archive: {e}"),
    })?;
    let file_size = raw_file
        .metadata()
        .map_err(|e| AppError::Internal {
            message: format!("cannot stat RAR/CBR archive: {e}"),
        })?
        .len();
    if !(16..=MAX_COMIC_ARCHIVE_BYTES).contains(&file_size) {
        return Err(AppError::CorruptedData {
            message: "invalid or oversized RAR/CBR archive".to_owned(),
        });
    }
    let file_sha_short = compute_fast_file_fingerprint(&mut raw_file, file_size)?;
    drop(raw_file);

    let archive = open_rar_uncached(path)?;
    let mut comic_info_idx: Option<usize> = None;
    let mut pages: Vec<ArchivePageEntry> = Vec::new();

    for (idx, member) in archive.member_refs().enumerate() {
        if member.is_directory() || member.is_redirection() || member.is_split_before() {
            continue;
        }
        let decoded_name = member.decoded_name(None).map_or_else(
            |_| String::from_utf8_lossy(member.name_bytes()).replace('\\', "/"),
            |b| String::from_utf8_lossy(&b).replace('\\', "/"),
        );

        if is_junk_entry(&decoded_name) {
            continue;
        }

        let lower = decoded_name.to_ascii_lowercase();
        let file_part = lower.rsplit('/').next().unwrap_or(&lower);
        if file_part == "comicinfo.xml" && comic_info_idx.is_none() {
            comic_info_idx = Some(idx);
            continue;
        }

        if is_supported_image_ext(file_part) && pages.len() < MAX_PAGES_PER_CHAPTER {
            pages.push(ArchivePageEntry {
                name: decoded_name,
                index: idx,
                uncompressed_size: 0,
            });
        }
    }

    pages.sort_by(|a, b| natural_cmp(&a.name, &b.name));
    if pages.is_empty() {
        return Err(AppError::UnsupportedFormat {
            message: "no readable comic images found inside RAR/CBR archive".to_owned(),
        });
    }

    let comic_info = if let Some(xml_idx) = comic_info_idx {
        let meta_opts = ArchiveReadOptions::new()
            .with_max_member_output_bytes(MAX_METADATA_BYTES)
            .with_max_total_output_bytes(MAX_METADATA_BYTES);
        archive
            .read_member_at_with_options(xml_idx, meta_opts)
            .ok()
            .flatten()
            .map(|b| parse_comic_info_xml(&b))
            .unwrap_or_default()
    } else {
        ComicMetadata::default()
    };

    let page_opts = rar_page_read_options();
    let first_bytes = archive
        .read_member_at_with_options(pages[0].index, page_opts)
        .map_err(|e| map_rar_error(&e))?
        .ok_or_else(|| AppError::CorruptedData {
            message: "first page in RAR/CBR archive could not be extracted".to_owned(),
        })?;
    if u64::try_from(first_bytes.len()).unwrap_or(u64::MAX) > MAX_COMIC_PAGE_BYTES {
        return Err(AppError::CorruptedData {
            message: "first page in RAR/CBR exceeds maximum 32 MiB limit".to_owned(),
        });
    }

    let header_info = inspect_image_bytes(&first_bytes).map_err(|e| AppError::CorruptedData {
        message: format!("first page in RAR/CBR archive is not a valid image: {e:?}"),
    })?;
    let first_mime = header_info.format.mime_type().to_owned();

    let stem = clean_stem_from_path(path, "CBR Archive");
    build_parsed_chapter(
        ComicFormat::Rar,
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

/// Lazily extracts and validates a single indexed image entry from a `.cbr`/`.rar` archive.
///
/// # Errors
///
/// Returns [`AppError`] if the archive is missing, encrypted, exceeds size limits, or corrupted.
pub(crate) fn read_rar_entry_sync(
    archive_path: &Path,
    entry_index: usize,
) -> Result<(Vec<u8>, String), AppError> {
    let archive = load_rar_cached(archive_path)?;
    let opts = rar_page_read_options();
    let bytes = archive
        .read_member_at_with_options(entry_index, opts)
        .map_err(|e| map_rar_error(&e))?
        .ok_or_else(|| AppError::NotFound {
            message: format!("RAR/CBR page entry index {entry_index} not found"),
        })?;

    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > MAX_COMIC_PAGE_BYTES {
        return Err(AppError::CorruptedData {
            message: "RAR/CBR page exceeds maximum permitted 32 MiB limit".to_owned(),
        });
    }

    let info = inspect_image_bytes(&bytes).map_err(|e| AppError::CorruptedData {
        message: format!("invalid image in RAR/CBR page: {e:?}"),
    })?;
    Ok((bytes, info.format.mime_type().to_owned()))
}

/// Evicts any cached open RAR archive handles pointing inside `dir_prefix`.
pub(crate) fn evict_cached_rars_for_prefix(dir_prefix: &Path) {
    if let Ok(mut guard) = OPEN_RAR_CACHE.lock() {
        guard.retain(|(p, _, _)| !p.starts_with(dir_prefix));
    }
}

fn load_rar_cached(path: &Path) -> Result<Arc<Archive>, AppError> {
    let file_size = std::fs::metadata(path)
        .map_err(|e| AppError::NotFound {
            message: format!("RAR/CBR archive missing on disk: {e}"),
        })?
        .len();

    if let Ok(mut guard) = OPEN_RAR_CACHE.lock()
        && let Some(pos) = guard
            .iter()
            .position(|(p, sz, _)| p == path && *sz == file_size)
    {
        let entry = guard.remove(pos);
        let handle = Arc::clone(&entry.2);
        guard.push(entry);
        return Ok(handle);
    }

    let loaded = Arc::new(open_rar_uncached(path)?);
    if let Ok(mut guard) = OPEN_RAR_CACHE.lock() {
        if guard.len() >= MAX_CACHED_RAR_HANDLES {
            let _ = guard.remove(0);
        }
        guard.push((path.to_path_buf(), file_size, Arc::clone(&loaded)));
    }
    Ok(loaded)
}

fn open_rar_uncached(path: &Path) -> Result<Archive, AppError> {
    let opts = ArchiveReadOptions::new()
        .with_max_header_count(16_384)
        .with_max_member_output_bytes(MAX_COMIC_PAGE_BYTES)
        .with_max_total_output_bytes(MAX_COMIC_ARCHIVE_BYTES);
    ArchiveReader::read_path_with_options(path, opts).map_err(|e| map_rar_error(&e))
}

fn rar_page_read_options() -> ArchiveReadOptions<'static> {
    ArchiveReadOptions::new()
        .with_max_member_output_bytes(MAX_COMIC_PAGE_BYTES)
        .with_max_total_output_bytes(MAX_COMIC_PAGE_BYTES)
}

fn map_rar_error(err: &RarError) -> AppError {
    match err.kind() {
        ErrorKind::PasswordRequired | ErrorKind::BadPassword => AppError::EncryptedArchive {
            message: "password-protected / encrypted RAR or CBR archives are not supported"
                .to_owned(),
        },
        ErrorKind::UnsupportedFormat | ErrorKind::UnsupportedFeature => {
            AppError::UnsupportedFormat {
                message: format!("unsupported RAR/CBR feature: {err}"),
            }
        }
        ErrorKind::EntryNotFound => AppError::NotFound {
            message: format!("RAR/CBR entry not found: {err}"),
        },
        ErrorKind::ResourceLimit => AppError::CorruptedData {
            message: format!("RAR/CBR page exceeded safety memory/size limit: {err}"),
        },
        _ => AppError::CorruptedData {
            message: format!("corrupted or unreadable RAR/CBR archive: {err}"),
        },
    }
}
