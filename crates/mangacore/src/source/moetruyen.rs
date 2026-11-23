//! Dedicated `MoeTruyen` adapter (`moetruyen.net`) supporting catalog, search, paginated manga
//! detail chapter lists, and embedded chapter media extraction.

use std::collections::HashSet;
use std::sync::OnceLock;

use async_trait::async_trait;
use regex::Regex;
use scraper::{ElementRef, Html, Selector};
use serde::Deserialize;
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

/// Maximum paginated chapter listing pages fetched for a single `MoeTruyen` manga detail request.
const MAX_CHAPTER_PAGES: u32 = 12;

/// Dedicated source adapter for `MoeTruyen` (`moetruyen.net`).
#[derive(Debug, Default, Clone, Copy)]
pub struct MoeTruyenSource;

/// Returns `true` if `host_lower` belongs to `MoeTruyen`.
#[must_use]
pub fn is_moetruyen_host(host_lower: &str) -> bool {
    matches!(
        host_lower,
        "moetruyen.net" | "www.moetruyen.net" | "m.moetruyen.net"
    )
}

/// Classifies a `MoeTruyen` URL into its semantic [`UrlKind`], returning `None` for other sites.
#[must_use]
pub fn classify_url(url: &Url) -> Option<UrlKind> {
    let host = url.host_str()?.to_ascii_lowercase();
    if !is_moetruyen_host(&host) {
        return None;
    }

    let segments: Vec<&str> = url
        .path_segments()
        .map(|s| s.filter(|p| !p.is_empty()).collect())
        .unwrap_or_default();

    let has_query = url
        .query_pairs()
        .any(|(k, v)| k.eq_ignore_ascii_case("q") && !v.trim().is_empty());

    if has_query {
        return Some(UrlKind::SearchPage);
    }

    Some(match segments.as_slice() {
        [] => UrlKind::WebsiteHome,
        ["manga"] | ["genres" | "tags" | "teams" | "authors", ..] => UrlKind::CatalogPage,
        ["manga", slug] if is_manga_slug(slug) => UrlKind::MangaDetail,
        ["manga", slug, "chapters", chap] if is_manga_slug(slug) && !chap.is_empty() => {
            UrlKind::ChapterPage
        }
        _ => UrlKind::UnsupportedUrl,
    })
}

fn manga_slug_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^(\d+)-[a-zA-Z0-9._-]+$").expect("valid moetruyen slug regex"))
}

fn title_suffix_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?i)\s*\[(?:tới|toi)\s+chap[^\]]*\]\s*$").expect("valid suffix regex")
    })
}

fn is_manga_slug(slug: &str) -> bool {
    manga_slug_re().is_match(slug)
}

fn extract_source_id(url: &Url) -> Option<String> {
    let segs: Vec<&str> = url.path_segments()?.filter(|s| !s.is_empty()).collect();
    if segs.first() != Some(&"manga") {
        return None;
    }
    let caps = manga_slug_re().captures(segs.get(1)?)?;
    Some(format!("moetruyen:{}", caps.get(1)?.as_str()))
}

fn derive_parent_manga_url(chapter_url: &Url) -> Option<Url> {
    let segs: Vec<&str> = chapter_url
        .path_segments()?
        .filter(|s| !s.is_empty())
        .collect();
    if segs.len() >= 4 && segs[0] == "manga" && segs[2] == "chapters" {
        let mut parent = chapter_url.clone();
        parent.set_path(&format!("/manga/{}", segs[1]));
        parent.set_query(None);
        return Some(parent);
    }
    None
}

fn sel(css: &str) -> Selector {
    Selector::parse(css).expect("valid moetruyen static css selector")
}

/// Splits a `MoeTruyen` image URL carrying `#media-fallback=<encoded>` into `(primary, fallback)`.
#[must_use]
pub fn split_media_fallback(raw: &str, base: &Url) -> Option<(Url, Option<Url>)> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed.starts_with("data:") || trimmed.contains("${") {
        return None;
    }
    let (main_part, fallback_part) = match trimmed.split_once("#media-fallback=") {
        Some((m, f)) => (m, Some(f)),
        None => (trimmed, None),
    };
    let mut primary = abs_url(base, main_part)?;
    primary.set_fragment(None);

    let fallback = fallback_part
        .and_then(percent_decode_url)
        .and_then(|decoded| abs_url(base, &decoded));

    Some((primary, fallback))
}

fn percent_decode_url(encoded: &str) -> Option<String> {
    let bytes = encoded.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).ok()?;
            let val = u8::from_str_radix(hex, 16).ok()?;
            out.push(val);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

fn is_valid_moetruyen_detail_url(url: &Url) -> bool {
    let Some(host) = url.host_str() else {
        return false;
    };
    if !is_moetruyen_host(&host.to_ascii_lowercase()) {
        return false;
    }
    let segs: Vec<&str> = url
        .path_segments()
        .map(|s| s.filter(|p| !p.is_empty()).collect())
        .unwrap_or_default();
    segs.len() == 2 && segs[0] == "manga" && is_manga_slug(segs[1])
}

/// Parses a `MoeTruyen` homepage, catalog, or search page into a [`CatalogPage`].
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
    let mut seen = HashSet::new();

    for a in doc.select(&sel("a[href*='/manga/']")) {
        let Some(href) = a.value().attr("href") else {
            continue;
        };
        let Some(mut resolved) = abs_url(&base, href) else {
            continue;
        };
        resolved.set_query(None);
        resolved.set_fragment(None);
        if !is_valid_moetruyen_detail_url(&resolved) {
            continue;
        }

        let key = resolved.as_str().trim_end_matches('/').to_owned();
        if !seen.insert(key) {
            continue;
        }

        let card = find_card_ancestor(a);
        let cover = a
            .select(&sel("img"))
            .chain(card.select(&sel("img")))
            .find_map(|img| {
                let raw = img
                    .value()
                    .attr("src")
                    .or_else(|| img.value().attr("data-src"))?;
                split_media_fallback(raw, &base).map(|(primary, _)| primary)
            });

        let Some(title) = extract_card_title(a, card) else {
            continue;
        };
        let latest_chapter = card
            .select(&sel("a[href*='/chapters/'], .chapter, .latest-chapter"))
            .find_map(|el| {
                let txt = clean_text(&el.text().collect::<Vec<_>>().join(" "));
                if txt.is_empty() { None } else { Some(txt) }
            });

        let source_id = extract_source_id(&resolved);
        entries.push(CatalogEntry {
            url: resolved,
            title,
            alt_titles: Vec::new(),
            cover,
            latest_chapter,
            status: None,
            genres: Vec::new(),
            source_id,
        });
    }

    let query = page_url
        .query_pairs()
        .find(|(k, _)| k.eq_ignore_ascii_case("q"))
        .map(|(_, v)| v.trim().to_owned())
        .filter(|v| !v.is_empty());

    if entries.is_empty() && query.is_none() {
        return Err(ScrapeError::NotRecognized {
            url: page_url.clone(),
            reason: "MoeTruyen catalog parser found no valid manga entries",
        });
    }

    let pagination = extract_moetruyen_pagination(&doc, &base, page_url);
    let title = match &query {
        Some(q) => format!("MoeTruyen: {q}"),
        None if url_kind == UrlKind::WebsiteHome => "MoeTruyen".to_owned(),
        None => "MoeTruyen — Danh sách truyện".to_owned(),
    };

    Ok(CatalogPage {
        url: page_url.clone(),
        site: SiteId::known("moetruyen"),
        site_name: "MoeTruyen".to_owned(),
        title,
        url_kind,
        query,
        search_url_template: Some("https://moetruyen.net/manga?q={query}".to_owned()),
        entries,
        categories: moetruyen_categories(),
        pagination,
    })
}

fn find_card_ancestor(anchor: ElementRef<'_>) -> ElementRef<'_> {
    let mut cur = anchor;
    for _ in 0..3 {
        let Some(parent) = cur.parent().and_then(ElementRef::wrap) else {
            break;
        };
        let tag = parent.value().name();
        if matches!(tag, "article" | "li") {
            return parent;
        }
        if let Some(cls) = parent.value().attr("class")
            && (cls.contains("card") || cls.contains("manga-item") || cls.contains("grid-item"))
        {
            return parent;
        }
        cur = parent;
    }
    anchor
}

fn extract_card_title(anchor: ElementRef<'_>, card: ElementRef<'_>) -> Option<String> {
    for candidate in [
        anchor.value().attr("title").map(clean_text),
        anchor.value().attr("aria-label").map(clean_text),
        card.select(&sel("h2, h3, h4, .title, .manga-title, .card-title"))
            .next()
            .map(|el| clean_text(&el.text().collect::<Vec<_>>().join(" "))),
        anchor
            .select(&sel("img[alt]"))
            .next()
            .and_then(|img| img.value().attr("alt").map(clean_text)),
        Some(clean_text(&anchor.text().collect::<Vec<_>>().join(" "))),
    ] {
        if let Some(txt) = candidate
            && !txt.is_empty()
            && !txt.contains("${")
        {
            let cleaned = title_suffix_re().replace(&txt, "").trim().to_owned();
            if !cleaned.is_empty() {
                return Some(cleaned);
            }
        }
    }
    None
}

fn moetruyen_categories() -> Vec<NavLink> {
    [
        (
            "Tất cả truyện",
            "https://moetruyen.net/manga",
            UrlKind::CatalogPage,
        ),
        ("Trang chủ", "https://moetruyen.net/", UrlKind::WebsiteHome),
    ]
    .into_iter()
    .filter_map(|(label, href, kind)| {
        Some(NavLink {
            label: label.to_owned(),
            url: Url::parse(href).ok()?,
            kind,
        })
    })
    .collect()
}

fn extract_moetruyen_pagination(doc: &Html, base: &Url, page_url: &Url) -> PaginationInfo {
    let current_page = page_url
        .query_pairs()
        .find(|(k, _)| k.eq_ignore_ascii_case("page"))
        .and_then(|(_, v)| v.parse::<u32>().ok())
        .unwrap_or(1)
        .max(1);

    let mut max_page = current_page;
    let mut prev_url = None;
    let mut next_url = None;

    for a in doc.select(&sel("a[href*='page=']")) {
        let Some(href) = a.value().attr("href") else {
            continue;
        };
        let Some(resolved) = abs_url(base, href) else {
            continue;
        };
        if let Some(p) = resolved
            .query_pairs()
            .find(|(k, _)| k.eq_ignore_ascii_case("page"))
            .and_then(|(_, v)| v.parse::<u32>().ok())
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

/// Parses a single `MoeTruyen` detail HTML page, returning `(Manga, max_chapter_page)`.
///
/// # Errors
///
/// Returns [`ScrapeError::MissingMetadata`] or [`ScrapeError::NotRecognized`] if title or chapters are missing.
pub fn parse_detail_html(html: &str, page_url: &Url) -> Result<(Manga, u32), ScrapeError> {
    let doc = Html::parse_document(html);
    let base = effective_base_url(&doc, page_url);
    let mut canon_url = page_url.clone();
    canon_url.set_query(None);
    canon_url.set_fragment(None);

    let title = doc
        .select(&sel(
            ".manga-detail-hero h1, .detail-info h1, .manga-detail-breadcrumb span:last-child, meta[property='og:title'], main h1, h1",
        ))
        .find_map(|el| {
            let raw = if el.value().name() == "meta" {
                el.value().attr("content")?.to_owned()
            } else {
                el.text().collect::<Vec<_>>().join(" ")
            };
            let cleaned = title_suffix_re().replace(&clean_text(&raw), "").trim().to_owned();
            if cleaned.is_empty() || cleaned.contains("${") {
                None
            } else {
                Some(cleaned)
            }
        })
        .ok_or_else(|| ScrapeError::MissingMetadata {
            url: canon_url.as_str().to_owned(),
            detail: "MoeTruyen detail page did not expose a manga title".to_owned(),
        })?;

    let cover = doc
        .select(&sel(
            ".detail-cover img, .cover--detail img, .manga-cover, meta[property='og:image']",
        ))
        .find_map(|el| {
            let raw = if el.value().name() == "meta" {
                el.value().attr("content")?
            } else {
                el.value()
                    .attr("src")
                    .or_else(|| el.value().attr("data-src"))?
            };
            split_media_fallback(raw, &base).map(|(primary, _)| primary)
        });

    let (authors, alt_titles, genres, description) = extract_moetruyen_detail_meta(&doc);

    let mut chapters = extract_moetruyen_chapters(&doc, &base);
    sort_chapters_ascending(&mut chapters);
    let max_chap_page = extract_max_chapter_page(&doc, &base);

    if chapters.is_empty() {
        return Err(ScrapeError::NotRecognized {
            url: canon_url,
            reason: "MoeTruyen detail page contained no readable chapters",
        });
    }

    let count = u32::try_from(chapters.len()).ok();
    let manga = Manga {
        source_id: extract_source_id(&canon_url),
        canonical_url: Some(canon_url.clone()),
        url: canon_url,
        site: SiteId::known("moetruyen"),
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
        status: None,
        translation_status: None,
        language: Some("vi".to_owned()),
        available_languages: vec!["vi".to_owned()],
        chapter_count: count,
    };

    Ok((manga, max_chap_page))
}

fn extract_moetruyen_detail_meta(
    doc: &Html,
) -> (Vec<String>, Vec<String>, Vec<String>, Option<String>) {
    let mut authors = Vec::new();
    let mut alt_titles = Vec::new();
    for p in doc.select(&sel(
        ".detail-info p, .manga-detail-hero p, .manga-detail-meta",
    )) {
        let full_txt = clean_text(&p.text().collect::<Vec<_>>().join(" "));
        let lower = full_txt.to_ascii_lowercase();
        if lower.contains("tác giả") {
            for a in p.select(&sel("a")) {
                let name = clean_text(&a.text().collect::<Vec<_>>().join(" "));
                if !name.is_empty() && !authors.contains(&name) {
                    authors.push(name);
                }
            }
        } else if lower.contains("tên khác") {
            let val = p
                .select(&sel(".manga-detail-meta-value"))
                .next()
                .map_or_else(
                    || {
                        full_txt
                            .split_once(':')
                            .map_or_else(String::new, |(_, r)| clean_text(r))
                    },
                    |el| clean_text(&el.text().collect::<Vec<_>>().join(" ")),
                );
            for part in val.split([',', '•', ';', '|']) {
                let item = clean_text(part);
                if !item.is_empty() && !alt_titles.contains(&item) {
                    alt_titles.push(item);
                }
            }
        }
    }

    let mut genres = Vec::new();
    for a in doc.select(&sel(
        ".detail-info a[href*='genre'], .detail-info a[href*='tag'], .manga-detail-tags a",
    )) {
        let g = clean_text(&a.text().collect::<Vec<_>>().join(" "));
        if !g.is_empty() && !genres.contains(&g) {
            genres.push(g);
        }
    }

    let description = doc
        .select(&sel(
            ".manga-detail-description, .detail-description, meta[property='og:description']",
        ))
        .find_map(|el| {
            let raw = if el.value().name() == "meta" {
                el.value().attr("content")?.to_owned()
            } else {
                el.text().collect::<Vec<_>>().join(" ")
            };
            let cleaned = clean_text(&raw);
            if cleaned.is_empty() || cleaned.contains("${") {
                None
            } else {
                Some(cleaned)
            }
        });

    (authors, alt_titles, genres, description)
}

fn extract_moetruyen_chapters(doc: &Html, base: &Url) -> Vec<ChapterRef> {
    let mut chapters = Vec::new();
    let mut seen = HashSet::new();

    for a in doc.select(&sel("a[href*='/chapters/']")) {
        let Some(href) = a.value().attr("href") else {
            continue;
        };
        let Some(mut url) = abs_url(base, href) else {
            continue;
        };
        url.set_fragment(None);
        if classify_url(&url) != Some(UrlKind::ChapterPage) {
            continue;
        }
        let key = url.as_str().trim_end_matches('/').to_owned();
        if !seen.insert(key) {
            continue;
        }

        let num_attr = a
            .value()
            .attr("data-chapter-number")
            .and_then(|s| s.trim().parse::<f32>().ok());
        let label = a
            .select(&sel(".chapter-num"))
            .next()
            .map(|el| clean_text(&el.text().collect::<Vec<_>>().join(" ")))
            .filter(|s| !s.is_empty())
            .or_else(|| {
                let txt = clean_text(&a.text().collect::<Vec<_>>().join(" "));
                if txt.is_empty() { None } else { Some(txt) }
            })
            .unwrap_or_else(|| {
                num_attr.map_or_else(|| "Chapter".to_owned(), |n| format!("Ch. {n}"))
            });

        let number = num_attr
            .or_else(|| extract_chapter_number(&label))
            .or_else(|| extract_chapter_number(url.path()));
        let source_id = a
            .value()
            .attr("data-chapter-id")
            .map(|id| format!("moetruyen:{id}"));

        chapters.push(ChapterRef {
            url,
            name: label,
            number,
            volume: None,
            published_at: None,
            language: Some("vi".to_owned()),
            source_id,
        });
    }

    chapters
}

fn extract_max_chapter_page(doc: &Html, base: &Url) -> u32 {
    let mut max_p = 1_u32;
    for a in doc.select(&sel("a[href*='chapterPage=']")) {
        if let Some(href) = a.value().attr("href")
            && let Some(resolved) = abs_url(base, href)
            && let Some(p) = resolved
                .query_pairs()
                .find(|(k, _)| k.eq_ignore_ascii_case("chapterPage"))
                .and_then(|(_, v)| v.parse::<u32>().ok())
        {
            max_p = max_p.max(p);
        }
    }
    max_p.min(MAX_CHAPTER_PAGES)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MoeMediaItem {
    #[serde(default)]
    page_index: Option<u32>,
    #[serde(default)]
    page_number: Option<u32>,
    #[serde(default)]
    storage_key: Option<String>,
    #[serde(default)]
    primary_url: Option<String>,
    #[serde(default)]
    download_url: Option<String>,
    #[serde(default)]
    fallback_url: Option<String>,
    #[serde(default)]
    allow_fallback: Option<bool>,
}

/// Extracts ordered [`PageImage`] entries from `MoeTruyen`'s embedded `media: [...]` chapter payload
/// (with DOM `<img>` fallback).
///
/// # Errors
///
/// Returns [`ScrapeError::MissingImages`] if zero readable chapter page images are found.
pub fn parse_chapter_html(html: &str, page_url: &Url) -> Result<Chapter, ScrapeError> {
    let doc = Html::parse_document(html);
    let base = effective_base_url(&doc, page_url);

    let mut images = extract_embedded_media_images(html, page_url);

    // Fallback to DOM <img> elements if a chapter uses server-rendered image tags.
    if images.is_empty() {
        let mut seen = HashSet::new();
        for img in doc.select(&sel(".reader-shell img, .chapter-images img, main img")) {
            let raw = img
                .value()
                .attr("data-src")
                .or_else(|| img.value().attr("src"))
                .unwrap_or("");
            if let Some((primary, fallback)) = split_media_fallback(raw, &base)
                && !primary.path().ends_with("taint.png")
                && seen.insert(primary.as_str().to_owned())
            {
                images.push(PageImage {
                    url: primary,
                    fallbacks: fallback.into_iter().collect(),
                    referer: page_url.clone(),
                });
            }
        }
    }

    if images.is_empty() {
        return Err(ScrapeError::MissingImages {
            url: page_url.clone(),
        });
    }

    let name = doc
        .select(&sel("h1, title"))
        .find_map(|el| {
            let txt = clean_text(&el.text().collect::<Vec<_>>().join(" "));
            if txt.is_empty() || txt.contains("${") {
                None
            } else {
                Some(txt)
            }
        })
        .unwrap_or_else(|| "Chapter".to_owned());

    let manga_url = derive_parent_manga_url(page_url);
    Ok(Chapter {
        url: page_url.clone(),
        name,
        images,
        prev: None,
        next: None,
        manga_url,
    })
}

fn extract_embedded_media_images(html: &str, page_url: &Url) -> Vec<PageImage> {
    let Some(json_slice) = find_json_array_after(html, "media:") else {
        return Vec::new();
    };
    let Ok(mut items) = serde_json::from_str::<Vec<MoeMediaItem>>(json_slice) else {
        return Vec::new();
    };

    items.sort_by_key(|it| {
        (
            it.page_index.unwrap_or(u32::MAX),
            it.page_number.unwrap_or(u32::MAX),
        )
    });

    let mut images = Vec::new();
    let mut seen = HashSet::new();
    for item in items {
        // Skip sentinel script markers (`0.js`) that carry no direct image primaryUrl.
        if item
            .storage_key
            .as_deref()
            .is_some_and(|k| k.eq_ignore_ascii_case("0.js"))
        {
            continue;
        }
        let primary_raw = item.primary_url.as_deref().or_else(|| {
            item.download_url
                .as_deref()
                .filter(|u| !u.to_ascii_lowercase().contains(".js"))
        });
        let Some(raw) = primary_raw else {
            continue;
        };
        let Ok(primary) = Url::parse(raw.trim()) else {
            continue;
        };
        if primary
            .path()
            .rsplit('.')
            .next()
            .is_some_and(|ext| matches!(ext.to_ascii_lowercase().as_str(), "js" | "css" | "html"))
        {
            continue;
        }
        if !seen.insert(primary.as_str().to_owned()) {
            continue;
        }

        let mut fallbacks = Vec::new();
        if let Some(fb_raw) = item.fallback_url.as_deref()
            && !fb_raw.trim().is_empty()
            && !fb_raw.to_ascii_lowercase().contains(".js")
            && let Ok(fb) = Url::parse(fb_raw.trim())
            && fb != primary
        {
            fallbacks.push(fb);
        }
        if item.allow_fallback.unwrap_or(false)
            && let Some(dl) = item.download_url.as_deref()
            && !dl.to_ascii_lowercase().contains(".js")
            && let Ok(fb) = Url::parse(dl.trim())
            && fb != primary
            && !fallbacks.contains(&fb)
        {
            fallbacks.push(fb);
        }

        images.push(PageImage {
            url: primary,
            fallbacks,
            referer: page_url.clone(),
        });
    }
    images
}

fn find_json_array_after<'a>(haystack: &'a str, marker: &str) -> Option<&'a str> {
    let mut search_from = 0;
    while let Some(rel) = haystack[search_from..].find(marker) {
        let after_marker = search_from + rel + marker.len();
        let rest = &haystack[after_marker..];
        let trimmed_offset = rest.len() - rest.trim_start().len();
        let start = after_marker + trimmed_offset;
        if haystack.as_bytes().get(start) == Some(&b'[')
            && let Some(end) = find_matching_bracket(haystack, start)
        {
            let slice = &haystack[start..=end];
            if slice.contains("pageIndex") || slice.contains("primaryUrl") {
                return Some(slice);
            }
        }
        search_from = after_marker;
    }
    None
}

fn find_matching_bracket(s: &str, open_idx: usize) -> Option<usize> {
    let bytes = s.as_bytes();
    let mut depth = 0_u32;
    let mut in_string = false;
    let mut escaped = false;

    for (idx, &b) in bytes.iter().enumerate().skip(open_idx) {
        if in_string {
            if escaped {
                escaped = false;
            } else if b == b'\\' {
                escaped = true;
            } else if b == b'"' {
                in_string = false;
            }
            continue;
        }
        match b {
            b'"' => in_string = true,
            b'[' => depth += 1,
            b']' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return Some(idx);
                }
            }
            _ => {}
        }
    }
    None
}

fn sort_chapters_ascending(chapters: &mut [ChapterRef]) {
    chapters.sort_by(|a, b| match (a.number, b.number) {
        (Some(na), Some(nb)) => na
            .partial_cmp(&nb)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.url.as_str().cmp(b.url.as_str())),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => a.url.as_str().cmp(b.url.as_str()),
    });
}

impl MoeTruyenSource {
    async fn fetch_full_detail(&self, http: &HttpClient, url: &Url) -> Result<Manga, ScrapeError> {
        let page = http.fetch_html(url).await?;
        let (mut manga, max_page) = parse_detail_html(&page.html, &page.final_url)?;

        if max_page > 1 {
            let mut seen: HashSet<String> = manga
                .chapters
                .iter()
                .map(|c| c.url.as_str().trim_end_matches('/').to_owned())
                .collect();

            for p in 2..=max_page {
                let mut page_url = manga.url.clone();
                page_url
                    .query_pairs_mut()
                    .append_pair("chapterPage", &p.to_string());
                let Ok(extra) = http.fetch_html(&page_url).await else {
                    break;
                };
                let extra_doc = Html::parse_document(&extra.html);
                let extra_base = effective_base_url(&extra_doc, &extra.final_url);
                let extra_chapters = extract_moetruyen_chapters(&extra_doc, &extra_base);
                if extra_chapters.is_empty() {
                    break;
                }
                for ch in extra_chapters {
                    let key = ch.url.as_str().trim_end_matches('/').to_owned();
                    if seen.insert(key) {
                        manga.chapters.push(ch);
                    }
                }
            }
        }

        sort_chapters_ascending(&mut manga.chapters);
        manga.chapter_count = u32::try_from(manga.chapters.len()).ok();
        Ok(manga)
    }
}

#[async_trait]
impl Source for MoeTruyenSource {
    fn id(&self) -> &'static str {
        "moetruyen"
    }

    fn matches(&self, url: &Url) -> bool {
        url.host_str()
            .is_some_and(|h| is_moetruyen_host(&h.to_ascii_lowercase()))
    }

    async fn resolve(&self, http: &HttpClient, url: &Url) -> Result<Resolved, ScrapeError> {
        let pre_kind = classify_url(url).unwrap_or(UrlKind::UnsupportedUrl);
        match pre_kind {
            UrlKind::WebsiteHome | UrlKind::CatalogPage | UrlKind::SearchPage => {
                let page = http.fetch_html(url).await?;
                let final_kind = classify_url(&page.final_url).unwrap_or(pre_kind);
                let catalog = parse_catalog_html(&page.html, &page.final_url, final_kind)?;
                Ok(Resolved::Catalog(Box::new(catalog)))
            }
            UrlKind::MangaDetail => {
                let manga = self.fetch_full_detail(http, url).await?;
                Ok(Resolved::Manga(Box::new(manga)))
            }
            UrlKind::ChapterPage => {
                let page = http.fetch_html(url).await?;
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
                    && let Ok(manga) = self.fetch_full_detail(http, parent_url).await
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
                    SiteId::known("moetruyen"),
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
                url: url.clone(),
                reason: "unrecognized MoeTruyen page type",
            }),
        }
    }

    async fn chapter(&self, http: &HttpClient, url: &Url) -> Result<Chapter, ScrapeError> {
        let page = http.fetch_html(url).await?;
        parse_chapter_html(&page.html, &page.final_url)
    }
}
