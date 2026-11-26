//! Pure-Rust O(1) TAR/CBT comic archive indexer and direct byte-offset page reader.

use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use bytes::Bytes;
use mangacore::inspect_image_bytes;
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncReadExt, AsyncSeekExt};

use crate::error::AppError;
use crate::services::comics::sort::{
    ComicMetadata, MAX_COMIC_ARCHIVE_BYTES, MAX_COMIC_PAGE_BYTES, MAX_METADATA_BYTES,
    MAX_PAGES_PER_CHAPTER, clean_stem_from_path, compute_fast_file_fingerprint, is_junk_entry,
    is_supported_image_ext, natural_cmp, parse_comic_info_xml,
};
use crate::services::comics::zip_epub::build_parsed_chapter;
use crate::services::comics::{ComicFormat, PageDescriptor, ParsedComicChapter};

/// Indexed byte offset of a single image page inside a `.cbt`/`.tar` archive for O(1) reads.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TarPageEntry {
    /// Entry path inside the TAR archive.
    pub name: String,
    /// Exact byte offset where the entry's raw payload begins.
    pub data_offset: u64,
    /// Payload length in bytes.
    pub size: u64,
}

/// Checks whether a 512-byte buffer looks like a valid POSIX/GNU TAR header (either `ustar`
/// magic at offset 257 or a valid TAR header checksum on a non-zero block).
#[must_use]
pub(crate) fn looks_like_tar_header(header: &[u8]) -> bool {
    if header.len() < 512 {
        return false;
    }
    if &header[257..262] == b"ustar" {
        return true;
    }
    if header[..512].iter().all(|&b| b == 0) {
        return false;
    }
    let stored_raw = std::str::from_utf8(&header[148..156]).unwrap_or("");
    let stored_str = stored_raw.trim_matches(|c: char| c == '\0' || c.is_ascii_whitespace());
    let Ok(stored_sum) = u32::from_str_radix(stored_str, 8) else {
        return false;
    };
    let mut computed: u32 = 0;
    for (idx, &b) in header[..512].iter().enumerate() {
        if (148..156).contains(&idx) {
            computed = computed.saturating_add(u32::from(b' '));
        } else {
            computed = computed.saturating_add(u32::from(b));
        }
    }
    computed == stored_sum && stored_sum > 0
}

/// Indexes a `.cbt` or `.tar` comic archive in a single streaming pass, recording exact file
/// offsets so subsequent page reads are O(1) seek+read operations.
///
/// # Errors
///
/// Returns [`AppError`] if the TAR archive is malformed, oversized, or contains no readable images.
pub(crate) fn inspect_tar_sync(
    path: &Path,
    override_series: Option<&str>,
) -> Result<ParsedComicChapter, AppError> {
    let mut file = std::fs::File::open(path).map_err(|e| AppError::NotFound {
        message: format!("cannot open TAR/CBT archive: {e}"),
    })?;
    let file_size = file
        .metadata()
        .map_err(|e| AppError::Internal {
            message: format!("cannot stat TAR/CBT archive: {e}"),
        })?
        .len();
    if !(512..=MAX_COMIC_ARCHIVE_BYTES).contains(&file_size) {
        return Err(AppError::CorruptedData {
            message: "invalid or oversized TAR/CBT archive".to_owned(),
        });
    }

    let file_sha_short = compute_fast_file_fingerprint(&mut file, file_size)?;
    file.seek(SeekFrom::Start(0))
        .map_err(|e| AppError::CorruptedData {
            message: e.to_string(),
        })?;

    let mut archive = tar::Archive::new(file);
    let mut comic_info = ComicMetadata::default();
    let mut pages: Vec<TarPageEntry> = Vec::new();

    let entries = archive.entries().map_err(|e| AppError::CorruptedData {
        message: format!("failed to read TAR/CBT headers: {e}"),
    })?;

    for entry_res in entries {
        let mut entry = entry_res.map_err(|e| AppError::CorruptedData {
            message: format!("corrupted TAR/CBT entry: {e}"),
        })?;
        if !entry.header().entry_type().is_file() {
            continue;
        }
        let raw_name = entry
            .path()
            .map(|p| p.to_string_lossy().replace('\\', "/"))
            .unwrap_or_default();
        if is_junk_entry(&raw_name) {
            continue;
        }

        let size = entry.size();
        let data_offset = entry.raw_file_position();
        let lower = raw_name.to_ascii_lowercase();
        let file_part = lower.rsplit('/').next().unwrap_or(&lower);

        if file_part == "comicinfo.xml" && size > 0 && size <= MAX_METADATA_BYTES {
            let cap = usize::try_from(size).unwrap_or(0);
            let mut xml_buf = Vec::with_capacity(cap);
            if entry.read_to_end(&mut xml_buf).is_ok() {
                comic_info = parse_comic_info_xml(&xml_buf);
            }
            continue;
        }

        if is_supported_image_ext(file_part)
            && size > 0
            && size <= MAX_COMIC_PAGE_BYTES
            && data_offset.saturating_add(size) <= file_size
            && pages.len() < MAX_PAGES_PER_CHAPTER
        {
            pages.push(TarPageEntry {
                name: raw_name,
                data_offset,
                size,
            });
        }
    }

    pages.sort_by(|a, b| natural_cmp(&a.name, &b.name));
    if pages.is_empty() {
        return Err(AppError::UnsupportedFormat {
            message: "no readable comic images found inside TAR/CBT archive".to_owned(),
        });
    }

    // Read page 0 directly by byte offset for cover thumbnail.
    let first_bytes = read_tar_slice_sync(path, &pages[0])?;
    let header_info = inspect_image_bytes(&first_bytes).map_err(|e| AppError::CorruptedData {
        message: format!("first page in TAR/CBT archive is not a valid image: {e:?}"),
    })?;
    let first_mime = header_info.format.mime_type().to_owned();

    let stem = clean_stem_from_path(path, "CBT Archive");
    build_parsed_chapter(
        ComicFormat::Tar,
        &stem,
        override_series,
        comic_info,
        pages.into_iter().map(PageDescriptor::TarEntry).collect(),
        Bytes::from(first_bytes),
        first_mime,
        file_size,
        file_sha_short,
    )
}

fn read_tar_slice_sync(archive_path: &Path, entry: &TarPageEntry) -> Result<Vec<u8>, AppError> {
    if entry.size == 0 || entry.size > MAX_COMIC_PAGE_BYTES {
        return Err(AppError::CorruptedData {
            message: "TAR/CBT page exceeds maximum permitted 32 MiB limit".to_owned(),
        });
    }
    let mut file = std::fs::File::open(archive_path).map_err(|e| AppError::NotFound {
        message: format!("TAR/CBT archive missing on disk: {e}"),
    })?;
    file.seek(SeekFrom::Start(entry.data_offset))
        .map_err(|e| AppError::CorruptedData {
            message: format!("failed to seek TAR/CBT page offset: {e}"),
        })?;
    let len = usize::try_from(entry.size).unwrap_or(0);
    let mut buf = vec![0_u8; len];
    file.read_exact(&mut buf)
        .map_err(|e| AppError::CorruptedData {
            message: format!("truncated TAR/CBT page payload: {e}"),
        })?;
    Ok(buf)
}

/// Reads a single TAR/CBT page in O(1) using its indexed byte offset.
///
/// # Errors
///
/// Returns [`AppError`] if the archive is missing, truncated, or contains invalid image bytes.
pub(crate) async fn read_tar_entry_async(
    archive_path: &Path,
    entry: TarPageEntry,
) -> Result<(Vec<u8>, String), AppError> {
    if entry.size == 0 || entry.size > MAX_COMIC_PAGE_BYTES {
        return Err(AppError::CorruptedData {
            message: "TAR/CBT page exceeds maximum permitted 32 MiB limit".to_owned(),
        });
    }

    let mut file = tokio::fs::File::open(archive_path)
        .await
        .map_err(|e| AppError::NotFound {
            message: format!("TAR/CBT archive missing on disk: {e}"),
        })?;
    file.seek(SeekFrom::Start(entry.data_offset))
        .await
        .map_err(|e| AppError::CorruptedData {
            message: format!("failed to seek TAR/CBT page offset: {e}"),
        })?;
    let len = usize::try_from(entry.size).unwrap_or(0);
    let mut bytes = vec![0_u8; len];
    file.read_exact(&mut bytes)
        .await
        .map_err(|e| AppError::CorruptedData {
            message: format!("truncated TAR/CBT page payload: {e}"),
        })?;

    let info = inspect_image_bytes(&bytes).map_err(|e| AppError::CorruptedData {
        message: format!("invalid image in TAR/CBT page: {e:?}"),
    })?;
    Ok((bytes, info.format.mime_type().to_owned()))
}
