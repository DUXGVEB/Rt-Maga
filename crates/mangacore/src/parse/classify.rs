//! Classifies a parsed HTML document as a manga detail page, a chapter reading page, or unknown.

use scraper::Html;
use url::Url;

use crate::parse::chapters::extract_chapters;
use crate::parse::dom::effective_base_url;
use crate::parse::images::extract_chapter_images;
use crate::parse::nav::extract_prev_next;

/// Minimum number of images required to classify a page as a chapter page.
pub const MIN_CHAPTER_IMAGES: usize = 3;

/// Classification of an arbitrary manga website page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum PageKind {
    /// A series overview page listing chapters.
    Manga,
    /// A reading page displaying chapter images.
    Chapter,
    /// Neither a recognizable manga page nor a chapter page.
    Unknown,
}

/// Classifies `doc` based on extracted images, chapter navigation, and chapter listing heuristics.
pub fn classify_page(doc: &Html, page_url: &Url) -> PageKind {
    let base = effective_base_url(doc, page_url);
    let images = extract_chapter_images(doc, page_url, &base);
    let nav = extract_prev_next(doc, page_url, &base);

    if images.len() >= MIN_CHAPTER_IMAGES && (nav.prev.is_some() || nav.next.is_some()) {
        return PageKind::Chapter;
    }

    let chapters = extract_chapters(doc, &base);
    if !chapters.is_empty() && images.len() < MIN_CHAPTER_IMAGES {
        return PageKind::Manga;
    }

    if images.len() >= MIN_CHAPTER_IMAGES {
        return PageKind::Chapter;
    }

    if !chapters.is_empty() {
        return PageKind::Manga;
    }

    if crate::parse::gallery::looks_like_gallery_page(doc, "") {
        return PageKind::Manga;
    }

    PageKind::Unknown
}
