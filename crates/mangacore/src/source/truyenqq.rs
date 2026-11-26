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
use crate::parse::filters::{
    CatalogFilterQuery, FilterAxis, FilterAxisKind, FilterOptionItem, FilterTagOption,
    SiteFilterSchema, UnhandledFilterMask, UpdatedWithinFilter, extract_select_axes,
};
use crate::parse::pagination::template_from_numbered_url;
use crate::recognize::UrlKind;
use crate::source::Source;

/// Dedicated source adapter for `TruyenQQ` (`truyenqq.com.vn`, `truyenqqko.com`, and mirrors).
#[derive(Debug, Default, Clone, Copy)]
pub struct TruyenQQSource;

/// Returns `true` if `host_lower` belongs to `TruyenQQ` (including `.com.vn`, `.vn`, `.com`, `.net`, and `m.`/`www.` subdomains).
#[must_use]
pub fn is_truyenqq_host(host_lower: &str) -> bool {
    let bare = host_lower
        .strip_prefix("www.")
        .or_else(|| host_lower.strip_prefix("m."))
        .unwrap_or(host_lower);
    let Some((first_label, rest)) = bare.split_once('.') else {
        return false;
    };
    first_label.starts_with("truyenqq")
        && !rest.is_empty()
        && rest
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'.' || b == b'-')
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

    if path.starts_with("tim-kiem-nang-cao") {
        return Some(UrlKind::CatalogPage);
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
    let pagination =
        extract_truyenqq_pagination(&doc, &base, page_url, url_kind, !entries.is_empty());

    let desktop_origin = canonical_desktop_url(page_url);
    let origin_str = format!(
        "{}://{}",
        desktop_origin.scheme(),
        desktop_origin.host_str().unwrap_or("truyenqq.com.vn")
    );
    let filter_schema = truyenqq_filter_schema(&origin_str, Some(&doc), Some(&base));

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
        filter_schema,
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
    let updated_at = card
        .select(&sel(".time-ago, .chapter-info span.time"))
        .find_map(|el| {
            let t = clean_text(&el.text().collect::<Vec<_>>().join(" "));
            if t.is_empty() { None } else { Some(t) }
        });
    let genres: Vec<String> = card
        .select(&sel(
            ".more-info .list-tags a, .more-info .list-tags p, .list01 a",
        ))
        .filter_map(|el| {
            let g = clean_text(&el.text().collect::<Vec<_>>().join(" "));
            if g.is_empty() || g.len() > 36 {
                None
            } else {
                Some(g)
            }
        })
        .fold(Vec::new(), |mut acc, g| {
            if !acc.contains(&g) {
                acc.push(g);
            }
            acc
        });
    let status = card.select(&sel(".more-info .info p")).find_map(|p| {
        let txt = clean_text(&p.text().collect::<Vec<_>>().join(" "));
        let low = txt.to_lowercase();
        if low.contains("tình trạng") {
            txt.split(':')
                .nth(1)
                .map(clean_text)
                .filter(|s| !s.is_empty())
        } else {
            None
        }
    });
    let chapter_count = latest_chapter
        .as_deref()
        .and_then(extract_chapter_number)
        .and_then(non_neg_f32_to_u32);

    let source_id = extract_source_id(&title_link.0);
    Some(CatalogEntry {
        url: title_link.0,
        title,
        alt_titles: Vec::new(),
        cover,
        latest_chapter,
        status,
        genres,
        source_id,
        chapter_count,
        updated_at,
    })
}

fn non_neg_f32_to_u32(val: f32) -> Option<u32> {
    if !val.is_finite() || val < 0.0 {
        return None;
    }
    #[expect(
        clippy::as_conversions,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "clamped finite non-negative chapter number"
    )]
    Some(val.floor().min(f32::from(u16::MAX)) as u32)
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
    let chapter_count = latest_chapter
        .as_deref()
        .and_then(extract_chapter_number)
        .and_then(non_neg_f32_to_u32);

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
        chapter_count,
        updated_at: None,
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
                chapter_count: None,
                updated_at: None,
            });
        }
    }
    entries
}

/// Builds a `TruyenQQ` paginated URL for `page_num` from any homepage, category, or search URL.
#[must_use]
pub fn build_truyenqq_page_url(base_listing: &Url, page_num: u32) -> Url {
    let mut clean = canonical_desktop_url(base_listing);
    let page = page_num.max(1);
    let raw_path = clean.path().trim_end_matches('/');

    let without_ext = raw_path.strip_suffix(".html").unwrap_or(raw_path);
    let stem = page_num_re().replace(without_ext, "").into_owned();
    let trimmed_stem = stem.trim_matches('/');

    let effective_stem = if trimmed_stem.is_empty()
        || trimmed_stem.eq_ignore_ascii_case("doc-truyen")
        || trimmed_stem.eq_ignore_ascii_case("index")
    {
        "truyen-moi-cap-nhat"
    } else {
        trimmed_stem
    };

    if page <= 1 {
        clean.set_path(&format!("/{effective_stem}.html"));
    } else {
        clean.set_path(&format!("/{effective_stem}/trang-{page}.html"));
    }
    clean
}

fn extract_truyenqq_categories(doc: &Html, base: &Url) -> Vec<NavLink> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();

    let desktop_origin = canonical_desktop_url(base);
    let origin_str = format!(
        "{}://{}",
        desktop_origin.scheme(),
        desktop_origin.host_str().unwrap_or("truyenqq.com.vn")
    );

    for (label, slug) in [
        ("Mới cập nhật", "/truyen-moi-cap-nhat.html"),
        ("Hoàn thành", "/truyen-hoan-thanh.html"),
        ("Top Ngày", "/top-ngay.html"),
        ("Top Tuần", "/top-tuan.html"),
        ("Top Tháng", "/top-thang.html"),
        ("Yêu thích", "/truyen-yeu-thich.html"),
        ("Con trai", "/truyen-con-trai.html"),
        ("Con gái", "/truyen-con-gai.html"),
    ] {
        if let Ok(u) = Url::parse(&format!("{origin_str}{slug}"))
            && seen.insert(u.as_str().to_owned())
        {
            out.push(NavLink {
                label: label.to_owned(),
                url: u,
                kind: UrlKind::CatalogPage,
            });
        }
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
            if out.len() >= 96 {
                break;
            }
        }
    }
    out
}

#[expect(
    clippy::fn_params_excessive_bools,
    reason = "internal pagination helper receiving has_entries guard flag"
)]
fn extract_truyenqq_pagination(
    doc: &Html,
    base: &Url,
    page_url: &Url,
    url_kind: UrlKind,
    has_entries: bool,
) -> PaginationInfo {
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
    let mut template = template_from_numbered_url(page_url);

    for a in doc.select(&sel(
        ".page_redirect a[href], .pagination a[href], a[href*='/trang-'], a[href*='page=']",
    )) {
        let Some(href) = a.value().attr("href") else {
            continue;
        };
        let trimmed = href.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with("javascript:") {
            continue;
        }
        let Some(resolved) = abs_url(base, trimmed).map(|u| canonical_desktop_url(&u)) else {
            continue;
        };
        let parsed_p = page_num_re()
            .captures(resolved.path())
            .and_then(|c| c.get(1)?.as_str().parse::<u32>().ok())
            .or_else(|| {
                resolved
                    .query_pairs()
                    .find(|(k, _)| k.eq_ignore_ascii_case("page"))
                    .and_then(|(_, v)| v.parse::<u32>().ok())
            });
        if let Some(p) = parsed_p {
            if template.is_none() {
                template = template_from_numbered_url(&resolved);
            }
            if p > max_page && p <= 50_000 {
                max_page = p;
            }
            if p == current_page.saturating_sub(1) && current_page > 1 && prev_url.is_none() {
                prev_url = Some(resolved.clone());
            }
            if p == current_page + 1 && next_url.is_none() {
                next_url = Some(resolved);
            }
        }
    }

    if template.is_none() && has_entries {
        let sample_p2 = build_truyenqq_page_url(page_url, 2);
        template = template_from_numbered_url(&sample_p2);
    }
    if next_url.is_none()
        && has_entries
        && (url_kind == UrlKind::WebsiteHome || max_page > current_page)
    {
        next_url = Some(build_truyenqq_page_url(page_url, current_page + 1));
    }
    if prev_url.is_none() && current_page > 1 {
        prev_url = Some(build_truyenqq_page_url(page_url, current_page - 1));
    }

    PaginationInfo {
        current_page,
        total_pages: (max_page > 1).then_some(max_page),
        prev_url,
        next_url,
        page_url_template: template,
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

fn cat_id_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?i)/the-loai/([a-z0-9-]+?)-(\d+)(?:\.html?)?$")
            .expect("valid truyenqq cat regex")
    })
}

/// Builds the full `TruyenQQ` filter schema (`tim-kiem-nang-cao.html`), merging built-in genre IDs
/// with any newly added `/the-loai/<slug>-<id>.html` links or `<select>` controls found in `doc`.
#[must_use]
#[expect(
    clippy::too_many_lines,
    reason = "declares complete TruyenQQ built-in filter axes and dynamic DOM tag extraction"
)]
pub fn truyenqq_filter_schema(
    origin: &str,
    doc: Option<&Html>,
    base: Option<&Url>,
) -> SiteFilterSchema {
    let clean_origin = origin.trim_end_matches('/');
    let mut tags: Vec<FilterTagOption> = Vec::new();
    let mut seen_ids = HashSet::new();

    if let (Some(d), Some(b)) = (doc, base) {
        let data_genre_sel = sel("[data-id][class*='genre'], .genre-item[data-id], span[data-id]");
        let link_sel = sel("a[href]");
        for el in d.select(&data_genre_sel) {
            if let Some(id) = el.value().attr("data-id").map(str::trim)
                && !id.is_empty()
                && id.bytes().all(|c| c.is_ascii_digit())
            {
                let label = clean_text(&el.text().collect::<String>());
                if !label.is_empty() && label.len() <= 40 && seen_ids.insert(id.to_owned()) {
                    tags.push(FilterTagOption {
                        id: id.to_owned(),
                        label,
                        url: None,
                    });
                }
            }
        }
        for a in d.select(&link_sel) {
            let Some(href) = a.value().attr("href") else {
                continue;
            };
            let Some(resolved) = abs_url(b, href) else {
                continue;
            };
            if let Some(caps) = cat_id_re().captures(resolved.path())
                && let Some(id) = caps.get(2).map(|m| m.as_str())
            {
                let label = clean_text(&a.text().collect::<String>());
                if !label.is_empty() && label.len() <= 40 && seen_ids.insert(id.to_owned()) {
                    tags.push(FilterTagOption {
                        id: id.to_owned(),
                        label,
                        url: Some(resolved.into()),
                    });
                }
            }
        }
    }

    for (id, slug, label) in TRUYENQQ_DEFAULT_GENRES {
        if seen_ids.insert((*id).to_owned()) {
            tags.push(FilterTagOption {
                id: (*id).to_owned(),
                label: (*label).to_owned(),
                url: Some(format!("{clean_origin}/the-loai/{slug}-{id}.html")),
            });
        }
    }

    let mut extra_axes = vec![
        FilterAxis {
            key: "status".to_owned(),
            label: "Tình trạng".to_owned(),
            kind: FilterAxisKind::SingleSelect,
            options: vec![
                FilterOptionItem {
                    value: "-1".to_owned(),
                    label: "Tất cả".to_owned(),
                },
                FilterOptionItem {
                    value: "0".to_owned(),
                    label: "Đang tiến hành".to_owned(),
                },
                FilterOptionItem {
                    value: "2".to_owned(),
                    label: "Hoàn thành".to_owned(),
                },
            ],
        },
        FilterAxis {
            key: "country".to_owned(),
            label: "Quốc gia".to_owned(),
            kind: FilterAxisKind::SingleSelect,
            options: vec![
                FilterOptionItem {
                    value: "0".to_owned(),
                    label: "Tất cả".to_owned(),
                },
                FilterOptionItem {
                    value: "1".to_owned(),
                    label: "Trung Quốc (Manhua)".to_owned(),
                },
                FilterOptionItem {
                    value: "2".to_owned(),
                    label: "Việt Nam".to_owned(),
                },
                FilterOptionItem {
                    value: "3".to_owned(),
                    label: "Hàn Quốc (Manhwa)".to_owned(),
                },
                FilterOptionItem {
                    value: "4".to_owned(),
                    label: "Nhật Bản (Manga)".to_owned(),
                },
                FilterOptionItem {
                    value: "5".to_owned(),
                    label: "Âu Mỹ (Comic)".to_owned(),
                },
            ],
        },
        FilterAxis {
            key: "sort".to_owned(),
            label: "Sắp xếp".to_owned(),
            kind: FilterAxisKind::SingleSelect,
            options: vec![
                FilterOptionItem {
                    value: "0".to_owned(),
                    label: "Ngày cập nhật giảm dần".to_owned(),
                },
                FilterOptionItem {
                    value: "1".to_owned(),
                    label: "Ngày cập nhật tăng dần".to_owned(),
                },
                FilterOptionItem {
                    value: "2".to_owned(),
                    label: "Ngày đăng mới nhất".to_owned(),
                },
                FilterOptionItem {
                    value: "4".to_owned(),
                    label: "Lượt xem nhiều nhất".to_owned(),
                },
                FilterOptionItem {
                    value: "5".to_owned(),
                    label: "Lượt theo dõi nhiều nhất".to_owned(),
                },
            ],
        },
    ];

    if let Some(d) = doc {
        for discovered in extract_select_axes(d) {
            if !discovered.key.eq_ignore_ascii_case("minchapter")
                && !extra_axes
                    .iter()
                    .any(|a| a.key.eq_ignore_ascii_case(&discovered.key))
            {
                extra_axes.push(discovered);
            }
        }
    }

    SiteFilterSchema {
        filter_endpoint: Some(format!("{clean_origin}/tim-kiem-nang-cao.html")),
        include_tag_param: Some("category".to_owned()),
        exclude_tag_param: Some("notcategory".to_owned()),
        comma_joined_tags: true,
        min_chapters_param: Some("minchapter".to_owned()),
        keyword_param: None,
        tags,
        extra_axes,
    }
}

/// Builds a native `TruyenQQ` filtered catalog/search/pagination URL.
pub(crate) fn build_truyenqq_filtered_url(
    base_url: &Url,
    current_url: Option<&Url>,
    clean_q: Option<&str>,
    filters: &CatalogFilterQuery,
    page_num: u32,
) -> Result<(Url, UnhandledFilterMask), ScrapeError> {
    let origin = canonical_desktop_url(base_url);
    let origin_str = format!(
        "{}://{}",
        origin.scheme(),
        origin.host_str().unwrap_or("truyenqq.com.vn")
    );

    let has_adv = !filters.include_tags.is_empty()
        || !filters.exclude_tags.is_empty()
        || filters.min_chapters.unwrap_or(0) > 0
        || filters
            .extra
            .values()
            .any(|vals| vals.iter().any(|v| !v.trim().is_empty()));

    let unhandled = UnhandledFilterMask {
        include_tags: false,
        exclude_tags: false,
        min_chapters: false,
        updated_within: filters.updated_within != UpdatedWithinFilter::Any,
    };

    if let Some(q) = clean_q
        && !has_adv
    {
        let path = if page_num > 1 {
            format!("{origin_str}/tim-kiem/trang-{page_num}.html")
        } else {
            format!("{origin_str}/tim-kiem.html")
        };
        let mut u = Url::parse(&path).map_err(|e| ScrapeError::InvalidUrl(e.to_string()))?;
        u.query_pairs_mut().append_pair("q", q);
        return Ok((u, unhandled));
    }

    if has_adv {
        let path = if page_num > 1 {
            format!("{origin_str}/tim-kiem-nang-cao/trang-{page_num}.html")
        } else {
            format!("{origin_str}/tim-kiem-nang-cao.html")
        };
        let mut u = Url::parse(&path).map_err(|e| ScrapeError::InvalidUrl(e.to_string()))?;
        {
            let mut qp = u.query_pairs_mut();
            if !filters.include_tags.is_empty() {
                qp.append_pair("category", &filters.include_tags.join(","));
            }
            if !filters.exclude_tags.is_empty() {
                qp.append_pair("notcategory", &filters.exclude_tags.join(","));
            }
            let status = filters
                .extra
                .get("status")
                .and_then(|v| v.first())
                .map_or("-1", String::as_str);
            let country = filters
                .extra
                .get("country")
                .and_then(|v| v.first())
                .map_or("0", String::as_str);
            let sort = filters
                .extra
                .get("sort")
                .and_then(|v| v.first())
                .map_or("0", String::as_str);
            let min_ch = filters.min_chapters.unwrap_or(0);
            qp.append_pair("country", country);
            qp.append_pair("status", status);
            qp.append_pair("minchapter", &min_ch.to_string());
            qp.append_pair("sort", sort);
            for (k, vals) in &filters.extra {
                if !matches!(k.as_str(), "status" | "country" | "sort") {
                    for v in vals {
                        if !v.trim().is_empty() {
                            qp.append_pair(k, v.trim());
                        }
                    }
                }
            }
        }
        let mut mask = unhandled;
        if clean_q.is_some() {
            mask.include_tags = false;
        }
        return Ok((u, mask));
    }

    let base_listing = current_url.unwrap_or(&origin);
    let target = build_truyenqq_page_url(base_listing, page_num);
    Ok((target, unhandled))
}

const TRUYENQQ_DEFAULT_GENRES: &[(&str, &str, &str)] = &[
    ("26", "action", "Action"),
    ("27", "adventure", "Adventure"),
    ("62", "anime", "Anime"),
    ("91", "chuyen-sinh", "Chuyển Sinh"),
    ("90", "co-dai", "Cổ Đại"),
    ("28", "comedy", "Comedy"),
    ("60", "comic", "Comic"),
    ("58", "cooking", "Cooking"),
    ("29", "drama", "Drama"),
    ("30", "fantasy", "Fantasy"),
    ("45", "gender-bender", "Gender Bender"),
    ("47", "harem", "Harem"),
    ("51", "historical", "Historical"),
    ("44", "horror", "Horror"),
    ("96", "isekai", "Isekai"),
    ("54", "josei", "Josei"),
    ("35", "manhua", "Manhua"),
    ("49", "manhwa", "Manhwa"),
    ("41", "martial-arts", "Martial Arts"),
    ("48", "mature", "Mature"),
    ("50", "mecha", "Mecha"),
    ("39", "mystery", "Mystery"),
    ("87", "ngon-tinh", "Ngôn Tình"),
    ("95", "one-shot", "One Shot"),
    ("40", "psychological", "Psychological"),
    ("36", "romance", "Romance"),
    ("37", "school-life", "School Life"),
    ("43", "sci-fi", "Sci-Fi"),
    ("42", "seinen", "Seinen"),
    ("38", "shoujo", "Shoujo"),
    ("31", "shounen", "Shounen"),
    ("46", "slice-of-life", "Slice of Life"),
    ("57", "sports", "Sports"),
    ("32", "supernatural", "Supernatural"),
    ("52", "tragedy", "Tragedy"),
    ("56", "trinh-tham", "Trinh Thám"),
    ("92", "truyen-mau", "Truyện Màu"),
    ("55", "webtoon", "Webtoon"),
    ("88", "xuyen-khong", "Xuyên Không"),
];
