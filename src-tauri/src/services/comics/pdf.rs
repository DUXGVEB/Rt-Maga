//! Pure-Rust `.pdf` comic/manga parser and lazy per-page rasterizer backed by `hayro`.
//!
//! Renders individual pages on demand into crisp PNG bytes without unpacking every page into RAM,
//! using a small bounded LRU handle cache (at most 2 open PDF documents) for fast page turns.

use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, Mutex};

use bytes::Bytes;
use hayro::hayro_interpret::InterpreterSettings;
use hayro::hayro_syntax::{LoadPdfError, Pdf};
use hayro::vello_cpu::color::{AlphaColor, Srgb};
use hayro::{PixmapSettings, RenderCache, RenderSettings};
use serde::{Deserialize, Serialize};

use crate::error::AppError;
use crate::services::comics::sort::{
    ComicMetadata, MAX_COMIC_ARCHIVE_BYTES, MAX_PAGES_PER_CHAPTER, clean_stem_from_path,
    compute_fast_file_fingerprint, push_csv_unique,
};
use crate::services::comics::zip_epub::build_parsed_chapter;
use crate::services::comics::{ComicFormat, PageDescriptor, ParsedComicChapter};

/// Maximum rendered pixel edge for a single PDF page (keeps manga text sharp & RAM bounded).
const MAX_PDF_RENDER_EDGE_PX: f32 = 2200.0;

/// Default PDF render scale (`2.0` ≈ 144 DPI on standard 72 pt/in PDF pages).
const DEFAULT_PDF_SCALE: f32 = 2.0;

/// Maximum number of parsed PDF handles kept warm in memory for instant sequential page turns.
const MAX_CACHED_PDF_HANDLES: usize = 2;

type CachedPdfEntry = (PathBuf, u64, Arc<Pdf>);

static OPEN_PDF_CACHE: LazyLock<Mutex<Vec<CachedPdfEntry>>> =
    LazyLock::new(|| Mutex::new(Vec::with_capacity(MAX_CACHED_PDF_HANDLES)));

/// Indexed PDF page descriptor stored in SQLite for lazy on-demand rasterization.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PdfPageEntry {
    /// Zero-based page index inside the PDF document.
    pub page_index: usize,
    /// Page width in PDF points (`1/72` inch).
    pub width_pt: f32,
    /// Page height in PDF points (`1/72` inch).
    pub height_pt: f32,
}

/// Indexes a `.pdf` document, extracting PDF Info metadata, page dimensions, and rendering ONLY
/// page 0 for the persistent cover thumbnail.
///
/// # Errors
///
/// Returns [`AppError::EncryptedArchive`] for password-protected PDFs, [`AppError::UnsupportedFormat`]
/// for empty PDFs, or [`AppError::CorruptedData`] for unreadable PDFs.
pub(crate) fn inspect_pdf_sync(
    path: &Path,
    override_series: Option<&str>,
) -> Result<ParsedComicChapter, AppError> {
    let mut raw_file = std::fs::File::open(path).map_err(|e| AppError::NotFound {
        message: format!("cannot open PDF document: {e}"),
    })?;
    let file_size = raw_file
        .metadata()
        .map_err(|e| AppError::Internal {
            message: format!("cannot stat PDF document: {e}"),
        })?
        .len();
    if !(16..=MAX_COMIC_ARCHIVE_BYTES).contains(&file_size) {
        return Err(AppError::CorruptedData {
            message: "invalid or oversized PDF document".to_owned(),
        });
    }
    let file_sha_short = compute_fast_file_fingerprint(&mut raw_file, file_size)?;
    drop(raw_file);

    let pdf = load_pdf_uncached(path)?;
    let pdf_pages = pdf.pages();
    if pdf_pages.is_empty() {
        return Err(AppError::UnsupportedFormat {
            message: "PDF document contains zero pages".to_owned(),
        });
    }

    let mut descriptors = Vec::with_capacity(pdf_pages.len().min(MAX_PAGES_PER_CHAPTER));
    for (idx, page) in pdf_pages.iter().take(MAX_PAGES_PER_CHAPTER).enumerate() {
        let (w, h) = page.render_dimensions();
        descriptors.push(PageDescriptor::PdfPage(PdfPageEntry {
            page_index: idx,
            width_pt: w,
            height_pt: h,
        }));
    }

    let first_png = render_single_page(&pdf, 0)?;

    let mut meta = ComicMetadata::default();
    let pdf_meta = pdf.metadata();
    if let Some(title) = pdf_meta.title.as_deref().and_then(decode_pdf_text) {
        meta.series = Some(title.clone());
        meta.title = Some(title);
    }
    if let Some(author) = pdf_meta.author.as_deref().and_then(decode_pdf_text) {
        push_csv_unique(&mut meta.authors, &author);
    }
    if let Some(subject) = pdf_meta.subject.as_deref().and_then(decode_pdf_text) {
        meta.summary = Some(subject);
    }

    let stem = clean_stem_from_path(path, "PDF Comic");
    build_parsed_chapter(
        ComicFormat::Pdf,
        &stem,
        override_series,
        meta,
        descriptors,
        Bytes::from(first_png),
        "image/png".to_owned(),
        file_size,
        file_sha_short,
    )
}

/// Lazily renders a single zero-based page of a stored `.pdf` file into PNG bytes.
///
/// # Errors
///
/// Returns [`AppError`] if the PDF file is missing, encrypted, out of bounds, or fails to render.
pub(crate) fn render_pdf_page_sync(
    pdf_path: &Path,
    page_index: usize,
) -> Result<(Vec<u8>, String), AppError> {
    let pdf = load_pdf_cached(pdf_path)?;
    let png_bytes = render_single_page(&pdf, page_index)?;
    Ok((png_bytes, "image/png".to_owned()))
}

/// Evicts any cached open PDF handles pointing inside `dir_prefix` (called on permanent manga delete).
pub(crate) fn evict_cached_pdfs_for_prefix(dir_prefix: &Path) {
    if let Ok(mut guard) = OPEN_PDF_CACHE.lock() {
        guard.retain(|(p, _, _)| !p.starts_with(dir_prefix));
    }
}

fn load_pdf_cached(path: &Path) -> Result<Arc<Pdf>, AppError> {
    let file_size = std::fs::metadata(path)
        .map_err(|e| AppError::NotFound {
            message: format!("PDF file missing on disk: {e}"),
        })?
        .len();

    if let Ok(mut guard) = OPEN_PDF_CACHE.lock()
        && let Some(pos) = guard
            .iter()
            .position(|(p, sz, _)| p == path && *sz == file_size)
    {
        let entry = guard.remove(pos);
        let handle = Arc::clone(&entry.2);
        guard.push(entry);
        return Ok(handle);
    }

    let loaded = Arc::new(load_pdf_uncached(path)?);
    if let Ok(mut guard) = OPEN_PDF_CACHE.lock() {
        if guard.len() >= MAX_CACHED_PDF_HANDLES {
            let _ = guard.remove(0);
        }
        guard.push((path.to_path_buf(), file_size, Arc::clone(&loaded)));
    }
    Ok(loaded)
}

fn load_pdf_uncached(path: &Path) -> Result<Pdf, AppError> {
    let bytes = std::fs::read(path).map_err(|e| AppError::NotFound {
        message: format!("cannot read PDF file: {e}"),
    })?;
    Pdf::new(Arc::new(bytes)).map_err(|err| match err {
        LoadPdfError::Decryption(_) => AppError::EncryptedArchive {
            message: "password-protected / encrypted PDF documents are not supported".to_owned(),
        },
        LoadPdfError::Invalid => AppError::CorruptedData {
            message: "corrupted or invalid PDF document".to_owned(),
        },
    })
}

fn render_single_page(pdf: &Pdf, page_index: usize) -> Result<Vec<u8>, AppError> {
    let page = pdf
        .pages()
        .get(page_index)
        .ok_or_else(|| AppError::NotFound {
            message: format!("PDF page index {page_index} out of range"),
        })?;

    let (w, h) = page.render_dimensions();
    let max_dim = w.max(h).max(1.0);
    let scale = (MAX_PDF_RENDER_EDGE_PX / max_dim).clamp(0.5, DEFAULT_PDF_SCALE);

    let cache = RenderCache::new();
    let interpreter_settings = InterpreterSettings::default();
    let render_settings = RenderSettings::default();
    let pixmap_settings = PixmapSettings {
        x_scale: scale,
        y_scale: scale,
        bg_color: AlphaColor::<Srgb>::WHITE,
    };

    let pixmap = hayro::render(
        page,
        &cache,
        &interpreter_settings,
        &render_settings,
        &pixmap_settings,
    );

    pixmap.into_png().map_err(|e| AppError::Internal {
        message: format!("failed to encode rendered PDF page as PNG: {e}"),
    })
}

fn decode_pdf_text(raw: &[u8]) -> Option<String> {
    let text = if raw.len() >= 2 && raw[0] == 0xFE && raw[1] == 0xFF {
        // UTF-16BE with BOM
        let units: Vec<u16> = raw[2..]
            .as_chunks::<2>()
            .0
            .iter()
            .map(|c| u16::from_be_bytes(*c))
            .collect();
        String::from_utf16_lossy(&units)
    } else if raw.len() >= 2 && raw[0] == 0xFF && raw[1] == 0xFE {
        // UTF-16LE with BOM
        let units: Vec<u16> = raw[2..]
            .as_chunks::<2>()
            .0
            .iter()
            .map(|c| u16::from_le_bytes(*c))
            .collect();
        String::from_utf16_lossy(&units)
    } else {
        String::from_utf8_lossy(raw).into_owned()
    };

    let trimmed = text.trim_matches(|c: char| c == '\0' || c.is_whitespace());
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_owned())
    }
}
