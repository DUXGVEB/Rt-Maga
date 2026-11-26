//! Unified multi-format local comic/manga reader engine supporting:
//! - Comic & standard archives: `.cbz`, `.zip`, `.cbr`, `.rar`, `.cb7`, `.7z`, `.cbt`, `.tar`
//! - Fixed-layout documents & ebooks: `.epub` (OPF spine order), `.pdf` (pure-Rust lazy rasterizer)
//! - Directories (chapter folders & Mihon/Suwayomi/Kotatsu/HakuNeko series folders) & loose images
//!
//! All format routing uses binary magic-byte inspection (`infer` + header verification) rather
//! than trusting file extensions alone, and every backend extracts/renders individual pages lazily
//! on demand rather than unpacking whole archives into RAM.

pub(crate) mod folder;
pub(crate) mod importer;
pub(crate) mod pdf;
pub(crate) mod rar;
pub(crate) mod sevenz;
pub(crate) mod sort;
pub(crate) mod tar_src;
pub(crate) mod zip_epub;

use std::io::Read;
use std::path::{Path, PathBuf};

use bytes::Bytes;
use mangacore::inspect_image_bytes;
use serde::{Deserialize, Serialize};

use crate::error::AppError;
use crate::services::comics::folder::LocalImageEntry;
use crate::services::comics::pdf::{PdfPageEntry, inspect_pdf_sync, render_pdf_page_sync};
use crate::services::comics::rar::{inspect_rar_sync, read_rar_entry_sync};
use crate::services::comics::sevenz::{inspect_sevenz_sync, read_sevenz_entry_sync};
use crate::services::comics::sort::ComicMetadata;
use crate::services::comics::tar_src::{TarPageEntry, inspect_tar_sync, looks_like_tar_header};
use crate::services::comics::zip_epub::{
    ZipPageEntry, inspect_epub_sync, inspect_zip_sync, is_zip_epub_sync,
};

pub(crate) use importer::{
    append_comic_staging_chunk, begin_comic_staging, delete_local_files_for_manga,
    finish_comic_staging_import, import_comic_paths, load_local_chapter,
    pick_and_import_comics_windows, read_local_page_bytes,
};

/// Canonical container/source format persisted in `local_comic_chapter.source_format`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub(crate) enum ComicFormat {
    Zip,
    Rar,
    SevenZ,
    Tar,
    Epub,
    Pdf,
    Folder,
}

impl ComicFormat {
    /// Storage key written to `local_comic_chapter.source_format`.
    #[must_use]
    pub(crate) const fn as_storage_str(self) -> &'static str {
        match self {
            Self::Zip => "zip",
            Self::Rar => "rar",
            Self::SevenZ => "sevenz",
            Self::Tar => "tar",
            Self::Epub => "epub",
            Self::Pdf => "pdf",
            Self::Folder => "folder",
        }
    }

    /// Parses a stored `local_comic_chapter.source_format` string (defaulting unknown legacy rows to `Zip`).
    #[must_use]
    pub(crate) fn from_storage_str(raw: &str) -> Self {
        match raw.trim().to_ascii_lowercase().as_str() {
            "rar" | "cbr" => Self::Rar,
            "sevenz" | "7z" | "cb7" => Self::SevenZ,
            "tar" | "cbt" => Self::Tar,
            "epub" => Self::Epub,
            "pdf" => Self::Pdf,
            "folder" | "images" => Self::Folder,
            _ => Self::Zip,
        }
    }

    /// File extension used when copying an archive/document into `<data_dir>/comics/`.
    #[must_use]
    pub(crate) const fn storage_extension(self) -> &'static str {
        match self {
            Self::Zip => "cbz",
            Self::Rar => "cbr",
            Self::SevenZ => "cb7",
            Self::Tar => "cbt",
            Self::Epub => "epub",
            Self::Pdf => "pdf",
            Self::Folder => "dir",
        }
    }
}

/// Format detected purely from filesystem type and binary magic headers (ignoring extension spoofing).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub(crate) enum DetectedFileKind {
    Directory,
    Zip,
    Epub,
    Rar,
    SevenZ,
    Tar,
    Pdf,
    Image,
}

/// Entry descriptor for RAR/7z archives indexed by name and entry index.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ArchivePageEntry {
    pub name: String,
    pub index: usize,
    pub uncompressed_size: u64,
}

/// Polymorphic page descriptor stored in `local_comic_chapter.page_entries_json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum PageDescriptor {
    ZipEntry(ZipPageEntry),
    TarEntry(TarPageEntry),
    ArchiveEntry(ArchivePageEntry),
    PdfPage(PdfPageEntry),
    LocalFile(LocalImageEntry),
}

/// Unified parsed chapter output returned by every format indexer before persistence.
#[derive(Debug)]
pub(crate) struct ParsedComicChapter {
    pub format: ComicFormat,
    pub series_title: String,
    pub chapter_title: String,
    pub chapter_number: Option<f32>,
    pub volume: Option<String>,
    pub summary: Option<String>,
    pub authors: Vec<String>,
    pub artists: Vec<String>,
    pub genres: Vec<String>,
    pub language: Option<String>,
    pub pages: Vec<PageDescriptor>,
    pub first_page_bytes: Bytes,
    pub first_page_mime: String,
    pub file_size: u64,
    pub file_sha_short: String,
}

/// Unified abstraction implemented by local comic sources for lazy on-demand page streaming.
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "unified ComicSource trait exercised by tests and direct sync source readers"
    )
)]
pub(crate) trait ComicSource: Send + Sync {
    /// Detected container/source format.
    fn format(&self) -> ComicFormat;
    /// Extracted comic metadata (`ComicInfo.xml`, OPF, PDF Info, or `details.json`).
    fn metadata(&self) -> &ComicMetadata;
    /// Indexed page descriptors in exact reading order.
    fn page_entries(&self) -> &[PageDescriptor];
    /// Lazily decodes/extracts/renders a single page at `index` into `(image_bytes, mime_type)`.
    ///
    /// # Errors
    ///
    /// Returns [`AppError`] if `index` is out of range or page decoding fails.
    fn read_page(&self, index: usize) -> Result<(Vec<u8>, String), AppError>;
}

/// Concrete [`ComicSource`] backed by a file or folder path plus its pre-indexed [`PageDescriptor`] list.
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "concrete ComicSource implementation used in unit tests and sync inspection"
    )
)]
#[derive(Debug)]
pub(crate) struct LocalComicSource {
    format: ComicFormat,
    source_path: PathBuf,
    metadata: ComicMetadata,
    pages: Vec<PageDescriptor>,
}

#[cfg_attr(
    not(test),
    expect(dead_code, reason = "constructor for LocalComicSource trait impl")
)]
impl LocalComicSource {
    /// Opens and indexes a local comic archive/document via magic-byte detection.
    ///
    /// # Errors
    ///
    /// Returns [`AppError`] on unsupported, encrypted, or corrupted input.
    pub(crate) fn open(path: &Path) -> Result<Self, AppError> {
        let parsed = inspect_single_archive_sync(path, None)?;
        let metadata = ComicMetadata {
            series: Some(parsed.series_title),
            title: Some(parsed.chapter_title),
            number: parsed.chapter_number,
            volume: parsed.volume,
            summary: parsed.summary,
            authors: parsed.authors,
            artists: parsed.artists,
            genres: parsed.genres,
            language: parsed.language,
        };
        Ok(Self {
            format: parsed.format,
            source_path: path.to_path_buf(),
            metadata,
            pages: parsed.pages,
        })
    }
}

impl ComicSource for LocalComicSource {
    fn format(&self) -> ComicFormat {
        self.format
    }

    fn metadata(&self) -> &ComicMetadata {
        &self.metadata
    }

    fn page_entries(&self) -> &[PageDescriptor] {
        &self.pages
    }

    fn read_page(&self, index: usize) -> Result<(Vec<u8>, String), AppError> {
        let desc = self.pages.get(index).ok_or_else(|| AppError::NotFound {
            message: format!("comic page index {index} out of range"),
        })?;
        match desc {
            PageDescriptor::PdfPage(entry) => {
                render_pdf_page_sync(&self.source_path, entry.page_index)
            }
            PageDescriptor::ArchiveEntry(entry) => match self.format {
                ComicFormat::Rar => read_rar_entry_sync(&self.source_path, entry.index),
                ComicFormat::SevenZ => read_sevenz_entry_sync(&self.source_path, &entry.name),
                _ => Err(AppError::UnsupportedFormat {
                    message: "unexpected archive page entry format".to_owned(),
                }),
            },
            PageDescriptor::ZipEntry(entry) => {
                let rt = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .map_err(|e| AppError::Internal {
                        message: e.to_string(),
                    })?;
                rt.block_on(zip_epub::read_zip_entry_async(
                    &self.source_path,
                    entry.clone(),
                ))
            }
            PageDescriptor::TarEntry(entry) => {
                let rt = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .map_err(|e| AppError::Internal {
                        message: e.to_string(),
                    })?;
                rt.block_on(tar_src::read_tar_entry_async(
                    &self.source_path,
                    entry.clone(),
                ))
            }
            PageDescriptor::LocalFile(entry) => {
                let rt = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .map_err(|e| AppError::Internal {
                        message: e.to_string(),
                    })?;
                rt.block_on(folder::read_local_image_async(
                    &self.source_path,
                    &entry.rel_name,
                ))
            }
        }
    }
}

/// Returns `true` when `url` is a local comic URL (`https://comic.local/...` or legacy `https://cbz.local/...`).
#[must_use]
pub(crate) fn is_local_comic_url(url: &str) -> bool {
    url.starts_with("https://comic.local/") || url.starts_with("https://cbz.local/")
}

/// Returns `true` when a manga's `site` key denotes locally imported offline content (`local:*`).
#[must_use]
pub(crate) fn is_local_site(site: &str) -> bool {
    site.starts_with("local:")
}

/// Deserializes `local_comic_chapter.page_entries_json`, seamlessly accepting both tagged
/// [`PageDescriptor`] arrays and legacy `v3.0.x` untagged [`ZipPageEntry`] arrays.
///
/// # Errors
///
/// Returns [`AppError::CorruptedData`] if the JSON cannot be parsed as either schema.
pub(crate) fn deserialize_page_descriptors(json: &str) -> Result<Vec<PageDescriptor>, AppError> {
    if let Ok(tagged) = serde_json::from_str::<Vec<PageDescriptor>>(json) {
        return Ok(tagged);
    }
    if let Ok(legacy_zip) = serde_json::from_str::<Vec<ZipPageEntry>>(json) {
        return Ok(legacy_zip
            .into_iter()
            .map(PageDescriptor::ZipEntry)
            .collect());
    }
    Err(AppError::CorruptedData {
        message: "corrupted local comic page index".to_owned(),
    })
}

/// Detects the true binary format of `path` using magic-byte inspection (`infer` + container
/// header checks), never trusting file extensions alone.
///
/// # Errors
///
/// Returns [`AppError::NotFound`] if the path does not exist, [`AppError::CorruptedData`] on
/// unreadable/empty files, or [`AppError::UnsupportedFormat`] for unsupported file types.
pub(crate) fn detect_format_magic(path: &Path) -> Result<DetectedFileKind, AppError> {
    let meta = std::fs::metadata(path).map_err(|e| AppError::NotFound {
        message: format!("cannot access selected path: {e}"),
    })?;
    if meta.is_dir() {
        return Ok(DetectedFileKind::Directory);
    }
    if meta.len() == 0 {
        return Err(AppError::CorruptedData {
            message: "selected file is empty (0 bytes)".to_owned(),
        });
    }

    let mut file = std::fs::File::open(path).map_err(|e| AppError::NotFound {
        message: format!("cannot open selected file: {e}"),
    })?;
    let mut header = [0_u8; 4096];
    let n = file
        .read(&mut header)
        .map_err(|e| AppError::CorruptedData {
            message: format!("cannot read file header: {e}"),
        })?;
    let slice = &header[..n];
    drop(file);

    // 1. Check explicit magic signatures + `infer` classification.
    if let Some(kind) = sniff_header_magic_bytes(slice) {
        if kind == DetectedFileKind::Zip && is_zip_epub_sync(path) {
            return Ok(DetectedFileKind::Epub);
        }
        return Ok(kind);
    }

    Err(AppError::UnsupportedFormat {
        message: format!(
            "unsupported file format for '{}'; expected CBZ/ZIP, CBR/RAR, CB7/7z, CBT/TAR, EPUB, PDF, or raster images",
            path.file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("selected file")
        ),
    })
}

/// Pure magic-byte header classifier (independent of filename extension).
#[must_use]
pub(crate) fn sniff_header_magic_bytes(header: &[u8]) -> Option<DetectedFileKind> {
    if header.len() >= 4 && header[0..4] == [0x50, 0x4B, 0x03, 0x04] {
        return Some(DetectedFileKind::Zip);
    }
    // RAR 1.5–4.x (`Rar!\x1A\x07\x00`) and RAR 5.0+ (`Rar!\x1A\x07\x01\x00`)
    if header.len() >= 7
        && header[0..6] == [0x52, 0x61, 0x72, 0x21, 0x1A, 0x07]
        && (header[6] == 0x00 || (header.len() >= 8 && header[6] == 0x01 && header[7] == 0x00))
    {
        return Some(DetectedFileKind::Rar);
    }
    // 7-Zip (`7z\xBC\xAF\x27\x1C`)
    if header.len() >= 6 && header[0..6] == [0x37, 0x7A, 0xBC, 0xAF, 0x27, 0x1C] {
        return Some(DetectedFileKind::SevenZ);
    }
    // PDF (`%PDF-`, allowing leading whitespace or small preamble up to 1024 bytes per PDF spec)
    let pdf_window = &header[..header.len().min(1024)];
    if pdf_window.windows(5).any(|w| w == b"%PDF-") {
        return Some(DetectedFileKind::Pdf);
    }

    if let Some(inferred) = infer::get(header) {
        match inferred.mime_type() {
            "application/epub+zip" => return Some(DetectedFileKind::Epub),
            "application/zip" => return Some(DetectedFileKind::Zip),
            "application/vnd.rar" | "application/x-rar-compressed" => {
                return Some(DetectedFileKind::Rar);
            }
            "application/x-7z-compressed" => return Some(DetectedFileKind::SevenZ),
            "application/x-tar" => return Some(DetectedFileKind::Tar),
            "application/pdf" => return Some(DetectedFileKind::Pdf),
            "image/jpeg" | "image/png" | "image/webp" | "image/gif" | "image/avif"
            | "image/bmp" => return Some(DetectedFileKind::Image),
            _ => {}
        }
    }

    if looks_like_tar_header(header) {
        return Some(DetectedFileKind::Tar);
    }

    if inspect_image_bytes(header).is_ok() {
        return Some(DetectedFileKind::Image);
    }

    None
}

/// Inspects a single archive or document file via magic-byte routing (never relying on extension).
///
/// # Errors
///
/// Returns [`AppError`] if the file is unsupported, encrypted, or corrupted.
pub(crate) fn inspect_single_archive_sync(
    path: &Path,
    override_series: Option<&str>,
) -> Result<ParsedComicChapter, AppError> {
    match detect_format_magic(path)? {
        DetectedFileKind::Zip => inspect_zip_sync(path, override_series),
        DetectedFileKind::Epub => inspect_epub_sync(path, override_series),
        DetectedFileKind::Rar => inspect_rar_sync(path, override_series),
        DetectedFileKind::SevenZ => inspect_sevenz_sync(path, override_series),
        DetectedFileKind::Tar => inspect_tar_sync(path, override_series),
        DetectedFileKind::Pdf => inspect_pdf_sync(path, override_series),
        DetectedFileKind::Directory | DetectedFileKind::Image => Err(AppError::UnsupportedFormat {
            message: "expected an archive or document file".to_owned(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::comics::sort::{
        infer_series_and_chapter_from_stem, is_junk_entry, natural_cmp, parse_comic_info_xml,
    };

    /// Minimal valid 16x16 PNG image bytes (for synthetic archive/folder tests).
    fn sample_png_bytes() -> Vec<u8> {
        let mut pixmap = hayro::vello_cpu::Pixmap::new(16, 16);
        pixmap
            .data_mut()
            .fill(hayro::vello_cpu::color::PremulRgba8 {
                r: 220,
                g: 220,
                b: 220,
                a: 255,
            });
        pixmap.into_png().expect("sample png encoding must succeed")
    }

    /// Builds a valid uncompressed (Store method = 0) ZIP archive in memory.
    fn build_store_zip(entries: &[(&str, &[u8])], flags: u16) -> Vec<u8> {
        let mut out = Vec::new();
        let mut central_dir = Vec::new();

        for (name, data) in entries {
            let offset = u32::try_from(out.len()).expect("zip offset fits u32");
            let name_bytes = name.as_bytes();
            let name_len = u16::try_from(name_bytes.len()).expect("name len fits u16");
            let data_len = u32::try_from(data.len()).expect("data len fits u32");
            let mut hasher = crc32fast::Hasher::new();
            hasher.update(data);
            let crc = hasher.finalize();

            // Local file header
            out.extend_from_slice(&[0x50, 0x4B, 0x03, 0x04]);
            out.extend_from_slice(&20_u16.to_le_bytes());
            out.extend_from_slice(&flags.to_le_bytes());
            out.extend_from_slice(&0_u16.to_le_bytes()); // Store
            out.extend_from_slice(&0_u16.to_le_bytes());
            out.extend_from_slice(&0_u16.to_le_bytes());
            out.extend_from_slice(&crc.to_le_bytes());
            out.extend_from_slice(&data_len.to_le_bytes());
            out.extend_from_slice(&data_len.to_le_bytes());
            out.extend_from_slice(&name_len.to_le_bytes());
            out.extend_from_slice(&0_u16.to_le_bytes());
            out.extend_from_slice(name_bytes);
            out.extend_from_slice(data);

            // Central directory header
            central_dir.extend_from_slice(&[0x50, 0x4B, 0x01, 0x02]);
            central_dir.extend_from_slice(&20_u16.to_le_bytes());
            central_dir.extend_from_slice(&20_u16.to_le_bytes());
            central_dir.extend_from_slice(&flags.to_le_bytes());
            central_dir.extend_from_slice(&0_u16.to_le_bytes());
            central_dir.extend_from_slice(&0_u16.to_le_bytes());
            central_dir.extend_from_slice(&0_u16.to_le_bytes());
            central_dir.extend_from_slice(&crc.to_le_bytes());
            central_dir.extend_from_slice(&data_len.to_le_bytes());
            central_dir.extend_from_slice(&data_len.to_le_bytes());
            central_dir.extend_from_slice(&name_len.to_le_bytes());
            central_dir.extend_from_slice(&0_u16.to_le_bytes());
            central_dir.extend_from_slice(&0_u16.to_le_bytes());
            central_dir.extend_from_slice(&0_u16.to_le_bytes());
            central_dir.extend_from_slice(&0_u16.to_le_bytes());
            central_dir.extend_from_slice(&0_u32.to_le_bytes());
            central_dir.extend_from_slice(&offset.to_le_bytes());
            central_dir.extend_from_slice(name_bytes);
        }

        let cd_offset = u32::try_from(out.len()).expect("cd offset fits u32");
        let cd_size = u32::try_from(central_dir.len()).expect("cd size fits u32");
        let count = u16::try_from(entries.len()).expect("entry count fits u16");
        out.extend_from_slice(&central_dir);

        // End of Central Directory
        out.extend_from_slice(&[0x50, 0x4B, 0x05, 0x06]);
        out.extend_from_slice(&0_u16.to_le_bytes());
        out.extend_from_slice(&0_u16.to_le_bytes());
        out.extend_from_slice(&count.to_le_bytes());
        out.extend_from_slice(&count.to_le_bytes());
        out.extend_from_slice(&cd_size.to_le_bytes());
        out.extend_from_slice(&cd_offset.to_le_bytes());
        out.extend_from_slice(&0_u16.to_le_bytes());
        out
    }

    /// Builds a valid minimal single-page PDF document with valid xref offsets.
    fn build_minimal_pdf() -> Vec<u8> {
        let mut pdf = b"%PDF-1.4\n".to_vec();
        let mut offsets = Vec::new();

        let objs: &[&[u8]] = &[
            b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n",
            b"2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n",
            b"3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 100 140] /Contents 4 0 R >>\nendobj\n",
            b"4 0 obj\n<< /Length 28 >>\nstream\n0.2 0.4 0.8 rg 10 10 80 120 re f\nendstream\nendobj\n",
            b"5 0 obj\n<< /Title (Synthetic Manga PDF) /Author (Test Author) >>\nendobj\n",
        ];

        for obj in objs {
            offsets.push(pdf.len());
            pdf.extend_from_slice(obj);
        }

        let xref_offset = pdf.len();
        pdf.extend_from_slice(b"xref\n0 6\n0000000000 65535 f \n");
        for off in offsets {
            pdf.extend_from_slice(format!("{off:010} 00000 n \n").as_bytes());
        }
        pdf.extend_from_slice(
            format!(
                "trailer\n<< /Size 6 /Root 1 0 R /Info 5 0 R >>\nstartxref\n{xref_offset}\n%%EOF\n"
            )
            .as_bytes(),
        );
        pdf
    }

    #[test]
    fn magic_byte_detection_handles_all_formats_and_mislabeled_extensions() {
        assert_eq!(
            sniff_header_magic_bytes(&[0x50, 0x4B, 0x03, 0x04, 0, 0]),
            Some(DetectedFileKind::Zip),
            "ZIP magic must be detected"
        );
        assert_eq!(
            sniff_header_magic_bytes(&[0x52, 0x61, 0x72, 0x21, 0x1A, 0x07, 0x00, 0x00]),
            Some(DetectedFileKind::Rar),
            "RAR4 magic must be detected"
        );
        assert_eq!(
            sniff_header_magic_bytes(&[0x52, 0x61, 0x72, 0x21, 0x1A, 0x07, 0x01, 0x00]),
            Some(DetectedFileKind::Rar),
            "RAR5 magic must be detected"
        );
        assert_eq!(
            sniff_header_magic_bytes(&[0x37, 0x7A, 0xBC, 0xAF, 0x27, 0x1C, 0x00, 0x04]),
            Some(DetectedFileKind::SevenZ),
            "7z magic must be detected"
        );
        assert_eq!(
            sniff_header_magic_bytes(b"%PDF-1.7\n"),
            Some(DetectedFileKind::Pdf),
            "PDF magic must be detected"
        );
        let png = sample_png_bytes();
        assert_eq!(
            sniff_header_magic_bytes(&png),
            Some(DetectedFileKind::Image),
            "PNG magic must be detected as image"
        );

        // Verify mislabeled file on disk: RAR5 bytes saved with a `.cbz` extension MUST route to Rar.
        let dir = std::env::temp_dir().join(format!("rtmaga_magic_test_{}", rand::random::<u64>()));
        std::fs::create_dir_all(&dir).expect("temp dir creation succeeds");
        let fake_cbz = dir.join("actually_rar.cbz");
        std::fs::write(
            &fake_cbz,
            [0x52_u8, 0x61, 0x72, 0x21, 0x1A, 0x07, 0x01, 0x00, 0x00],
        )
        .expect("write mislabeled rar succeeds");
        assert_eq!(
            detect_format_magic(&fake_cbz).expect("magic detection must succeed"),
            DetectedFileKind::Rar,
            "RAR5 bytes named .cbz must still detect as Rar"
        );

        let fake_7z = dir.join("actually_7z.cbz");
        std::fs::write(
            &fake_7z,
            [0x37_u8, 0x7A, 0xBC, 0xAF, 0x27, 0x1C, 0x00, 0x04],
        )
        .expect("write mislabeled 7z succeeds");
        assert_eq!(
            detect_format_magic(&fake_7z).expect("7z magic detection must succeed"),
            DetectedFileKind::SevenZ,
            "7z bytes named .cbz must still detect as SevenZ"
        );

        let txt_file = dir.join("not_a_comic.cbz");
        std::fs::write(&txt_file, b"plain text not an archive").expect("write txt succeeds");
        assert!(
            matches!(
                detect_format_magic(&txt_file),
                Err(AppError::UnsupportedFormat { .. })
            ),
            "plain text misnamed .cbz must return UnsupportedFormat"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn natural_sorting_junk_filtering_and_comicinfo_work() {
        let mut names = vec!["page_10.jpg", "page_2.jpg", "page_1.jpg", "page_20.jpg"];
        names.sort_by(|a, b| natural_cmp(a, b));
        assert_eq!(
            names,
            vec!["page_1.jpg", "page_2.jpg", "page_10.jpg", "page_20.jpg"],
            "natural_cmp must sort numeric suffixes numerically"
        );

        assert!(
            is_junk_entry("__MACOSX/page_1.jpg"),
            "__MACOSX must be filtered"
        );
        assert!(
            is_junk_entry("chapter/.DS_Store"),
            ".DS_Store must be filtered"
        );
        assert!(is_junk_entry("Thumbs.db"), "Thumbs.db must be filtered");
        assert!(
            is_junk_entry("folder/._001.jpg"),
            "AppleDouble files must be filtered"
        );
        assert!(
            is_junk_entry("../escape.jpg"),
            "path traversal must be filtered"
        );
        assert!(
            !is_junk_entry("ch01/page_2.webp"),
            "valid image paths must be kept"
        );

        let (series, chap, num, vol) = infer_series_and_chapter_from_stem("Solo_Leveling_Ch_12.5");
        assert_eq!(series, "Solo Leveling", "series title extracted");
        assert_eq!(chap, "Chapter 12.5", "chapter label formatted");
        assert_eq!(num, Some(12.5), "decimal chapter number parsed");
        assert_eq!(vol, None, "volume absent");

        let ci = parse_comic_info_xml(
            br"<ComicInfo><Series>Demo</Series><Number>4</Number><Writer>Author A</Writer><Genre>Action, Drama</Genre></ComicInfo>",
        );
        assert_eq!(
            ci.series.as_deref(),
            Some("Demo"),
            "ComicInfo series parsed"
        );
        assert_eq!(ci.number, Some(4.0), "ComicInfo number parsed");
        assert_eq!(ci.authors, vec!["Author A"], "ComicInfo writer parsed");
        assert_eq!(
            ci.genres,
            vec!["Action", "Drama"],
            "ComicInfo genres parsed"
        );
    }

    #[test]
    #[expect(
        clippy::too_many_lines,
        reason = "end-to-end integration test covering CBZ, EPUB, TAR, PDF, folder, and legacy JSON"
    )]
    fn cbz_epub_tar_pdf_and_folder_pipelines_work_end_to_end() {
        let dir = std::env::temp_dir().join(format!("rtmaga_multi_test_{}", rand::random::<u64>()));
        std::fs::create_dir_all(&dir).expect("temp dir creation succeeds");
        let png = sample_png_bytes();

        // 1. CBZ with ComicInfo.xml + junk files + natural ordering + encrypted rejection
        let cbz_bytes = build_store_zip(
            &[
                ("__MACOSX/._page_1.png", b"junk"),
                ("Thumbs.db", b"junk"),
                ("page_10.png", &png),
                ("page_2.png", &png),
                (
                    "ComicInfo.xml",
                    b"<ComicInfo><Series>My CBZ</Series><Number>3</Number></ComicInfo>",
                ),
            ],
            0,
        );
        let cbz_path = dir.join("test_cbz.cbz");
        std::fs::write(&cbz_path, &cbz_bytes).expect("write cbz succeeds");
        let src_cbz = LocalComicSource::open(&cbz_path).expect("CBZ open succeeds");
        assert_eq!(src_cbz.format(), ComicFormat::Zip, "CBZ detects as Zip");
        assert_eq!(
            src_cbz.page_entries().len(),
            2,
            "CBZ filters junk and keeps 2 pages"
        );
        assert_eq!(
            src_cbz.metadata().series.as_deref(),
            Some("My CBZ"),
            "ComicInfo series preserved"
        );
        let (p0, mime0) = src_cbz.read_page(0).expect("CBZ page 0 reads");
        assert_eq!(mime0, "image/png", "CBZ page mime detected");
        assert!(!p0.is_empty(), "CBZ page bytes non-empty");

        let enc_zip = build_store_zip(&[("page_1.png", &png)], 0x0001);
        let enc_path = dir.join("encrypted.cbz");
        std::fs::write(&enc_path, &enc_zip).expect("write encrypted zip succeeds");
        assert!(
            matches!(
                inspect_single_archive_sync(&enc_path, None),
                Err(AppError::EncryptedArchive { .. })
            ),
            "encrypted ZIP must return AppError::EncryptedArchive"
        );

        // 2. EPUB with META-INF/container.xml, OPF metadata, and non-alphabetical spine order
        let epub_bytes = build_store_zip(
            &[
                ("mimetype", b"application/epub+zip"),
                (
                    "META-INF/container.xml",
                    br#"<?xml version="1.0"?><container version="1.0"><rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles></container>"#,
                ),
                (
                    "OEBPS/content.opf",
                    br#"<?xml version="1.0"?><package><metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:title>Epub Manga Vol 1</dc:title><dc:creator>Creator X</dc:creator></metadata><manifest><item id="p1" href="text/first.xhtml" media-type="application/xhtml+xml"/><item id="p2" href="text/second.xhtml" media-type="application/xhtml+xml"/><item id="img_z" href="images/z_first.png" media-type="image/png"/><item id="img_a" href="images/a_second.png" media-type="image/png"/></manifest><spine><itemref idref="p1"/><itemref idref="p2"/></spine></package>"#,
                ),
                (
                    "OEBPS/text/first.xhtml",
                    br#"<html><body><img src="../images/z_first.png"/></body></html>"#,
                ),
                (
                    "OEBPS/text/second.xhtml",
                    br#"<html><body><svg><image xlink:href="../images/a_second.png"/></svg></body></html>"#,
                ),
                ("OEBPS/images/z_first.png", &png),
                ("OEBPS/images/a_second.png", &png),
            ],
            0,
        );
        let epub_path = dir.join("comic.epub");
        std::fs::write(&epub_path, &epub_bytes).expect("write epub succeeds");
        assert_eq!(
            detect_format_magic(&epub_path).expect("epub magic succeeds"),
            DetectedFileKind::Epub,
            "ZIP with container.xml detects as Epub"
        );
        let src_epub = LocalComicSource::open(&epub_path).expect("EPUB open succeeds");
        assert_eq!(
            src_epub.format(),
            ComicFormat::Epub,
            "EPUB format preserved"
        );
        assert_eq!(
            src_epub.metadata().series.as_deref(),
            Some("Epub Manga Vol 1"),
            "OPF dc:title mapped"
        );
        assert_eq!(
            src_epub.page_entries().len(),
            2,
            "EPUB extracts 2 spine images"
        );
        if let PageDescriptor::ZipEntry(first_entry) = &src_epub.page_entries()[0] {
            assert_eq!(
                first_entry.name, "OEBPS/images/z_first.png",
                "EPUB preserves exact spine reading order (z_first before a_second)"
            );
        } else {
            panic!("EPUB page descriptor must be ZipEntry");
        }

        // 3. TAR / CBT archive with O(1) offset reads
        let tar_path = dir.join("chapter_1.cbt");
        {
            let f = std::fs::File::create(&tar_path).expect("create tar succeeds");
            let mut builder = tar::Builder::new(f);
            let mut hdr1 = tar::Header::new_gnu();
            hdr1.set_size(u64::try_from(png.len()).expect("png len fits u64"));
            hdr1.set_cksum();
            builder
                .append_data(&mut hdr1, "page_10.png", png.as_slice())
                .expect("append tar page 10");
            let mut hdr2 = tar::Header::new_gnu();
            hdr2.set_size(u64::try_from(png.len()).expect("png len fits u64"));
            hdr2.set_cksum();
            builder
                .append_data(&mut hdr2, "page_2.png", png.as_slice())
                .expect("append tar page 2");
            builder.finish().expect("finish tar");
        }
        let src_tar = LocalComicSource::open(&tar_path).expect("CBT/TAR open succeeds");
        assert_eq!(src_tar.format(), ComicFormat::Tar, "CBT detects as Tar");
        assert_eq!(src_tar.page_entries().len(), 2, "TAR indexes 2 pages");
        let (tar_p0, tar_mime) = src_tar.read_page(0).expect("TAR O(1) page 0 reads");
        assert_eq!(tar_mime, "image/png", "TAR page mime valid");
        assert_eq!(tar_p0, png, "TAR O(1) bytes match exact payload");

        // 4. Synthetic PDF rendered lazily via hayro
        let pdf_path = dir.join("sample_comic.pdf");
        std::fs::write(&pdf_path, build_minimal_pdf()).expect("write pdf succeeds");
        let src_pdf = LocalComicSource::open(&pdf_path).expect("PDF open succeeds");
        assert_eq!(src_pdf.format(), ComicFormat::Pdf, "PDF detects as Pdf");
        assert_eq!(
            src_pdf.metadata().series.as_deref(),
            Some("Synthetic Manga PDF"),
            "PDF Info title extracted"
        );
        assert_eq!(src_pdf.page_entries().len(), 1, "PDF indexes 1 page");
        let (pdf_png, pdf_mime) = src_pdf.read_page(0).expect("PDF page 0 renders lazily");
        assert_eq!(pdf_mime, "image/png", "PDF rendered as PNG");
        assert!(
            inspect_image_bytes(&pdf_png).is_ok(),
            "rendered PDF bytes must be a valid PNG"
        );

        // 5. Mihon/Suwayomi series directory with `details.json` and 2 chapter subfolders
        let series_dir = dir.join("Mihon_Series");
        let ch1_dir = series_dir.join("Chapter 1");
        let ch2_dir = series_dir.join("Chapter 2");
        std::fs::create_dir_all(&ch1_dir).expect("create ch1 dir");
        std::fs::create_dir_all(&ch2_dir).expect("create ch2 dir");
        std::fs::write(
            series_dir.join("details.json"),
            br#"{"title":"Mihon Exported","author":"Author M","genre":["Adventure"]}"#,
        )
        .expect("write details.json");
        std::fs::write(ch1_dir.join("02.png"), &png).expect("write ch1 p2");
        std::fs::write(ch1_dir.join("10.png"), &png).expect("write ch1 p10");
        std::fs::write(ch1_dir.join(".DS_Store"), b"junk").expect("write junk");
        std::fs::write(ch2_dir.join("01.png"), &png).expect("write ch2 p1");

        let units = folder::expand_directory_sync(&series_dir, None).expect("expand dir succeeds");
        assert_eq!(
            units.len(),
            2,
            "series directory expands into 2 chapter batches"
        );

        // 6. Legacy CbzPageEntry JSON backward-compatibility check
        let legacy_json = r#"[{"name":"001.jpg","method":0,"localHeaderOffset":0,"compressedSize":10,"uncompressedSize":10,"crc32":123}]"#;
        let decoded =
            deserialize_page_descriptors(legacy_json).expect("legacy CBZ JSON must deserialize");
        assert_eq!(
            decoded.len(),
            1,
            "legacy CBZ JSON yields 1 ZipEntry descriptor"
        );
        assert!(
            matches!(decoded[0], PageDescriptor::ZipEntry(_)),
            "legacy CBZ JSON maps to PageDescriptor::ZipEntry"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn migration_0008_migrates_legacy_cbz_and_supports_upsert() {
        let db = crate::repo::Database::open_in_memory()
            .await
            .expect("in-memory DB opens with migration 0008");
        let manga = mangacore::Manga::minimal(
            url::Url::parse("https://comic.local/series/demo-01").expect("valid url"),
            mangacore::SiteId::known("local:comic"),
            "Local Demo".to_owned(),
            None,
            vec![mangacore::ChapterRef::simple(
                url::Url::parse("https://comic.local/chapter/demo-01/abc123").expect("valid url"),
                "Chapter 1".to_owned(),
                Some(1.0),
            )],
        );
        let manga_id = db.upsert_manga(&manga).await.expect("upsert local manga");
        db.upsert_local_comic_chapter(
            manga_id,
            "https://comic.local/chapter/demo-01/abc123",
            "pdf",
            "demo-01/abc123.pdf",
            r#"[{"kind":"pdf_page","pageIndex":0,"widthPt":100.0,"heightPt":140.0}]"#,
            2048,
        )
        .await
        .expect("initial upsert_local_comic_chapter succeeds");

        // Re-upserting same chapter_url must update cleanly via ON CONFLICT(chapter_url).
        db.upsert_local_comic_chapter(
            manga_id,
            "https://comic.local/chapter/demo-01/abc123",
            "epub",
            "demo-01/abc123.epub",
            "[]",
            4096,
        )
        .await
        .expect("re-upsert on same chapter_url succeeds");

        let row = db
            .get_local_comic_chapter_by_url("https://comic.local/chapter/demo-01/abc123")
            .await
            .expect("query succeeds")
            .expect("row exists");
        assert_eq!(row.0, manga_id, "manga_id matches");
        assert_eq!(row.1, "epub", "updated source_format persisted");
        assert_eq!(row.2, "demo-01/abc123.epub", "updated rel_path persisted");

        // Local manga must be excluded from remote background update targets.
        let targets = db.list_targets_for_updates().await.expect("list targets");
        assert!(
            targets.is_empty(),
            "local:comic manga must be excluded from remote update polling"
        );
    }
}
