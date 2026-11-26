//! Pure-Rust O(1) ZIP/ZIP64 Central Directory parser for `.cbz`/`.zip` comic archives and
//! fixed-layout `.epub` ebooks (`META-INF/container.xml` + `.opf` metadata/manifest/spine).

use std::collections::{HashMap, HashSet};
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use bytes::Bytes;
use crc32fast::Hasher as Crc32Hasher;
use flate2::read::DeflateDecoder;
use mangacore::inspect_image_bytes;
use quick_xml::Reader as XmlReader;
use quick_xml::events::Event;
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncReadExt, AsyncSeekExt};

use crate::error::AppError;
use crate::services::comics::sort::{
    ComicMetadata, MAX_COMIC_ARCHIVE_BYTES, MAX_COMIC_PAGE_BYTES, MAX_METADATA_BYTES,
    MAX_PAGES_PER_CHAPTER, clean_stem_from_path, compute_fast_file_fingerprint,
    infer_series_and_chapter_from_stem, is_junk_entry, is_supported_image_ext, natural_cmp,
    parse_comic_info_xml, push_csv_unique,
};
use crate::services::comics::{ComicFormat, PageDescriptor, ParsedComicChapter};

/// Maximum Central Directory tail search window (`64 KiB + 22 bytes`).
const EOCD_SEARCH_WINDOW: u64 = 65_557;

/// Indexed location of a single image page inside a ZIP/CBZ/EPUB archive for O(1) reads.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ZipPageEntry {
    /// Archive entry path (used for natural ordering and diagnostics).
    pub name: String,
    /// Compression method (`0` = Stored, `8` = Deflate).
    pub method: u16,
    /// Byte offset of the local file header inside the archive.
    pub local_header_offset: u64,
    /// Compressed payload byte length.
    pub compressed_size: u64,
    /// Uncompressed payload byte length.
    pub uncompressed_size: u64,
    /// Expected CRC-32 checksum.
    pub crc32: u32,
}

/// Inspects a ZIP container's Central Directory to determine whether it is an `.epub` ebook
/// (`mimetype` containing `application/epub+zip` or `META-INF/container.xml`).
pub(crate) fn is_zip_epub_sync(path: &Path) -> bool {
    let Ok(mut file) = std::fs::File::open(path) else {
        return false;
    };
    let Ok(meta) = file.metadata() else {
        return false;
    };
    let file_size = meta.len();
    if !(22..=MAX_COMIC_ARCHIVE_BYTES).contains(&file_size) {
        return false;
    }
    let Ok(entries) = read_zip_central_directory(&mut file, file_size) else {
        return false;
    };

    for entry in &entries {
        if entry.name.eq_ignore_ascii_case("META-INF/container.xml") {
            return true;
        }
        if entry.name.eq_ignore_ascii_case("mimetype")
            && entry.uncompressed_size <= 256
            && let Ok(bytes) = read_entry_raw_sync(&mut file, entry)
        {
            let text = String::from_utf8_lossy(&bytes);
            if text.trim().eq_ignore_ascii_case("application/epub+zip") {
                return true;
            }
        }
    }
    false
}

/// Parses a `.cbz` or `.zip` comic archive into a [`ParsedComicChapter`] without extracting
/// all pages into RAM (only reads `ComicInfo.xml` if present and the cover page 0).
///
/// # Errors
///
/// Returns [`AppError`] if the ZIP is corrupted, encrypted, oversized, or contains no readable images.
pub(crate) fn inspect_zip_sync(
    path: &Path,
    override_series: Option<&str>,
) -> Result<ParsedComicChapter, AppError> {
    let (mut file, file_size, file_sha_short, raw_entries) = open_and_index_zip(path)?;

    let mut comic_info = ComicMetadata::default();
    let mut pages = Vec::new();

    for entry in raw_entries {
        if is_junk_entry(&entry.name) {
            continue;
        }
        let lower_name = entry.name.to_ascii_lowercase();
        let file_part = lower_name.rsplit('/').next().unwrap_or(&lower_name);
        if file_part == "comicinfo.xml" && entry.uncompressed_size <= MAX_METADATA_BYTES {
            if let Ok(xml_bytes) = read_entry_raw_sync(&mut file, &entry) {
                comic_info = parse_comic_info_xml(&xml_bytes);
            }
            continue;
        }
        if is_supported_image_ext(file_part) {
            if entry.uncompressed_size == 0 || entry.uncompressed_size > MAX_COMIC_PAGE_BYTES {
                continue;
            }
            if pages.len() < MAX_PAGES_PER_CHAPTER {
                pages.push(entry);
            }
        }
    }

    pages.sort_by(|a, b| natural_cmp(&a.name, &b.name));
    if pages.is_empty() {
        return Err(AppError::UnsupportedFormat {
            message: "no readable comic images found inside ZIP/CBZ archive".to_owned(),
        });
    }

    let first_bytes = read_entry_raw_sync(&mut file, &pages[0])?;
    let header_info = inspect_image_bytes(&first_bytes).map_err(|e| AppError::CorruptedData {
        message: format!("first page in ZIP/CBZ archive is not a valid image: {e:?}"),
    })?;
    let first_mime = header_info.format.mime_type().to_owned();

    let stem = clean_stem_from_path(path, "Comic Archive");
    build_parsed_chapter(
        ComicFormat::Zip,
        &stem,
        override_series,
        comic_info,
        pages.into_iter().map(PageDescriptor::ZipEntry).collect(),
        Bytes::from(first_bytes),
        first_mime,
        file_size,
        file_sha_short,
    )
}

/// Parses an `.epub` comic/manga ebook by reading `META-INF/container.xml`, resolving the `.opf`
/// package metadata/manifest/spine, and indexing embedded page images in exact reading order.
///
/// # Errors
///
/// Returns [`AppError`] if the EPUB structure is invalid, encrypted, or contains no readable images.
#[expect(
    clippy::too_many_lines,
    reason = "resolves container.xml, OPF metadata/manifest/spine, XHTML image refs, and cover image"
)]
pub(crate) fn inspect_epub_sync(
    path: &Path,
    override_series: Option<&str>,
) -> Result<ParsedComicChapter, AppError> {
    let (mut file, file_size, file_sha_short, raw_entries) = open_and_index_zip(path)?;

    let mut entry_map: HashMap<String, ZipPageEntry> = HashMap::with_capacity(raw_entries.len());
    let mut lower_map: HashMap<String, String> = HashMap::with_capacity(raw_entries.len());
    let mut comic_info: Option<ComicMetadata> = None;

    for entry in raw_entries {
        let norm = normalize_zip_path(&entry.name);
        if norm.is_empty() || is_junk_entry(&norm) {
            continue;
        }
        let lower = norm.to_ascii_lowercase();
        if lower.rsplit('/').next() == Some("comicinfo.xml")
            && entry.uncompressed_size <= MAX_METADATA_BYTES
            && let Ok(xml_bytes) = read_entry_raw_sync(&mut file, &entry)
        {
            comic_info = Some(parse_comic_info_xml(&xml_bytes));
        }
        lower_map.insert(lower, norm.clone());
        entry_map.insert(norm, entry);
    }

    let container_entry = lookup_zip_entry(&entry_map, &lower_map, "META-INF/container.xml")
        .ok_or_else(|| AppError::CorruptedData {
            message: "EPUB is missing META-INF/container.xml".to_owned(),
        })?;
    if container_entry.uncompressed_size > MAX_METADATA_BYTES {
        return Err(AppError::CorruptedData {
            message: "EPUB container.xml exceeds maximum metadata limit".to_owned(),
        });
    }
    let container_bytes = read_entry_raw_sync(&mut file, &container_entry)?;
    let opf_path =
        parse_container_xml_opf_path(&container_bytes).ok_or_else(|| AppError::CorruptedData {
            message: "EPUB META-INF/container.xml does not specify an OPF rootfile".to_owned(),
        })?;

    let opf_norm = normalize_zip_path(&opf_path);
    let opf_entry = lookup_zip_entry(&entry_map, &lower_map, &opf_norm).ok_or_else(|| {
        AppError::CorruptedData {
            message: format!("EPUB OPF package '{opf_norm}' not found inside archive"),
        }
    })?;
    if opf_entry.uncompressed_size > MAX_METADATA_BYTES {
        return Err(AppError::CorruptedData {
            message: "EPUB OPF package exceeds maximum metadata limit".to_owned(),
        });
    }
    let opf_bytes = read_entry_raw_sync(&mut file, &opf_entry)?;
    let opf = parse_opf_package(&opf_bytes);
    let opf_dir = parent_dir(&opf_norm);

    let mut ordered_pages: Vec<ZipPageEntry> = Vec::new();
    let mut seen_paths: HashSet<String> = HashSet::new();

    for idref in &opf.spine_idrefs {
        let Some(item) = opf.manifest.get(idref) else {
            continue;
        };
        let resolved_item_path = resolve_relative_zip_path(&opf_dir, &item.href);
        let Some(zip_item) = lookup_zip_entry(&entry_map, &lower_map, &resolved_item_path) else {
            continue;
        };

        let media_lower = item.media_type.to_ascii_lowercase();
        let path_lower = resolved_item_path.to_ascii_lowercase();

        if media_lower.starts_with("image/") || is_supported_image_ext(&path_lower) {
            push_valid_epub_image(&zip_item, &mut seen_paths, &mut ordered_pages);
            continue;
        }

        let is_markup = media_lower.contains("xhtml")
            || media_lower.contains("html")
            || media_lower.contains("svg")
            || Path::new(&resolved_item_path)
                .extension()
                .is_some_and(|ext| {
                    ext.eq_ignore_ascii_case("xhtml")
                        || ext.eq_ignore_ascii_case("html")
                        || ext.eq_ignore_ascii_case("htm")
                        || ext.eq_ignore_ascii_case("svg")
                });

        if is_markup
            && zip_item.uncompressed_size <= MAX_METADATA_BYTES
            && let Ok(doc_bytes) = read_entry_raw_sync(&mut file, &zip_item)
        {
            let doc_dir = parent_dir(&resolved_item_path);
            for raw_href in extract_markup_image_refs(&doc_bytes) {
                let img_path = resolve_relative_zip_path(&doc_dir, &raw_href);
                if let Some(img_entry) = lookup_zip_entry(&entry_map, &lower_map, &img_path) {
                    push_valid_epub_image(&img_entry, &mut seen_paths, &mut ordered_pages);
                }
            }
        }
    }

    // Fallback: if spine documents had no direct image tags (e.g. non-standard manga EPUB),
    // collect image items declared in the OPF manifest sorted naturally.
    if ordered_pages.is_empty() {
        let mut fallback_imgs: Vec<ZipPageEntry> = opf
            .manifest
            .values()
            .filter_map(|item| {
                let media_lower = item.media_type.to_ascii_lowercase();
                let resolved = resolve_relative_zip_path(&opf_dir, &item.href);
                if media_lower.starts_with("image/")
                    || is_supported_image_ext(&resolved.to_ascii_lowercase())
                {
                    lookup_zip_entry(&entry_map, &lower_map, &resolved)
                } else {
                    None
                }
            })
            .filter(|e| e.uncompressed_size > 0 && e.uncompressed_size <= MAX_COMIC_PAGE_BYTES)
            .collect();
        fallback_imgs.sort_by(|a, b| natural_cmp(&a.name, &b.name));
        for e in fallback_imgs {
            push_valid_epub_image(&e, &mut seen_paths, &mut ordered_pages);
        }
    }

    if ordered_pages.is_empty() {
        return Err(AppError::UnsupportedFormat {
            message: "EPUB contains no readable embedded comic page images (text-only reflowable EPUBs are not supported)".to_owned(),
        });
    }

    // Prefer dedicated OPF cover image if valid, otherwise fall back to spine page 0.
    let mut cover_bytes_opt: Option<(Vec<u8>, String)> = None;
    if let Some(ref cover_id) = opf.cover_manifest_id
        && let Some(cover_item) = opf.manifest.get(cover_id)
    {
        let cover_path = resolve_relative_zip_path(&opf_dir, &cover_item.href);
        if let Some(cover_entry) = lookup_zip_entry(&entry_map, &lower_map, &cover_path)
            && cover_entry.uncompressed_size > 0
            && cover_entry.uncompressed_size <= MAX_COMIC_PAGE_BYTES
            && let Ok(bytes) = read_entry_raw_sync(&mut file, &cover_entry)
            && let Ok(info) = inspect_image_bytes(&bytes)
        {
            cover_bytes_opt = Some((bytes, info.format.mime_type().to_owned()));
        }
    }

    let (first_bytes, first_mime) = if let Some(cov) = cover_bytes_opt {
        cov
    } else {
        let b = read_entry_raw_sync(&mut file, &ordered_pages[0])?;
        let info = inspect_image_bytes(&b).map_err(|e| AppError::CorruptedData {
            message: format!("first image page in EPUB is not a valid raster image: {e:?}"),
        })?;
        (b, info.format.mime_type().to_owned())
    };

    // Merge OPF metadata with optional ComicInfo.xml (ComicInfo takes precedence if present).
    let mut merged_meta = opf.metadata;
    if let Some(ci) = comic_info {
        if ci.series.is_some() {
            merged_meta.series = ci.series;
        }
        if ci.title.is_some() {
            merged_meta.title = ci.title;
        }
        if ci.number.is_some() {
            merged_meta.number = ci.number;
        }
        if ci.volume.is_some() {
            merged_meta.volume = ci.volume;
        }
        if ci.summary.is_some() {
            merged_meta.summary = ci.summary;
        }
        if !ci.authors.is_empty() {
            merged_meta.authors = ci.authors;
        }
        if !ci.artists.is_empty() {
            merged_meta.artists = ci.artists;
        }
        if !ci.genres.is_empty() {
            merged_meta.genres = ci.genres;
        }
    }

    let stem = clean_stem_from_path(path, "EPUB Comic");
    build_parsed_chapter(
        ComicFormat::Epub,
        &stem,
        override_series,
        merged_meta,
        ordered_pages
            .into_iter()
            .map(PageDescriptor::ZipEntry)
            .collect(),
        Bytes::from(first_bytes),
        first_mime,
        file_size,
        file_sha_short,
    )
}

fn push_valid_epub_image(
    entry: &ZipPageEntry,
    seen_paths: &mut HashSet<String>,
    ordered_pages: &mut Vec<ZipPageEntry>,
) {
    if ordered_pages.len() >= MAX_PAGES_PER_CHAPTER {
        return;
    }
    if entry.uncompressed_size == 0 || entry.uncompressed_size > MAX_COMIC_PAGE_BYTES {
        return;
    }
    let lower = entry.name.to_ascii_lowercase();
    if !is_supported_image_ext(&lower) {
        return;
    }
    if seen_paths.insert(lower) {
        ordered_pages.push(entry.clone());
    }
}

/// Reads and decompresses a single [`ZipPageEntry`] directly from its byte offset asynchronously.
///
/// # Errors
///
/// Returns [`AppError`] if the archive is missing, truncated, fails CRC-32 verification, or contains
/// invalid image bytes.
pub(crate) async fn read_zip_entry_async(
    archive_path: &Path,
    entry: ZipPageEntry,
) -> Result<(Vec<u8>, String), AppError> {
    if entry.compressed_size > MAX_COMIC_PAGE_BYTES
        || entry.uncompressed_size > MAX_COMIC_PAGE_BYTES
    {
        return Err(AppError::CorruptedData {
            message: "archive page exceeds maximum permitted 32 MiB limit".to_owned(),
        });
    }

    let mut file = tokio::fs::File::open(archive_path)
        .await
        .map_err(|e| AppError::NotFound {
            message: format!("comic archive file missing on disk: {e}"),
        })?;

    file.seek(SeekFrom::Start(entry.local_header_offset))
        .await
        .map_err(|e| AppError::CorruptedData {
            message: format!("failed to seek ZIP local header: {e}"),
        })?;
    let mut local_hdr = [0_u8; 30];
    file.read_exact(&mut local_hdr)
        .await
        .map_err(|e| AppError::CorruptedData {
            message: format!("truncated ZIP local header: {e}"),
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

    file.seek(SeekFrom::Start(data_offset))
        .await
        .map_err(|e| AppError::CorruptedData {
            message: format!("failed to seek ZIP page payload: {e}"),
        })?;
    let comp_len = usize::try_from(entry.compressed_size).unwrap_or(0);
    let mut compressed = vec![0_u8; comp_len];
    file.read_exact(&mut compressed)
        .await
        .map_err(|e| AppError::CorruptedData {
            message: format!("truncated ZIP page payload: {e}"),
        })?;

    tokio::task::spawn_blocking(move || decode_and_verify_zip_entry(&entry, &compressed))
        .await
        .map_err(|e| AppError::Internal {
            message: format!("ZIP decode worker failed: {e}"),
        })?
}

fn open_and_index_zip(
    path: &Path,
) -> Result<(std::fs::File, u64, String, Vec<ZipPageEntry>), AppError> {
    let mut file = std::fs::File::open(path).map_err(|e| AppError::NotFound {
        message: format!("cannot open comic archive: {e}"),
    })?;
    let meta = file.metadata().map_err(|e| AppError::Internal {
        message: format!("cannot stat comic archive: {e}"),
    })?;
    let file_size = meta.len();
    if !(22..=MAX_COMIC_ARCHIVE_BYTES).contains(&file_size) {
        return Err(AppError::CorruptedData {
            message: "invalid or oversized ZIP/EPUB archive".to_owned(),
        });
    }

    let mut head_magic = [0_u8; 4];
    file.read_exact(&mut head_magic)
        .map_err(|_| AppError::CorruptedData {
            message: "unable to read ZIP header".to_owned(),
        })?;
    if head_magic != [0x50, 0x4B, 0x03, 0x04] {
        return Err(AppError::CorruptedData {
            message: "file does not start with a valid ZIP header".to_owned(),
        });
    }

    let sha_short = compute_fast_file_fingerprint(&mut file, file_size)?;
    let entries = read_zip_central_directory(&mut file, file_size)?;
    Ok((file, file_size, sha_short, entries))
}

fn read_zip_central_directory(
    file: &mut std::fs::File,
    file_size: u64,
) -> Result<Vec<ZipPageEntry>, AppError> {
    let tail_size = usize::try_from(file_size.min(EOCD_SEARCH_WINDOW)).unwrap_or(22);
    let tail_start = file_size.saturating_sub(u64::try_from(tail_size).unwrap_or(0));
    file.seek(SeekFrom::Start(tail_start))
        .map_err(|e| AppError::CorruptedData {
            message: format!("seek ZIP EOCD failed: {e}"),
        })?;
    let mut tail = vec![0_u8; tail_size];
    file.read_exact(&mut tail)
        .map_err(|e| AppError::CorruptedData {
            message: format!("read ZIP EOCD failed: {e}"),
        })?;

    let eocd_pos = (0..=tail.len().saturating_sub(22))
        .rev()
        .find(|&i| tail[i..i + 4] == [0x50, 0x4B, 0x05, 0x06])
        .ok_or_else(|| AppError::CorruptedData {
            message: "ZIP End of Central Directory not found (archive may be truncated)".to_owned(),
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
        if z64_offset.saturating_add(56) <= file_size {
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
            message: "ZIP Central Directory bounds are invalid".to_owned(),
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

fn parse_central_directory_entries(cd_buf: &[u8]) -> Result<Vec<ZipPageEntry>, AppError> {
    let mut out = Vec::new();
    let mut cursor = 0_usize;

    while cursor + 46 <= cd_buf.len() {
        if cd_buf[cursor..cursor + 4] != [0x50, 0x4B, 0x01, 0x02] {
            break;
        }
        let flags = u16::from_le_bytes([cd_buf[cursor + 8], cd_buf[cursor + 9]]);
        if (flags & 0x0001) != 0 {
            return Err(AppError::EncryptedArchive {
                message: "password-protected / encrypted ZIP or CBZ archives are not supported"
                    .to_owned(),
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

        if matches!(method, 0 | 8) && out.len() < 16_384 {
            out.push(ZipPageEntry {
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
    entry: &ZipPageEntry,
) -> Result<Vec<u8>, AppError> {
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
    decode_raw_zip_payload(entry, &comp_buf)
}

fn decode_raw_zip_payload(entry: &ZipPageEntry, compressed: &[u8]) -> Result<Vec<u8>, AppError> {
    let decoded = match entry.method {
        0 => compressed.to_vec(),
        8 => {
            let cap =
                usize::try_from(entry.uncompressed_size.min(MAX_COMIC_PAGE_BYTES)).unwrap_or(0);
            let mut out = Vec::with_capacity(cap);
            let decoder = DeflateDecoder::new(compressed);
            decoder
                .take(MAX_COMIC_PAGE_BYTES + 1)
                .read_to_end(&mut out)
                .map_err(|e| AppError::CorruptedData {
                    message: format!("deflate decompression failed: {e}"),
                })?;
            if u64::try_from(out.len()).unwrap_or(u64::MAX) > MAX_COMIC_PAGE_BYTES {
                return Err(AppError::CorruptedData {
                    message: "archive entry exceeded maximum 32 MiB page size".to_owned(),
                });
            }
            out
        }
        other => {
            return Err(AppError::UnsupportedFormat {
                message: format!("unsupported ZIP compression method {other}"),
            });
        }
    };

    let mut crc = Crc32Hasher::new();
    crc.update(&decoded);
    if entry.crc32 != 0 && crc.finalize() != entry.crc32 {
        return Err(AppError::CorruptedData {
            message: "ZIP entry CRC-32 checksum verification failed".to_owned(),
        });
    }
    Ok(decoded)
}

fn decode_and_verify_zip_entry(
    entry: &ZipPageEntry,
    compressed: &[u8],
) -> Result<(Vec<u8>, String), AppError> {
    let bytes = decode_raw_zip_payload(entry, compressed)?;
    let info = inspect_image_bytes(&bytes).map_err(|e| AppError::CorruptedData {
        message: format!("invalid image in ZIP/EPUB page: {e:?}"),
    })?;
    Ok((bytes, info.format.mime_type().to_owned()))
}

// ---------------------------------------------------------------------------
// EPUB XML / OPF / XHTML helpers
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct OpfManifestItem {
    href: String,
    media_type: String,
}

#[derive(Debug, Default)]
struct ParsedOpf {
    metadata: ComicMetadata,
    manifest: HashMap<String, OpfManifestItem>,
    spine_idrefs: Vec<String>,
    cover_manifest_id: Option<String>,
}

fn parse_container_xml_opf_path(xml_bytes: &[u8]) -> Option<String> {
    let mut reader = XmlReader::from_reader(xml_bytes);
    reader.config_mut().trim_text(true);
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Empty(e) | Event::Start(e)) => {
                let local = local_xml_name(e.name().as_ref());
                if local.eq_ignore_ascii_case("rootfile") {
                    for attr in e.attributes().flatten() {
                        let key = local_xml_name(attr.key.as_ref());
                        if key.eq_ignore_ascii_case("full-path")
                            && let Ok(val) = attr.unescape_value()
                        {
                            let trimmed = val.trim();
                            if !trimmed.is_empty() {
                                return Some(trimmed.to_owned());
                            }
                        }
                    }
                }
            }
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
        buf.clear();
    }
    None
}

#[expect(
    clippy::too_many_lines,
    reason = "single-pass quick-xml parser for OPF metadata, manifest, and spine"
)]
fn parse_opf_package(xml_bytes: &[u8]) -> ParsedOpf {
    let mut reader = XmlReader::from_reader(xml_bytes);
    reader.config_mut().trim_text(true);
    let mut out = ParsedOpf::default();
    let mut buf = Vec::new();
    let mut current_meta_tag = String::new();
    let mut in_meta_collection = false;

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                let local = local_xml_name(e.name().as_ref()).to_ascii_lowercase();
                match local.as_str() {
                    "title" | "creator" | "description" | "subject" | "language" => {
                        current_meta_tag = local;
                    }
                    "meta" => {
                        let mut prop = String::new();
                        let mut name = String::new();
                        let mut content = String::new();
                        for attr in e.attributes().flatten() {
                            let k = local_xml_name(attr.key.as_ref()).to_ascii_lowercase();
                            if let Ok(v) = attr.unescape_value() {
                                match k.as_str() {
                                    "property" => prop = v.trim().to_ascii_lowercase(),
                                    "name" => name = v.trim().to_ascii_lowercase(),
                                    "content" => v.trim().clone_into(&mut content),
                                    _ => {}
                                }
                            }
                        }
                        if name == "cover" && !content.is_empty() && out.cover_manifest_id.is_none()
                        {
                            out.cover_manifest_id = Some(content);
                        }
                        if prop == "belongs-to-collection" {
                            in_meta_collection = true;
                        }
                    }
                    "item" => handle_opf_manifest_item(&e, &mut out),
                    "itemref" => handle_opf_itemref(&e, &mut out),
                    _ => {}
                }
            }
            Ok(Event::Empty(e)) => {
                let local = local_xml_name(e.name().as_ref()).to_ascii_lowercase();
                match local.as_str() {
                    "item" => handle_opf_manifest_item(&e, &mut out),
                    "itemref" => handle_opf_itemref(&e, &mut out),
                    "meta" => {
                        let mut name = String::new();
                        let mut content = String::new();
                        for attr in e.attributes().flatten() {
                            let k = local_xml_name(attr.key.as_ref()).to_ascii_lowercase();
                            if let Ok(v) = attr.unescape_value() {
                                if k == "name" {
                                    name = v.trim().to_ascii_lowercase();
                                } else if k == "content" {
                                    v.trim().clone_into(&mut content);
                                }
                            }
                        }
                        if name == "cover" && !content.is_empty() && out.cover_manifest_id.is_none()
                        {
                            out.cover_manifest_id = Some(content);
                        }
                    }
                    _ => {}
                }
            }
            Ok(Event::Text(t)) => {
                if let Ok(val) = t.decode() {
                    let text = val.trim();
                    if !text.is_empty() {
                        if in_meta_collection {
                            out.metadata.series = Some(text.to_owned());
                        } else {
                            match current_meta_tag.as_str() {
                                "title" if out.metadata.title.is_none() => {
                                    out.metadata.title = Some(text.to_owned());
                                }
                                "creator" => push_csv_unique(&mut out.metadata.authors, text),
                                "description" if out.metadata.summary.is_none() => {
                                    out.metadata.summary = Some(text.to_owned());
                                }
                                "subject" => push_csv_unique(&mut out.metadata.genres, text),
                                "language" if out.metadata.language.is_none() => {
                                    out.metadata.language = Some(text.to_owned());
                                }
                                _ => {}
                            }
                        }
                    }
                }
            }
            Ok(Event::End(_)) => {
                current_meta_tag.clear();
                in_meta_collection = false;
            }
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
        buf.clear();
    }

    if out.metadata.series.is_none() {
        out.metadata.series.clone_from(&out.metadata.title);
    }
    out
}

fn handle_opf_manifest_item(e: &quick_xml::events::BytesStart<'_>, out: &mut ParsedOpf) {
    let mut id = String::new();
    let mut href = String::new();
    let mut media_type = String::new();
    let mut properties = String::new();

    for attr in e.attributes().flatten() {
        let k = local_xml_name(attr.key.as_ref()).to_ascii_lowercase();
        if let Ok(v) = attr.unescape_value() {
            match k.as_str() {
                "id" => v.trim().clone_into(&mut id),
                "href" => v.trim().clone_into(&mut href),
                "media-type" => v.trim().clone_into(&mut media_type),
                "properties" => properties = v.trim().to_ascii_lowercase(),
                _ => {}
            }
        }
    }

    if !id.is_empty() && !href.is_empty() {
        if properties.split_whitespace().any(|p| p == "cover-image") {
            out.cover_manifest_id = Some(id.clone());
        }
        out.manifest
            .insert(id, OpfManifestItem { href, media_type });
    }
}

fn handle_opf_itemref(e: &quick_xml::events::BytesStart<'_>, out: &mut ParsedOpf) {
    let mut idref = String::new();
    let mut linear = true;

    for attr in e.attributes().flatten() {
        let k = local_xml_name(attr.key.as_ref()).to_ascii_lowercase();
        if let Ok(v) = attr.unescape_value() {
            if k == "idref" {
                v.trim().clone_into(&mut idref);
            } else if k == "linear" && v.trim().eq_ignore_ascii_case("no") {
                linear = false;
            }
        }
    }

    if !idref.is_empty() && linear {
        out.spine_idrefs.push(idref);
    }
}

fn extract_markup_image_refs(doc_bytes: &[u8]) -> Vec<String> {
    let mut reader = XmlReader::from_reader(doc_bytes);
    reader.config_mut().check_end_names = false;
    let mut buf = Vec::new();
    let mut refs = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e) | Event::Empty(e)) => {
                let local = local_xml_name(e.name().as_ref()).to_ascii_lowercase();
                if local == "img" || local == "image" {
                    for attr in e.attributes().flatten() {
                        let k = local_xml_name(attr.key.as_ref()).to_ascii_lowercase();
                        if (k == "src" || k == "href")
                            && let Ok(v) = attr.unescape_value()
                        {
                            let trimmed = v.trim();
                            if !trimmed.is_empty() && !trimmed.starts_with("data:") {
                                refs.push(trimmed.to_owned());
                            }
                        }
                    }
                }
            }
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
        buf.clear();
    }
    refs
}

fn local_xml_name(raw: &[u8]) -> String {
    let s = String::from_utf8_lossy(raw);
    s.rsplit(':').next().unwrap_or(&s).to_owned()
}

fn normalize_zip_path(raw: &str) -> String {
    let clean = raw
        .split(['?', '#'])
        .next()
        .unwrap_or(raw)
        .replace('\\', "/");
    let decoded = percent_decode_path(&clean);
    let mut parts: Vec<&str> = Vec::new();
    for seg in decoded.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                let _ = parts.pop();
            }
            other => parts.push(other),
        }
    }
    parts.join("/")
}

fn parent_dir(path: &str) -> String {
    match path.rsplit_once('/') {
        Some((dir, _)) => dir.to_owned(),
        None => String::new(),
    }
}

fn resolve_relative_zip_path(base_dir: &str, href: &str) -> String {
    if href.starts_with('/') || base_dir.is_empty() {
        normalize_zip_path(href)
    } else {
        normalize_zip_path(&format!("{base_dir}/{href}"))
    }
}

fn lookup_zip_entry(
    exact: &HashMap<String, ZipPageEntry>,
    lower: &HashMap<String, String>,
    path: &str,
) -> Option<ZipPageEntry> {
    if let Some(e) = exact.get(path) {
        return Some(e.clone());
    }
    let key = lower.get(&path.to_ascii_lowercase())?;
    exact.get(key).cloned()
}

fn percent_decode_path(raw: &str) -> String {
    let bytes = raw.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let (Some(hi), Some(lo)) = (hex_nibble(bytes[i + 1]), hex_nibble(bytes[i + 2]))
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

const fn hex_nibble(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "internal constructor assembling validated parsed chapter metadata and cover"
)]
pub(crate) fn build_parsed_chapter(
    format: ComicFormat,
    stem: &str,
    override_series: Option<&str>,
    meta: ComicMetadata,
    pages: Vec<PageDescriptor>,
    first_page_bytes: Bytes,
    first_page_mime: String,
    file_size: u64,
    file_sha_short: String,
) -> Result<ParsedComicChapter, AppError> {
    if pages.is_empty() {
        return Err(AppError::UnsupportedFormat {
            message: "comic chapter contains zero readable pages".to_owned(),
        });
    }

    let (inferred_series, inferred_chap_label, inferred_chap_num, inferred_vol) =
        infer_series_and_chapter_from_stem(stem);

    let series_title = override_series
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .or(meta.series)
        .unwrap_or(inferred_series);

    let chapter_number = meta.number.or(inferred_chap_num);
    let volume = meta.volume.or(inferred_vol);
    let chapter_title = meta
        .title
        .filter(|t| !t.trim().is_empty())
        .unwrap_or(inferred_chap_label);

    Ok(ParsedComicChapter {
        format,
        series_title,
        chapter_title,
        chapter_number,
        volume,
        summary: meta.summary,
        authors: meta.authors,
        artists: meta.artists,
        genres: meta.genres,
        language: meta.language,
        pages,
        first_page_bytes,
        first_page_mime,
        file_size,
        file_sha_short,
    })
}
