//! Extraction of previous/next chapter links and parent manga URLs from chapter pages.

use std::sync::LazyLock;

use regex::Regex;
use scraper::{Html, Selector};
use url::Url;

use crate::parse::dom::{abs_url, clean_text};

static LINK_REL_NEXT: LazyLock<Selector> = LazyLock::new(|| {
    Selector::parse(r#"link[rel="next"], a[rel="next"]"#).expect("valid selector")
});
static LINK_REL_PREV: LazyLock<Selector> = LazyLock::new(|| {
    Selector::parse(r#"link[rel="prev"], a[rel="prev"]"#).expect("valid selector")
});
static ALL_LINKS: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("a[href]").expect("valid selector"));
static BREADCRUMB_LINKS: LazyLock<Selector> = LazyLock::new(|| {
    Selector::parse(
        ".breadcrumb a[href], nav[aria-label*='breadcrumb' i] a[href], [itemtype*='BreadcrumbList'] a[href]",
    )
    .expect("valid selector")
});

static STRIP_CHAPTER_SUFFIX_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(?:[-/](?:chapter|chap|chuong|ch|ep|episode|tap)[-_.]*\d+(?:[.-]\d+)?)(?:\.html?)?/?$")
        .expect("static regex is valid")
});

/// Neighbor chapter links found directly in a chapter HTML document.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ChapterNav {
    /// Previous chapter URL.
    pub prev: Option<Url>,
    /// Next chapter URL.
    pub next: Option<Url>,
}

/// Extracts previous and next chapter URLs from `<link rel>`, CSS classes, or button text.
pub fn extract_prev_next(doc: &Html, page_url: &Url, base: &Url) -> ChapterNav {
    let mut nav = ChapterNav::default();

    if let Some(el) = doc.select(&LINK_REL_PREV).next()
        && let Some(href) = el.value().attr("href")
    {
        nav.prev = abs_url(base, href).filter(|u| u != page_url);
    }

    if let Some(el) = doc.select(&LINK_REL_NEXT).next()
        && let Some(href) = el.value().attr("href")
    {
        nav.next = abs_url(base, href).filter(|u| u != page_url);
    }

    if nav.prev.is_some() && nav.next.is_some() {
        return nav;
    }

    let page_host = page_url.host_str().unwrap_or("");
    for a in doc.select(&ALL_LINKS) {
        let Some(href) = a.value().attr("href") else {
            continue;
        };
        let Some(url) = abs_url(base, href) else {
            continue;
        };
        if url == *page_url || url.host_str() != Some(page_host) {
            continue;
        }

        let class_and_id = format!(
            "{} {}",
            a.value().attr("class").unwrap_or(""),
            a.value().attr("id").unwrap_or("")
        )
        .to_ascii_lowercase();
        let text = clean_text(&a.text().collect::<String>()).to_ascii_lowercase();

        if nav.next.is_none() && looks_like_next(&class_and_id, &text) {
            nav.next = Some(url.clone());
        } else if nav.prev.is_none() && looks_like_prev(&class_and_id, &text) {
            nav.prev = Some(url);
        }

        if nav.prev.is_some() && nav.next.is_some() {
            break;
        }
    }

    nav
}

/// Attempts to discover the parent manga detail page URL from breadcrumbs or URL structure.
pub fn extract_manga_url_from_chapter(doc: &Html, page_url: &Url, base: &Url) -> Option<Url> {
    let page_host = page_url.host_str().unwrap_or("");

    // Breadcrumbs typically end with: Home -> Category -> Manga Title -> Current Chapter.
    let crumbs: Vec<Url> = doc
        .select(&BREADCRUMB_LINKS)
        .filter_map(|a| a.value().attr("href").and_then(|h| abs_url(base, h)))
        .filter(|u| {
            u.host_str() == Some(page_host) && u.path() != "/" && u.path() != page_url.path()
        })
        .collect();

    if let Some(last_crumb) = crumbs.last() {
        return Some(last_crumb.clone());
    }

    // Fallback: strip `-chap-44` / `/chapter-44` from the chapter URL path.
    let stripped = STRIP_CHAPTER_SUFFIX_RE.replace(page_url.path(), "");
    if stripped.len() < page_url.path().len() && stripped.len() > 1 {
        let mut candidate = page_url.clone();
        candidate.set_path(&stripped);
        candidate.set_query(None);
        return Some(candidate);
    }

    None
}

fn looks_like_next(class_id: &str, text: &str) -> bool {
    if class_id.contains("disabled") {
        return false;
    }
    class_id.contains("next-chap")
        || class_id.contains("chap-next")
        || class_id.contains("next_chap")
        || text.contains("chap sau")
        || text.contains("chương sau")
        || text.contains("chap tiếp")
        || text.contains("next chap")
        || text == "next"
        || text == "tiếp"
}

fn looks_like_prev(class_id: &str, text: &str) -> bool {
    if class_id.contains("disabled") {
        return false;
    }
    class_id.contains("prev-chap")
        || class_id.contains("chap-prev")
        || class_id.contains("prev_chap")
        || text.contains("chap trước")
        || text.contains("chương trước")
        || text.contains("prev chap")
        || text == "prev"
        || text == "previous"
        || text == "trước"
}
