//! Metadata extraction (series title and cover image) from arbitrary manga HTML pages.

use std::sync::LazyLock;

use scraper::{Html, Selector};
use url::Url;

use crate::parse::dom::{abs_url, clean_text, image_candidates, strip_site_suffix};

static OG_TITLE_SEL: LazyLock<Selector> = LazyLock::new(|| {
    Selector::parse(r#"meta[property="og:title"], meta[name="twitter:title"]"#)
        .expect("static selector is valid")
});
static H1_SEL: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("h1").expect("static selector is valid"));
static TITLE_SEL: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("title").expect("static selector is valid"));
static OG_IMAGE_SEL: LazyLock<Selector> = LazyLock::new(|| {
    Selector::parse(r#"meta[property="og:image"], meta[name="twitter:image"]"#)
        .expect("static selector is valid")
});
static COVER_IMG_SEL: LazyLock<Selector> = LazyLock::new(|| {
    Selector::parse(
        ".book_avatar img, .detail-info img, .cover img, .summary_image img, [itemprop='image']",
    )
    .expect("static selector is valid")
});

/// Extracts the most likely manga title using `h1`, `og:title`, then `<title>`.
pub fn extract_title(doc: &Html) -> Option<String> {
    if let Some(h1) = doc.select(&H1_SEL).next() {
        let text = clean_text(&h1.text().collect::<String>());
        if !text.is_empty() {
            return Some(text);
        }
    }

    if let Some(meta) = doc.select(&OG_TITLE_SEL).next()
        && let Some(content) = meta.value().attr("content")
    {
        let text = strip_site_suffix(content);
        if !text.is_empty() {
            return Some(text);
        }
    }

    if let Some(title) = doc.select(&TITLE_SEL).next() {
        let text = strip_site_suffix(&title.text().collect::<String>());
        if !text.is_empty() {
            return Some(text);
        }
    }

    None
}

/// Extracts the most likely series cover image URL from dedicated cover containers or `og:image`.
pub fn extract_cover(doc: &Html, base: &Url) -> Option<Url> {
    if let Some(el) = doc.select(&COVER_IMG_SEL).next() {
        if let Some(url) = image_candidates(el, base).into_iter().next() {
            return Some(url);
        }
        if let Some(content) = el.value().attr("content")
            && let Some(url) = abs_url(base, content)
        {
            return Some(url);
        }
    }

    for meta in doc.select(&OG_IMAGE_SEL) {
        if let Some(content) = meta.value().attr("content")
            && let Some(url) = abs_url(base, content)
        {
            return Some(url);
        }
    }

    None
}
