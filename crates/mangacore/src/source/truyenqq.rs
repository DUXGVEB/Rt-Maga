//! Dedicated `TruyenQQ` adapter handling both mobile AMP (`m.truyenqq*.com`) and desktop
//! (`truyenqq*.com`) catalog, category, search, manga detail, and chapter pages.

use std::collections::HashSet;
use std::sync::OnceLock;

use async_trait::async_trait;
use regex::Regex;
use scraper::{ElementRef, Html, Selector};
use url::Url;

use crate::error::ScrapeError;
use crate::http::HttpClient;
use crate::model::{
    CatalogEntry, CatalogPage, Chapter, ChapterRef, Manga, NavLink, PageImage, PaginationInfo,
    Resolved, SiteId,
};
use crate::parse::chapters::extract_chapter_number;
use crate::parse::dom::{abs_url, clean_text, effective_base_url};
use crate::recognize::UrlKind;
use crate::source::Source;

/// Dedicated source adapter for `TruyenQQ` (`truyenqqko.com` and `m.truyenqqko.com`).
#[derive(Debug, Default, Clone, Copy)]
pub struct TruyenQQSource;

/// Returns `true` if `host_lower` belongs to `TruyenQQ` (including `m.` and `www.` subdomains).
#[must_use]
pub fn is_truyenqq_host(host_lower: &str) -> bool {
    let bare = host_lower
        .strip_prefix("www.")
        .or_else(|| host_lower.strip_prefix("m."))
        .unwrap_or(host_lower);
    bare.starts_with("truyenqq")
        && bare
            .rsplit('.')
            .next()
            .is_some_and(|tld| matches!(tld, "com" | "net"))
}

/// Classifies a `TruyenQQ` URL into its semantic [`UrlKind`], returning `None` for other websites.
#[must_use]
pub fn classify_url(url: &Url) -> Option<UrlKind> {
    let host = url.host_str()?.to_ascii_lowercase();
    if !is_truyenqq_host(&host) {
        return None;
    }

    let path = url.path().trim_matches('/').to_ascii_lowercase();
    if path.is_empty() || path == "doc-truyen" || path == "index.html" {
        return Some(UrlKind::WebsiteHome);
    }

    if path.starts_with("tim-kiem")
        || url
            .query_pairs()
            .any(|(k, v)| k.eq_ignore_ascii_case("q") && !v.trim().is_empty())
    {
        return Some(UrlKind::SearchPage);
    }

    if path == "truyen-tranh" {
        return Some(UrlKind::CatalogPage);
    }

    if let Some(rest) = path.strip_prefix("truyen-tranh/") {
        if rest.is_empty() {
            return Some(UrlKind::CatalogPage);
        }
        if is_chapter_slug(rest) {
            return Some(UrlKind::ChapterPage);
        }
        return Some(UrlKind::MangaDetail);
    }

    if path.starts_with("truyen-moi-cap-nhat")
        || path.starts_with("the-loai")
        || path.starts_with("top-")
        || path.starts_with("truyen-con-")
        || path.starts_with("truyen-yeu-thich")
        || path.starts_with("truyen-dang-cap-nhat")
        || path.starts_with("truyen-hoan-thanh")
        || path.starts_with("tac-gia/")
        || path.starts_with("nhom-dich/")
    {
        return Some(UrlKind::CatalogPage);
    }

    Some(UrlKind::UnsupportedUrl)
}

fn chapter_slug_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?i)-chap-\d+(?:[.-]\d+)?(?:\.html)?$").expect("valid truyenqq chapter regex")
    })
}

fn trailing_id_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"-(\d+)(?:\.html)?$").expect("valid truyenqq id regex"))
}

fn page_num_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?i)/trang-(\d+)").expect("valid truyenqq page regex"))
}

fn is_chapter_slug(slug: &str) -> bool {
    chapter_slug_re().is_match(slug)
}

/// Normalizes mobile `m.truyenqq*.com` detail/chapter URLs to their canonical desktop host.
#[must_use]
pub fn canonical_desktop_url(url: &Url) -> Url {
    let Some(host) = url.host_str() else {
        return url.clone();
    };
    let lower = host.to_ascii_lowercase();
    if let Some(desktop_host) = lower
        .strip_prefix("m.")
        .or_else(|| lower.strip_prefix("www."))
    {
        let mut cloned = url.clone();
        let _ = cloned.set_host(Some(desktop_host));
        return cloned;
    }
    url.clone()
}

fn extract_source_id(url: &Url) -> Option<String> {
    let path = url.path().trim_matches('/');
    let slug = path.strip_prefix("truyen-tranh/")?;
    let detail_slug = chapter_slug_re().replace(slug, "");
    let caps = trailing_id_re().captures(&detail_slug)?;
    Some(format!("truyenqq:{}", caps.get(1)?.as_str()))
}

fn derive_parent_manga_url(chapter_url: &Url) -> Option<Url> {
    let desktop = canonical_desktop_url(chapter_url);
    let path = desktop.path().trim_end_matches('/');
    let slug = path.strip_prefix("/truyen-tranh/")?;
    if !is_chapter_slug(slug) {
        return None;
    }
    let stripped = chapter_slug_re().replace(slug, "");
    let mut parent = desktop.clone();
    parent.set_path(&format!("/truyen-tranh/{stripped}"));
    parent.set_query(None);
    Some(parent)
}

fn sel(css: &str) -> Selector {
    Selector::parse(css).expect("valid truyenqq static css selector")
}

fn pick_img_src(el: ElementRef<'_>, base: &Url) -> Option<Url> {
    for attr in ["data-original", "data-src", "data-lazy-src", "src"] {
        if let Some(raw) = el.value().attr(attr) {
            let trimmed = raw.trim();
            if !trimmed.is_empty()
                && !trimmed.starts_with("data:")
                && let Some(u) = abs_url(base, trimmed)
            {
                return Some(u);
            }
        }
    }
    None
}

fn is_valid_truyenqq_manga_link(url: &Url) -> bool {
    let Some(host) = url.host_str() else {
        return false;
    };
    if !is_truyenqq_host(&host.to_ascii_lowercase()) {
        return false;
    }
    let path = url.path().trim_matches('/').to_ascii_lowercase();
    let Some(slug) = path.strip_prefix("truyen-tranh/") else {
        return false;
    };
    !slug.is_empty() && !is_chapter_slug(slug)
}

/// Parses a `TruyenQQ` homepage, catalog, category, or search page (both AMP mobile and desktop HTML).
///
/// # Errors
///
/// Returns [`ScrapeError::NotRecognized`] if zero valid manga entries can be extracted.
pub fn parse_catalog_html(
    html: &str,
    page_url: &Url,
    url_kind: UrlKind,
) -> Result<CatalogPage, ScrapeError> {
    let doc = Html::parse_document(html);
    let base = effective_base_url(&doc, page_url);

    let mut entries = Vec::new();
    let mut seen_urls = HashSet::new();

    // 1. Desktop card grid: `.list_grid li`
    for card in doc.select(&sel("ul.list_grid > li, .list_grid li")) {
        if let Some(entry) = parse_desktop_card(card, &base) {
            let key = entry.url.as_str().trim_end_matches('/').to_owned();
            if seen_urls.insert(key) {
                entries.push(entry);
            }
        }
    }

    // 2. Mobile AMP card blocks: `div.item`
    if entries.is_empty() {
        for card in doc.select(&sel("div.item")) {
            if let Some(entry) = parse_amp_card(card, &base) {
                let key = entry.url.as_str().trim_end_matches('/').to_owned();
                if seen_urls.insert(key) {
                    entries.push(entry);
                }
            }
        }
    }

    // 3. Structured JSON-LD `ItemList` fallback when card DOM changed
    if entries.is_empty() {
        for entry in parse_json_ld_item_list(&doc, &base) {
            let key = entry.url.as_str().trim_end_matches('/').to_owned();
            if seen_urls.insert(key) {
                entries.push(entry);
            }
        }
    }

    let query = page_url
        .query_pairs()
        .find(|(k, _)| k.eq_ignore_ascii_case("q"))
        .map(|(_, v)| v.trim().to_owned())
        .filter(|v| !v.is_empty());

    if entries.is_empty() && query.is_none() {
        return Err(ScrapeError::NotRecognized {
            url: page_url.clone(),
            reason: "TruyenQQ catalog parser found no valid manga entries",
        });
    }

    let title = doc
        .select(&sel("h1, .text_list_update, title"))
        .find_map(|el| {
            let txt = clean_text(&el.text().collect::<Vec<_>>().join(" "));
            if txt.is_empty() { None } else { Some(txt) }
        })
        .unwrap_or_else(|| "TruyenQQ".to_owned());

    let categories = extract_truyenqq_categories(&doc, &base);
    let pagination = extract_truyenqq_pagination(&doc, &base, page_url);

    let desktop_origin = canonical_desktop_url(page_url);
    let origin_str = format!(
        "{}://{}",
        desktop_origin.scheme(),
        desktop_origin.host_str().unwrap_or("truyenqqko.com")
    );

    Ok(CatalogPage {
        url: page_url.clone(),
        site: SiteId::known("truyenqq"),
        site_name: "TruyenQQ".to_owned(),
        title,
        url_kind,
        query,
        search_url_template: Some(format!("{origin_str}/tim-kiem.html?q={{query}}")),
        entries,
        categories,
        pagination,
    })
}

fn parse_desktop_card(card: ElementRef<'_>, base: &Url) -> Option<CatalogEntry> {
    let title_link = card
        .select(&sel(".book_name a[href], h3 a[href], .book_avatar a[href]"))
        .find_map(|a| {
            let href = a.value().attr("href")?;
            let resolved = canonical_desktop_url(&abs_url(base, href)?);
            if !is_valid_truyenqq_manga_link(&resolved) {
                return None;
            }
            let txt = a
                .value()
                .attr("title")
                .map(clean_text)
                .filter(|s| !s.is_empty())
                .or_else(|| {
                    let t = clean_text(&a.text().collect::<Vec<_>>().join(" "));
                    if t.is_empty() { None } else { Some(t) }
                });
            Some((resolved, txt))
        })?;

    let cover = card
        .select(&sel(".book_avatar img, img"))
        .find_map(|img| pick_img_src(img, base));
    let title = title_link.1.or_else(|| {
        card.select(&sel("img[alt]")).find_map(|img| {
            img.value()
                .attr("alt")
                .map(clean_text)
                .filter(|s| !s.is_empty())
        })
    })?;

    let latest_chapter = card
        .select(&sel(".last_chapter a, .chapter-info a"))
        .find_map(|el| {
            let t = clean_text(&el.text().collect::<Vec<_>>().join(" "));
            if t.is_empty() { None } else { Some(t) }
        });

    let source_id = extract_source_id(&title_link.0);
    Some(CatalogEntry {
        url: title_link.0,
        title,
        alt_titles: Vec::new(),
        cover,
        latest_chapter,
        status: None,
        genres: Vec::new(),
        source_id,
    })
}

fn parse_amp_card(card: ElementRef<'_>, base: &Url) -> Option<CatalogEntry> {
    let link_el = card.select(&sel("h3 a[href], a[href]")).find(|a| {
        a.value()
            .attr("href")
            .and_then(|h| abs_url(base, h))
            .is_some_and(|u| is_valid_truyenqq_manga_link(&canonical_desktop_url(&u)))
    })?;
    let url = canonical_desktop_url(&abs_url(base, link_el.value().attr("href")?)?);
    let img_el = card.select(&sel("amp-img, img")).next();
    let cover = img_el.and_then(|el| pick_img_src(el, base));

    let title = link_el
        .value()
        .attr("title")
        .map(clean_text)
        .filter(|s| !s.is_empty())
        .or_else(|| {
            let t = clean_text(&link_el.text().collect::<Vec<_>>().join(" "));
            if t.is_empty() { None } else { Some(t) }
        })
        .or_else(|| {
            img_el
                .and_then(|el| el.value().attr("alt"))
                .map(clean_text)
                .filter(|s| !s.is_empty())
        })?;

    let latest_chapter = card.select(&sel("a[href]")).find_map(|a| {
        let href = a.value().attr("href")?;
        if is_chapter_slug(href) {
            let txt = clean_text(&a.text().collect::<Vec<_>>().join(" "));
            if txt.is_empty() { None } else { Some(txt) }
        } else {
            None
        }
    });

    let source_id = extract_source_id(&url);
    Some(CatalogEntry {
        url,
        title,
        alt_titles: Vec::new(),
        cover,
        latest_chapter,
        status: None,
        genres: Vec::new(),
        source_id,
    })
}

fn parse_json_ld_item_list(doc: &Html, base: &Url) -> Vec<CatalogEntry> {
    let mut entries = Vec::new();
    for script in doc.select(&sel("script[type='application/ld+json']")) {
        let text = script.text().collect::<Vec<_>>().join("");
        let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) else {
            continue;
        };
        if json.get("@type").and_then(|v| v.as_str()) != Some("ItemList") {
            continue;
        }
        let Some(items) = json.get("itemListElement").and_then(|v| v.as_array()) else {
            continue;
        };
        for item in items {
            let Some(href) = item.get("url").and_then(|v| v.as_str()) else {
                continue;
            };
            let Some(url) = abs_url(base, href).map(|u| canonical_desktop_url(&u)) else {
                continue;
            };
            if !is_valid_truyenqq_manga_link(&url) {
                continue;
            }
            let title = item
                .get("name")
                .and_then(|v| v.as_str())
                .map(clean_text)
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| {
                    url.path_segments()
                        .and_then(std::iter::Iterator::last)
                        .unwrap_or("Manga")
                        .replace('-', " ")
                });
            let source_id = extract_source_id(&url);
            entries.push(CatalogEntry {
                url,
                title,
                alt_titles: Vec::new(),
                cover: None,
                latest_chapter: None,
                status: None,
                genres: Vec::new(),
                source_id,
            });
        }
    }
    entries
}

fn extract_truyenqq_categories(doc: &Html, base: &Url) -> Vec<NavLink> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();

    let desktop_origin = canonical_desktop_url(base);
    if let Ok(latest) = Url::parse(&format!(
        "{}://{}/truyen-moi-cap-nhat.html",
        desktop_origin.scheme(),
        desktop_origin.host_str().unwrap_or("truyenqqko.com")
    )) {
        seen.insert(latest.as_str().to_owned());
        out.push(NavLink {
            label: "Mới cập nhật".to_owned(),
            url: latest,
            kind: UrlKind::CatalogPage,
        });
    }

    for a in doc.select(&sel("a[href*='/the-loai/']")) {
        let Some(href) = a.value().attr("href") else {
            continue;
        };
        let Some(url) = abs_url(base, href).map(|u| canonical_desktop_url(&u)) else {
            continue;
        };
        let label = clean_text(&a.text().collect::<Vec<_>>().join(" "));
        if label.is_empty() || label.len() > 36 {
            continue;
        }
        if seen.insert(url.as_str().to_owned()) {
            out.push(NavLink {
                label,
                url,
                kind: UrlKind::CatalogPage,
            });
            if out.len() >= 18 {
                break;
            }
        }
    }
    out
}

fn extract_truyenqq_pagination(doc: &Html, base: &Url, page_url: &Url) -> PaginationInfo {
    let current_page = page_num_re()
        .captures(page_url.path())
        .and_then(|c| c.get(1)?.as_str().parse::<u32>().ok())
        .or_else(|| {
            page_url
                .query_pairs()
                .find(|(k, _)| k.eq_ignore_ascii_case("page"))
                .and_then(|(_, v)| v.parse::<u32>().ok())
        })
        .unwrap_or(1)
        .max(1);

    let mut max_page = current_page;
    let mut prev_url = None;
    let mut next_url = None;

    for a in doc.select(&sel(
        ".page_redirect a[href], .pagination a[href], a[href*='/trang-']",
    )) {
        let Some(href) = a.value().attr("href") else {
            continue;
        };
        let Some(resolved) = abs_url(base, href).map(|u| canonical_desktop_url(&u)) else {
            continue;
        };
        if let Some(p) = page_num_re()
            .captures(resolved.path())
            .and_then(|c| c.get(1)?.as_str().parse::<u32>().ok())
        {
            if p > max_page {
                max_page = p;
            }
            if p == current_page.saturating_sub(1) && prev_url.is_none() {
                prev_url = Some(resolved.clone());
            }
            if p == current_page + 1 && next_url.is_none() {
                next_url = Some(resolved);
            }
        }
    }

    PaginationInfo {
        current_page,
        total_pages: (max_page > 1).then_some(max_page),
        prev_url,
        next_url,
    }
}

/// Parses a `TruyenQQ` manga detail page into a rich [`Manga`] with metadata and ascending chapters.
///
/// # Errors
///
/// Returns [`ScrapeError::MissingMetadata`] or [`ScrapeError::NotRecognized`] if title/chapters are missing.
pub fn parse_detail_html(html: &str, page_url: &Url) -> Result<Manga, ScrapeError> {
    let doc = Html::parse_document(html);
    let base = effective_base_url(&doc, page_url);
    let canon_url = canonical_desktop_url(page_url);

    let title = doc
        .select(&sel(".book_other h1, h1[itemprop='name'], h1"))
        .find_map(|el| {
            let txt = clean_text(&el.text().collect::<Vec<_>>().join(" "));
            if txt.is_empty() { None } else { Some(txt) }
        })
        .ok_or_else(|| ScrapeError::MissingMetadata {
            url: canon_url.as_str().to_owned(),
            detail: "TruyenQQ detail page did not expose a manga title".to_owned(),
        })?;

    let cover = doc
        .select(&sel(".book_avatar img, meta[property='og:image']"))
        .find_map(|el| {
            if el.value().name() == "meta" {
                el.value().attr("content").and_then(|c| abs_url(&base, c))
            } else {
                pick_img_src(el, &base)
            }
        });

    let (alt_titles, authors, status, chapter_count) = extract_truyenqq_meta_rows(&doc);
    let genres: Vec<String> = doc
        .select(&sel(".list01 a, .book_other .list01 li a"))
        .filter_map(|a| {
            let g = clean_text(&a.text().collect::<Vec<_>>().join(" "));
            if g.is_empty() { None } else { Some(g) }
        })
        .fold(Vec::new(), |mut acc, g| {
            if !acc.contains(&g) {
                acc.push(g);
            }
            acc
        });

    let description = doc
        .select(&sel(
            ".story-detail-info.detail-content, .story-detail-info",
        ))
        .find_map(|el| {
            let txt = clean_text(&el.text().collect::<Vec<_>>().join(" "));
            if txt.is_empty() { None } else { Some(txt) }
        });

    let chapters = extract_truyenqq_chapters(&doc, &base);
    if chapters.is_empty() {
        return Err(ScrapeError::NotRecognized {
            url: canon_url,
            reason: "TruyenQQ detail page contained no readable chapters",
        });
    }

    let effective_count = chapter_count.or_else(|| u32::try_from(chapters.len()).ok());
    Ok(Manga {
        source_id: extract_source_id(&canon_url),
        canonical_url: Some(canon_url.clone()),
        url: canon_url,
        site: SiteId::known("truyenqq"),
        title,
        cover,
        chapters,
        alt_titles,
        original_title: None,
        authors,
        artists: Vec::new(),
        description,
        genres,
        tags: Vec::new(),
        status,
        translation_status: None,
        language: Some("vi".to_owned()),
        available_languages: vec!["vi".to_owned()],
        chapter_count: effective_count,
    })
}

fn extract_truyenqq_meta_rows(
    doc: &Html,
) -> (Vec<String>, Vec<String>, Option<String>, Option<u32>) {
    let mut alt_titles = Vec::new();
    let mut authors = Vec::new();
    let mut status = None;
    let mut chapter_count = None;

    for li in doc.select(&sel(".list-info li")) {
        let cls = li.value().attr("class").unwrap_or("");
        let label = li
            .select(&sel("p.name"))
            .next()
            .map(|el| clean_text(&el.text().collect::<Vec<_>>().join(" ")).to_ascii_lowercase())
            .unwrap_or_default();
        let val_el = li
            .select(&sel("p.col-xs-9, p.col-xs-10, h2.other-name"))
            .next();
        let val_text = val_el
            .map(|el| clean_text(&el.text().collect::<Vec<_>>().join(" ")))
            .unwrap_or_default();

        if val_text.is_empty() || val_text.eq_ignore_ascii_case("đang cập nhật") {
            continue;
        }
        if cls.contains("othername") || label.contains("tên khác") {
            for part in val_text.split([';', '|', '\n']) {
                let p = clean_text(part);
                if !p.is_empty() && !alt_titles.contains(&p) {
                    alt_titles.push(p);
                }
            }
        } else if cls.contains("author") || label.contains("tác giả") {
            for a in li.select(&sel("a")) {
                let name = clean_text(&a.text().collect::<Vec<_>>().join(" "));
                if !name.is_empty() && !authors.contains(&name) {
                    authors.push(name);
                }
            }
            if authors.is_empty() {
                authors.push(val_text);
            }
        } else if cls.contains("status") || label.contains("tình trạng") {
            status = Some(val_text);
        } else if label.contains("tổng số chap") {
            chapter_count = val_text.parse::<u32>().ok();
        }
    }

    (alt_titles, authors, status, chapter_count)
}

fn extract_truyenqq_chapters(doc: &Html, base: &Url) -> Vec<ChapterRef> {
    let mut chapters = Vec::new();
    let mut seen = HashSet::new();

    for row in doc.select(&sel(".works-chapter-list .works-chapter-item")) {
        let Some(a) = row.select(&sel(".name-chap a[href], a[href]")).next() else {
            continue;
        };
        let Some(href) = a.value().attr("href") else {
            continue;
        };
        let Some(url) = abs_url(base, href).map(|u| canonical_desktop_url(&u)) else {
            continue;
        };
        if !seen.insert(url.as_str().trim_end_matches('/').to_owned()) {
            continue;
        }
        let name = clean_text(&a.text().collect::<Vec<_>>().join(" "));
        let number = extract_chapter_number(&name).or_else(|| extract_chapter_number(url.path()));
        let published_at = row.select(&sel(".time-chap")).next().and_then(|el| {
            let txt = clean_text(&el.text().collect::<Vec<_>>().join(" "));
            if txt.is_empty() { None } else { Some(txt) }
        });
        chapters.push(ChapterRef {
            url,
            name: if name.is_empty() {
                "Chương".to_owned()
            } else {
                name
            },
            number,
            volume: None,
            published_at,
            language: Some("vi".to_owned()),
            source_id: None,
        });
    }

    // TruyenQQ lists newest chapter first; normalize to oldest-first ascending reading order.
    chapters.reverse();
    chapters
}

/// Parses a `TruyenQQ` chapter reading page into ordered [`PageImage`] entries and navigation links.
///
/// # Errors
///
/// Returns [`ScrapeError::MissingImages`] if zero chapter page images are found.
pub fn parse_chapter_html(html: &str, page_url: &Url) -> Result<Chapter, ScrapeError> {
    let doc = Html::parse_document(html);
    let base = effective_base_url(&doc, page_url);
    let canon_url = canonical_desktop_url(page_url);

    let mut images = Vec::new();
    let mut seen = HashSet::new();

    for img in doc.select(&sel(
        ".chapter_content .page-chapter img, .page-chapter img",
    )) {
        let Some(primary) = pick_img_src(img, &base) else {
            continue;
        };
        if !seen.insert(primary.as_str().to_owned()) {
            continue;
        }
        let mut fallbacks = Vec::new();
        for fb_attr in ["data-cdn", "data-sv2", "data-backup"] {
            if let Some(raw) = img.value().attr(fb_attr)
                && let Some(fb) = abs_url(&base, raw.trim())
                && fb != primary
                && !fallbacks.contains(&fb)
            {
                fallbacks.push(fb);
            }
        }
        images.push(PageImage {
            url: primary,
            fallbacks,
            referer: canon_url.clone(),
        });
    }

    if images.is_empty() {
        return Err(ScrapeError::MissingImages { url: canon_url });
    }

    let name = doc
        .select(&sel("h1.detail-title, .detail-title, h1"))
        .find_map(|el| {
            let txt = clean_text(&el.text().collect::<Vec<_>>().join(" "));
            if txt.is_empty() { None } else { Some(txt) }
        })
        .unwrap_or_else(|| "Chapter".to_owned());

    let prev = doc
        .select(&sel("a.prev[href], a.control-button.link-prev-chap[href]"))
        .find_map(|a| abs_url(&base, a.value().attr("href")?))
        .map(|u| canonical_desktop_url(&u));
    let next = doc
        .select(&sel("a.next[href], a.control-button.link-next-chap[href]"))
        .find_map(|a| abs_url(&base, a.value().attr("href")?))
        .map(|u| canonical_desktop_url(&u));
    let manga_url = derive_parent_manga_url(&canon_url);

    Ok(Chapter {
        url: canon_url,
        name,
        images,
        prev,
        next,
        manga_url,
    })
}

#[async_trait]
impl Source for TruyenQQSource {
    fn id(&self) -> &'static str {
        "truyenqq"
    }

    fn metadata(&self) -> crate::source::AdapterMetadata {
        crate::source::AdapterMetadata {
            id: "truyenqq",
            name: "TruyenQQ",
            version: "1.2.0",
            domains: &["truyenqq*.com", "truyenqq*.net", "m.truyenqq*"],
            auth_support: crate::source::AdapterAuthSupport::OptionalUserSession,
            strategies: &[
                crate::ExtractionStrategyKind::SiteAdapter,
                crate::ExtractionStrategyKind::NativeHtml,
            ],
            known_limitations: &[
                "CDNs require chapter Referer header and may rate-limit rapid concurrent image downloads.",
            ],
        }
    }

    fn matches(&self, url: &Url) -> bool {
        url.host_str()
            .is_some_and(|h| is_truyenqq_host(&h.to_ascii_lowercase()))
    }

    async fn resolve(&self, http: &HttpClient, url: &Url) -> Result<Resolved, ScrapeError> {
        let pre_kind = classify_url(url).unwrap_or(UrlKind::UnsupportedUrl);
        let fetch_target = match pre_kind {
            UrlKind::MangaDetail | UrlKind::ChapterPage => canonical_desktop_url(url),
            _ => url.clone(),
        };

        let page = http.fetch_html(&fetch_target).await?;
        let final_kind = classify_url(&page.final_url).unwrap_or(pre_kind);

        match final_kind {
            UrlKind::WebsiteHome | UrlKind::CatalogPage | UrlKind::SearchPage => {
                let catalog = parse_catalog_html(&page.html, &page.final_url, final_kind)?;
                Ok(Resolved::Catalog(Box::new(catalog)))
            }
            UrlKind::MangaDetail => {
                let manga = parse_detail_html(&page.html, &page.final_url)?;
                Ok(Resolved::Manga(Box::new(manga)))
            }
            UrlKind::ChapterPage => {
                let ch = parse_chapter_html(&page.html, &page.final_url)?;
                let ch_ref = ChapterRef {
                    url: ch.url.clone(),
                    name: ch.name.clone(),
                    number: extract_chapter_number(&ch.name)
                        .or_else(|| extract_chapter_number(ch.url.path())),
                    volume: None,
                    published_at: None,
                    language: Some("vi".to_owned()),
                    source_id: None,
                };

                if let Some(parent_url) = ch.manga_url.as_ref()
                    && let Ok(parent_page) = http.fetch_html(parent_url).await
                    && let Ok(manga) = parse_detail_html(&parent_page.html, &parent_page.final_url)
                {
                    let matched = manga
                        .chapters
                        .iter()
                        .find(|c| {
                            c.url.as_str().trim_end_matches('/')
                                == ch.url.as_str().trim_end_matches('/')
                        })
                        .cloned()
                        .unwrap_or(ch_ref);
                    return Ok(Resolved::Chapter {
                        manga: Box::new(manga),
                        at: Box::new(matched),
                    });
                }

                let fallback = Manga::minimal(
                    ch.manga_url.unwrap_or_else(|| ch.url.clone()),
                    SiteId::known("truyenqq"),
                    ch.name,
                    None,
                    vec![ch_ref.clone()],
                );
                Ok(Resolved::Chapter {
                    manga: Box::new(fallback),
                    at: Box::new(ch_ref),
                })
            }
            _ => Err(ScrapeError::NotRecognized {
                url: page.final_url,
                reason: "unrecognized TruyenQQ page type",
            }),
        }
    }

    async fn chapter(&self, http: &HttpClient, url: &Url) -> Result<Chapter, ScrapeError> {
        let target = canonical_desktop_url(url);
        let page = http.fetch_html(&target).await?;
        parse_chapter_html(&page.html, &page.final_url)
    }
}
