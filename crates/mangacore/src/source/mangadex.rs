//! Dedicated `MangaDex` adapter backed by the documented `MangaDex` REST API v5.
//!
//! All `mangadex.org` URLs are translated directly to `https://api.mangadex.org` requests before
//! any HTML fetch is attempted. This avoids client-side SPA limitations and bypasses ISP-level
//! SNI/TLS blocks that affect the `mangadex.org` web frontend while `api.mangadex.org` remains
//! accessible.

use std::collections::{BTreeSet, HashMap};

use async_trait::async_trait;
use serde::Deserialize;
use url::Url;

use crate::error::ScrapeError;
use crate::http::HttpClient;
use crate::model::{
    CatalogEntry, CatalogPage, Chapter, ChapterRef, Manga, NavLink, PageImage, PaginationInfo,
    Resolved, SiteId,
};
use crate::parse::filters::{
    CatalogFilterQuery, FilterAxis, FilterAxisKind, FilterOptionItem, FilterTagOption,
    SiteFilterSchema, UnhandledFilterMask, UpdatedWithinFilter,
};
use crate::recognize::UrlKind;
use crate::source::Source;

/// Official `MangaDex` REST API v5 origin.
pub const API_BASE: &str = "https://api.mangadex.org";

/// Official `MangaDex` cover image CDN origin.
pub const UPLOADS_BASE: &str = "https://uploads.mangadex.org";

/// User-Agent sent to `MangaDex` API endpoints per API documentation guidelines.
const MANGADEX_USER_AGENT: &str = "RtMaga/2.4.0 (+https://github.com/DUXGVEB/Rt-Maga)";

/// Page size for catalog and search listings.
const CATALOG_LIMIT: u32 = 24;

/// Maximum chapter feed pages fetched per manga detail request (`500 * 4 = 2000` chapters).
const MAX_FEED_PAGES: u32 = 4;

/// Dedicated source adapter for `MangaDex` (`mangadex.org` and `api.mangadex.org`).
#[derive(Debug, Default, Clone, Copy)]
pub struct MangaDexSource;

/// Returns `true` if `host_lower` belongs to `MangaDex`.
#[must_use]
pub fn is_mangadex_host(host_lower: &str) -> bool {
    matches!(
        host_lower,
        "mangadex.org" | "www.mangadex.org" | "api.mangadex.org"
    )
}

/// Classifies a `MangaDex` URL into its semantic [`UrlKind`], returning `None` for other sites.
#[must_use]
pub fn classify_url(url: &Url) -> Option<UrlKind> {
    let host = url.host_str()?.to_ascii_lowercase();
    if host == "uploads.mangadex.org" {
        return Some(UrlKind::ImageUrl);
    }
    if !is_mangadex_host(&host) {
        return None;
    }

    let segments: Vec<&str> = url
        .path_segments()
        .map(|s| s.filter(|p| !p.is_empty()).collect())
        .unwrap_or_default();

    if host == "api.mangadex.org" {
        return Some(match segments.as_slice() {
            ["manga", id, ..] if is_uuid(id) => UrlKind::MangaDetail,
            ["chapter", id, ..] | ["at-home", "server", id, ..] if is_uuid(id) => {
                UrlKind::ChapterPage
            }
            ["manga"] => {
                if query_param(url, "title").is_some() {
                    UrlKind::SearchPage
                } else {
                    UrlKind::CatalogPage
                }
            }
            _ => UrlKind::ApiEndpoint,
        });
    }

    if query_param(url, "q").is_some() || query_param(url, "title").is_some() {
        return Some(UrlKind::SearchPage);
    }

    Some(match segments.as_slice() {
        [] => UrlKind::WebsiteHome,
        ["title" | "manga", id, ..] if is_uuid(id) => UrlKind::MangaDetail,
        ["chapter", id, ..] if is_uuid(id) => UrlKind::ChapterPage,
        ["search"] => UrlKind::SearchPage,
        ["titles" | "tag" | "group" | "author" | "list", ..] => UrlKind::CatalogPage,
        _ => UrlKind::UnsupportedUrl,
    })
}

/// Validates a canonical hyphenated UUID (36 ASCII characters: `8-4-4-4-12`).
#[must_use]
pub fn is_uuid(s: &str) -> bool {
    if s.len() != 36 {
        return false;
    }
    let bytes = s.as_bytes();
    for (idx, &b) in bytes.iter().enumerate() {
        if matches!(idx, 8 | 13 | 18 | 23) {
            if b != b'-' {
                return false;
            }
        } else if !b.is_ascii_hexdigit() {
            return false;
        }
    }
    true
}

fn query_param(url: &Url, key: &str) -> Option<String> {
    url.query_pairs()
        .find(|(k, _)| k.eq_ignore_ascii_case(key))
        .map(|(_, v)| v.trim().to_owned())
        .filter(|v| !v.is_empty())
}

fn page_number_from_url(url: &Url) -> u32 {
    if let Some(p) = query_param(url, "page").and_then(|v| v.parse::<u32>().ok()) {
        return p.max(1);
    }
    if let Some(offset) = query_param(url, "offset").and_then(|v| v.parse::<u32>().ok()) {
        return (offset / CATALOG_LIMIT) + 1;
    }
    1
}

fn extract_manga_uuid(url: &Url) -> Option<String> {
    let segs: Vec<&str> = url.path_segments()?.filter(|s| !s.is_empty()).collect();
    match segs.as_slice() {
        ["title" | "manga", id, ..] if is_uuid(id) => Some((*id).to_ascii_lowercase()),
        _ => None,
    }
}

fn extract_chapter_uuid(url: &Url) -> Option<String> {
    let segs: Vec<&str> = url.path_segments()?.filter(|s| !s.is_empty()).collect();
    match segs.as_slice() {
        ["chapter", id, ..] | ["at-home", "server", id, ..] if is_uuid(id) => {
            Some((*id).to_ascii_lowercase())
        }
        _ => None,
    }
}

fn extract_tag_uuid(url: &Url) -> Option<String> {
    let segs: Vec<&str> = url.path_segments()?.filter(|s| !s.is_empty()).collect();
    match segs.as_slice() {
        ["tag", id, ..] if is_uuid(id) => Some((*id).to_ascii_lowercase()),
        _ => None,
    }
}

fn canonical_manga_url(id: &str) -> Result<Url, ScrapeError> {
    Url::parse(&format!("https://mangadex.org/title/{id}"))
        .map_err(|e| ScrapeError::InvalidUrl(e.to_string()))
}

fn canonical_chapter_url(id: &str) -> Result<Url, ScrapeError> {
    Url::parse(&format!("https://mangadex.org/chapter/{id}"))
        .map_err(|e| ScrapeError::InvalidUrl(e.to_string()))
}

#[derive(Debug, Deserialize)]
struct MdResponse<T> {
    result: String,
    data: T,
    #[serde(default)]
    total: Option<u32>,
    #[serde(default)]
    limit: Option<u32>,
    #[serde(default)]
    offset: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct MdEntity<A> {
    id: String,
    #[serde(default)]
    attributes: A,
    #[serde(default)]
    relationships: Vec<MdRelationship>,
}

#[derive(Debug, Deserialize)]
struct MdRelationship {
    #[serde(rename = "type")]
    rel_type: String,
    id: String,
    #[serde(default)]
    attributes: Option<serde_json::Value>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MdMangaAttrs {
    #[serde(default)]
    title: HashMap<String, String>,
    #[serde(default)]
    alt_titles: Vec<HashMap<String, String>>,
    #[serde(default)]
    description: HashMap<String, String>,
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    original_language: Option<String>,
    #[serde(default)]
    available_translated_languages: Vec<Option<String>>,
    #[serde(default)]
    tags: Vec<MdTag>,
}

#[derive(Debug, Deserialize)]
struct MdTag {
    #[serde(default)]
    attributes: MdTagAttrs,
}

#[derive(Debug, Default, Deserialize)]
struct MdTagAttrs {
    #[serde(default)]
    name: HashMap<String, String>,
    #[serde(default)]
    group: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MdChapterAttrs {
    #[serde(default)]
    volume: Option<String>,
    #[serde(default)]
    chapter: Option<String>,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    translated_language: Option<String>,
    #[serde(default)]
    publish_at: Option<String>,
    #[serde(default)]
    external_url: Option<String>,
    #[serde(default)]
    pages: u32,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MdAtHomeResponse {
    result: String,
    base_url: String,
    chapter: MdAtHomeChapter,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MdAtHomeChapter {
    hash: String,
    #[serde(default)]
    data: Vec<String>,
    #[serde(default)]
    data_saver: Vec<String>,
}

fn pick_localized(map: &HashMap<String, String>, preferred: &[&str]) -> Option<String> {
    for lang in preferred {
        if let Some(val) = map.get(*lang).map(|s| s.trim()).filter(|s| !s.is_empty()) {
            return Some(val.to_owned());
        }
    }
    let mut keys: Vec<&String> = map.keys().collect();
    keys.sort();
    for key in keys {
        if let Some(val) = map.get(key).map(|s| s.trim()).filter(|s| !s.is_empty()) {
            return Some(val.to_owned());
        }
    }
    None
}

fn extract_cover_url(
    manga_id: &str,
    relationships: &[MdRelationship],
    size_suffix: &str,
) -> Option<Url> {
    let filename = relationships.iter().find_map(|rel| {
        if rel.rel_type != "cover_art" {
            return None;
        }
        rel.attributes
            .as_ref()?
            .get("fileName")?
            .as_str()
            .map(str::trim)
            .filter(|s| !s.is_empty())
    })?;
    Url::parse(&format!(
        "{UPLOADS_BASE}/covers/{manga_id}/{filename}{size_suffix}"
    ))
    .ok()
}

fn extract_rel_names(relationships: &[MdRelationship], rel_type: &str) -> Vec<String> {
    let mut out = Vec::new();
    for rel in relationships {
        if rel.rel_type != rel_type {
            continue;
        }
        if let Some(name) = rel
            .attributes
            .as_ref()
            .and_then(|a| a.get("name"))
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            && !out
                .iter()
                .any(|existing: &String| existing.eq_ignore_ascii_case(name))
        {
            out.push(name.to_owned());
        }
    }
    out
}

fn split_genres_and_tags(tags: &[MdTag]) -> (Vec<String>, Vec<String>) {
    let mut genres = Vec::new();
    let mut other_tags = Vec::new();
    for tag in tags {
        let Some(name) = pick_localized(&tag.attributes.name, &["en", "vi"]) else {
            continue;
        };
        if tag.attributes.group.as_deref() == Some("genre") {
            if !genres.contains(&name) {
                genres.push(name);
            }
        } else if !other_tags.contains(&name) {
            other_tags.push(name);
        }
    }
    (genres, other_tags)
}

fn collect_alt_titles(
    alt_maps: &[HashMap<String, String>],
    primary: &str,
    orig_lang: Option<&str>,
) -> (Vec<String>, Option<String>) {
    let mut alts = Vec::new();
    let mut original_title = None;
    for map in alt_maps {
        if original_title.is_none()
            && let Some(ol) = orig_lang
            && let Some(val) = map.get(ol).map(|s| s.trim()).filter(|s| !s.is_empty())
        {
            original_title = Some(val.to_owned());
        }
        let mut keys: Vec<&String> = map.keys().collect();
        keys.sort();
        for key in keys {
            if let Some(val) = map.get(key).map(|s| s.trim()).filter(|s| !s.is_empty())
                && !val.eq_ignore_ascii_case(primary)
                && !alts.iter().any(|a: &String| a.eq_ignore_ascii_case(val))
                && alts.len() < 12
            {
                alts.push(val.to_owned());
            }
        }
    }
    (alts, original_title)
}

/// Parses a `MangaDex` `/manga/{id}` JSON response into a [`Manga`] (with empty chapters until feed is merged).
///
/// # Errors
///
/// Returns [`ScrapeError::ApiFailure`] if the JSON structure or `result` field is invalid.
pub fn parse_manga_detail_json(body: &str, request_url: &Url) -> Result<Manga, ScrapeError> {
    let resp: MdResponse<MdEntity<MdMangaAttrs>> =
        serde_json::from_str(body).map_err(|e| ScrapeError::ApiFailure {
            service: "mangadex".to_owned(),
            url: request_url.as_str().to_owned(),
            status: None,
            detail: format!("invalid manga detail json: {e}"),
        })?;

    if resp.result != "ok" {
        return Err(ScrapeError::ApiFailure {
            service: "mangadex".to_owned(),
            url: request_url.as_str().to_owned(),
            status: None,
            detail: format!("unexpected MangaDex result status '{}'", resp.result),
        });
    }

    let entity = resp.data;
    let id = entity.id;
    let url = canonical_manga_url(&id)?;
    let title = pick_localized(&entity.attributes.title, &["en", "vi", "ja-ro", "ja"]).ok_or_else(
        || ScrapeError::MissingMetadata {
            url: url.as_str().to_owned(),
            detail: "MangaDex manga payload contained no title".to_owned(),
        },
    )?;

    let (alt_titles, original_title) = collect_alt_titles(
        &entity.attributes.alt_titles,
        &title,
        entity.attributes.original_language.as_deref(),
    );
    let description = pick_localized(&entity.attributes.description, &["vi", "en", "ja"]);
    let cover = extract_cover_url(&id, &entity.relationships, ".512.jpg");
    let authors = extract_rel_names(&entity.relationships, "author");
    let artists = extract_rel_names(&entity.relationships, "artist");
    let (genres, tags) = split_genres_and_tags(&entity.attributes.tags);
    let mut available_languages: Vec<String> = entity
        .attributes
        .available_translated_languages
        .into_iter()
        .flatten()
        .filter(|s| !s.trim().is_empty())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    available_languages.sort();

    Ok(Manga {
        canonical_url: Some(url.clone()),
        url,
        site: SiteId::known("mangadex"),
        title,
        cover,
        chapters: Vec::new(),
        source_id: Some(format!("mangadex:{id}")),
        alt_titles,
        original_title,
        authors,
        artists,
        description,
        genres,
        tags,
        status: entity.attributes.status,
        translation_status: None,
        language: entity.attributes.original_language,
        available_languages,
        chapter_count: None,
    })
}

/// Parses a `MangaDex` `/manga/{id}/feed` JSON response into `(chapters, total_reported)`.
///
/// # Errors
///
/// Returns [`ScrapeError::ApiFailure`] if the JSON structure is malformed.
pub fn parse_chapter_feed_json(
    body: &str,
    request_url: &Url,
) -> Result<(Vec<ChapterRef>, u32), ScrapeError> {
    let resp: MdResponse<Vec<MdEntity<MdChapterAttrs>>> =
        serde_json::from_str(body).map_err(|e| ScrapeError::ApiFailure {
            service: "mangadex".to_owned(),
            url: request_url.as_str().to_owned(),
            status: None,
            detail: format!("invalid chapter feed json: {e}"),
        })?;

    if resp.result != "ok" {
        return Err(ScrapeError::ApiFailure {
            service: "mangadex".to_owned(),
            url: request_url.as_str().to_owned(),
            status: None,
            detail: format!("unexpected MangaDex feed result '{}'", resp.result),
        });
    }

    let mut chapters = Vec::new();
    for item in resp.data {
        // Skip chapters hosted only on external paywalled websites with 0 readable MangaDex pages.
        if item.attributes.external_url.is_some() && item.attributes.pages == 0 {
            continue;
        }
        let Ok(ch_url) = canonical_chapter_url(&item.id) else {
            continue;
        };
        let num_str = item.attributes.chapter.as_deref().map(str::trim);
        let number = num_str.and_then(|s| s.parse::<f32>().ok());
        let title_part = item
            .attributes
            .title
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty());

        let base_label = match (num_str.filter(|s| !s.is_empty()), title_part) {
            (Some(n), Some(t)) => format!("Chapter {n} - {t}"),
            (Some(n), None) => format!("Chapter {n}"),
            (None, Some(t)) => t.to_owned(),
            (None, None) => "Oneshot".to_owned(),
        };

        chapters.push(ChapterRef {
            url: ch_url,
            name: base_label,
            number,
            volume: item.attributes.volume.filter(|v| !v.trim().is_empty()),
            published_at: item
                .attributes
                .publish_at
                .map(|ts| ts.chars().take(10).collect()),
            language: item
                .attributes
                .translated_language
                .filter(|l| !l.trim().is_empty()),
            source_id: Some(format!("mangadex:{}", item.id)),
        });
    }

    let total = resp
        .total
        .unwrap_or_else(|| u32::try_from(chapters.len()).unwrap_or(u32::MAX));
    Ok((chapters, total))
}

/// Parses a `MangaDex` `/manga` collection JSON response into a [`CatalogPage`].
///
/// # Errors
///
/// Returns [`ScrapeError::ApiFailure`] if the JSON payload is invalid.
pub fn parse_catalog_json(
    body: &str,
    submitted_url: &Url,
    url_kind: UrlKind,
    query: Option<String>,
) -> Result<CatalogPage, ScrapeError> {
    let resp: MdResponse<Vec<MdEntity<MdMangaAttrs>>> =
        serde_json::from_str(body).map_err(|e| ScrapeError::ApiFailure {
            service: "mangadex".to_owned(),
            url: submitted_url.as_str().to_owned(),
            status: None,
            detail: format!("invalid catalog json: {e}"),
        })?;

    if resp.result != "ok" {
        return Err(ScrapeError::ApiFailure {
            service: "mangadex".to_owned(),
            url: submitted_url.as_str().to_owned(),
            status: None,
            detail: format!("unexpected MangaDex catalog result '{}'", resp.result),
        });
    }

    let mut entries = Vec::with_capacity(resp.data.len());
    for item in resp.data {
        let Ok(detail_url) = canonical_manga_url(&item.id) else {
            continue;
        };
        let Some(title) = pick_localized(&item.attributes.title, &["en", "vi", "ja-ro", "ja"])
        else {
            continue;
        };
        let (alt_titles, _) = collect_alt_titles(
            &item.attributes.alt_titles,
            &title,
            item.attributes.original_language.as_deref(),
        );
        let cover = extract_cover_url(&item.id, &item.relationships, ".256.jpg");
        let (genres, _) = split_genres_and_tags(&item.attributes.tags);
        entries.push(CatalogEntry {
            url: detail_url,
            title,
            alt_titles,
            cover,
            latest_chapter: None,
            status: item.attributes.status,
            genres,
            source_id: Some(format!("mangadex:{}", item.id)),
            chapter_count: None,
            updated_at: None,
        });
    }

    let limit = resp.limit.unwrap_or(CATALOG_LIMIT).max(1);
    let offset = resp.offset.unwrap_or(0);
    let current_page = (offset / limit) + 1;
    let total_pages = resp.total.map(|tot| tot.div_ceil(limit).max(1));

    let prev_url = if current_page > 1 {
        build_browse_page_url(submitted_url, current_page - 1, query.as_deref())
    } else {
        None
    };
    let next_url = if total_pages.is_none_or(|tp| current_page < tp) && !entries.is_empty() {
        build_browse_page_url(submitted_url, current_page + 1, query.as_deref())
    } else {
        None
    };
    let page_url_template = build_browse_page_url(submitted_url, 2, query.as_deref())
        .and_then(|u| crate::parse::pagination::template_from_numbered_url(&u));

    let page_title = match &query {
        Some(q) => format!("MangaDex Search: {q}"),
        None if url_kind == UrlKind::WebsiteHome => "MangaDex — Latest Updates".to_owned(),
        None => "MangaDex Catalog".to_owned(),
    };

    Ok(CatalogPage {
        url: submitted_url.clone(),
        site: SiteId::known("mangadex"),
        site_name: "MangaDex".to_owned(),
        title: page_title,
        url_kind,
        query,
        search_url_template: Some("https://mangadex.org/search?q={query}".to_owned()),
        entries,
        categories: mangadex_categories(),
        pagination: PaginationInfo {
            current_page,
            total_pages,
            prev_url,
            next_url,
            page_url_template,
        },
        filter_schema: mangadex_filter_schema(),
    })
}

fn build_browse_page_url(base: &Url, page: u32, query: Option<&str>) -> Option<Url> {
    let mut url = if let Some(q) = query {
        let mut u = Url::parse("https://mangadex.org/search").ok()?;
        u.query_pairs_mut().append_pair("q", q);
        u
    } else if base
        .host_str()
        .is_some_and(|h| h.eq_ignore_ascii_case("api.mangadex.org"))
    {
        Url::parse("https://mangadex.org/titles/latest").ok()?
    } else {
        let mut u = base.clone();
        let kept: Vec<(String, String)> = u
            .query_pairs()
            .filter(|(k, _)| k != "page" && k != "offset")
            .map(|(k, v)| (k.into_owned(), v.into_owned()))
            .collect();
        u.query_pairs_mut().clear().extend_pairs(kept);
        u
    };
    if page > 1 {
        url.query_pairs_mut().append_pair("page", &page.to_string());
    }
    Some(url)
}

fn mangadex_categories() -> Vec<NavLink> {
    [
        (
            "Latest Updates",
            "https://mangadex.org/titles/latest",
            UrlKind::CatalogPage,
        ),
        (
            "Popular",
            "https://mangadex.org/titles?order=followedCount",
            UrlKind::CatalogPage,
        ),
        (
            "Top Rated",
            "https://mangadex.org/titles?order=rating",
            UrlKind::CatalogPage,
        ),
        (
            "Recently Added",
            "https://mangadex.org/titles/recent",
            UrlKind::CatalogPage,
        ),
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

/// Parses a `MangaDex` `/at-home/server/{chapterId}` response into ordered [`PageImage`] entries.
///
/// # Errors
///
/// Returns [`ScrapeError::ApiFailure`] or [`ScrapeError::MissingImages`] on invalid/empty payloads.
pub fn parse_at_home_json(
    body: &str,
    chapter_url: &Url,
    request_url: &Url,
) -> Result<Vec<PageImage>, ScrapeError> {
    let resp: MdAtHomeResponse =
        serde_json::from_str(body).map_err(|e| ScrapeError::ApiFailure {
            service: "mangadex".to_owned(),
            url: request_url.as_str().to_owned(),
            status: None,
            detail: format!("invalid at-home server json: {e}"),
        })?;

    if resp.result != "ok" {
        return Err(ScrapeError::ApiFailure {
            service: "mangadex".to_owned(),
            url: request_url.as_str().to_owned(),
            status: None,
            detail: format!("at-home server returned result '{}'", resp.result),
        });
    }

    let base = resp.base_url.trim_end_matches('/');
    let hash = resp.chapter.hash.trim();
    if base.is_empty() || hash.is_empty() {
        return Err(ScrapeError::MissingImages {
            url: chapter_url.clone(),
        });
    }

    let referer = Url::parse("https://mangadex.org/").unwrap_or_else(|_| chapter_url.clone());
    let mut images = Vec::new();
    let (files, primary_mode) = if resp.chapter.data.is_empty() {
        (&resp.chapter.data_saver, "data-saver")
    } else {
        (&resp.chapter.data, "data")
    };

    for (idx, file) in files.iter().enumerate() {
        let Ok(primary_url) = Url::parse(&format!("{base}/{primary_mode}/{hash}/{file}")) else {
            continue;
        };
        let mut fallbacks = Vec::new();
        if primary_mode == "data"
            && let Some(saver_file) = resp.chapter.data_saver.get(idx)
            && let Ok(fb) = Url::parse(&format!("{base}/data-saver/{hash}/{saver_file}"))
        {
            fallbacks.push(fb);
        }
        images.push(PageImage {
            url: primary_url,
            fallbacks,
            referer: referer.clone(),
        });
    }

    if images.is_empty() {
        return Err(ScrapeError::MissingImages {
            url: chapter_url.clone(),
        });
    }
    Ok(images)
}

/// Extracts `(parent_manga_uuid, chapter_label)` from a `MangaDex` `/chapter/{id}?includes[]=manga` JSON response.
///
/// # Errors
///
/// Returns [`ScrapeError::ApiFailure`] if the JSON structure is malformed.
pub fn parse_chapter_meta_json(
    body: &str,
    request_url: &Url,
) -> Result<(Option<String>, String, Option<f32>), ScrapeError> {
    let resp: MdResponse<MdEntity<MdChapterAttrs>> =
        serde_json::from_str(body).map_err(|e| ScrapeError::ApiFailure {
            service: "mangadex".to_owned(),
            url: request_url.as_str().to_owned(),
            status: None,
            detail: format!("invalid chapter metadata json: {e}"),
        })?;

    let parent_id = resp
        .data
        .relationships
        .iter()
        .find(|r| r.rel_type == "manga")
        .map(|r| r.id.clone());

    let num_str = resp.data.attributes.chapter.as_deref().map(str::trim);
    let number = num_str.and_then(|s| s.parse::<f32>().ok());
    let title = resp
        .data
        .attributes
        .title
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let name = match (num_str.filter(|s| !s.is_empty()), title) {
        (Some(n), Some(t)) => format!("Chapter {n} - {t}"),
        (Some(n), None) => format!("Chapter {n}"),
        (None, Some(t)) => t.to_owned(),
        (None, None) => "Chapter".to_owned(),
    };
    Ok((parent_id, name, number))
}

impl MangaDexSource {
    async fn fetch_manga_by_id(&self, http: &HttpClient, id: &str) -> Result<Manga, ScrapeError> {
        let mut detail_url = Url::parse(&format!("{API_BASE}/manga/{id}"))
            .map_err(|e| ScrapeError::InvalidUrl(e.to_string()))?;
        detail_url
            .query_pairs_mut()
            .append_pair("includes[]", "cover_art")
            .append_pair("includes[]", "author")
            .append_pair("includes[]", "artist");

        let detail_resp = http
            .fetch_json(&detail_url, "mangadex", Some(MANGADEX_USER_AGENT))
            .await?;
        let mut manga = parse_manga_detail_json(&detail_resp.html, &detail_url)?;

        let mut all_chapters = Vec::new();
        for page_idx in 0..MAX_FEED_PAGES {
            let offset = page_idx * 500;
            let mut feed_url = Url::parse(&format!("{API_BASE}/manga/{id}/feed"))
                .map_err(|e| ScrapeError::InvalidUrl(e.to_string()))?;
            feed_url
                .query_pairs_mut()
                .append_pair("limit", "500")
                .append_pair("offset", &offset.to_string())
                .append_pair("order[volume]", "asc")
                .append_pair("order[chapter]", "asc")
                .append_pair("contentRating[]", "safe")
                .append_pair("contentRating[]", "suggestive")
                .append_pair("contentRating[]", "erotica")
                .append_pair("contentRating[]", "pornographic")
                .append_pair("includeEmptyPages", "0");

            let feed_resp = http
                .fetch_json(&feed_url, "mangadex", Some(MANGADEX_USER_AGENT))
                .await?;
            let (batch, total) = parse_chapter_feed_json(&feed_resp.html, &feed_url)?;
            let batch_empty = batch.is_empty();
            all_chapters.extend(batch);
            if batch_empty || offset + 500 >= total {
                break;
            }
        }

        // Preserve stable reading order (numeric chapter ascending, then published_at).
        all_chapters.sort_by(|a, b| match (a.number, b.number) {
            (Some(na), Some(nb)) => na
                .partial_cmp(&nb)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.language.cmp(&b.language))
                .then_with(|| a.url.as_str().cmp(b.url.as_str())),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => a.url.as_str().cmp(b.url.as_str()),
        });

        manga.chapter_count = u32::try_from(all_chapters.len()).ok();
        manga.chapters = all_chapters;
        Ok(manga)
    }

    async fn fetch_catalog(
        &self,
        http: &HttpClient,
        url: &Url,
        url_kind: UrlKind,
    ) -> Result<CatalogPage, ScrapeError> {
        let page = page_number_from_url(url);
        let offset = page.saturating_sub(1).saturating_mul(CATALOG_LIMIT);
        let query = query_param(url, "q").or_else(|| query_param(url, "title"));
        let tag_id = extract_tag_uuid(url);
        let order_param = query_param(url, "order");

        let mut api_url = Url::parse(&format!("{API_BASE}/manga"))
            .map_err(|e| ScrapeError::InvalidUrl(e.to_string()))?;
        {
            let mut pairs = api_url.query_pairs_mut();
            pairs.append_pair("limit", &CATALOG_LIMIT.to_string());
            pairs.append_pair("offset", &offset.to_string());
            pairs.append_pair("includes[]", "cover_art");
            pairs.append_pair("hasAvailableChapters", "true");
            pairs.append_pair("contentRating[]", "safe");
            pairs.append_pair("contentRating[]", "suggestive");
            pairs.append_pair("contentRating[]", "erotica");

            if let Some(q) = query.as_deref() {
                pairs.append_pair("title", q);
                pairs.append_pair("order[relevance]", "desc");
            } else if url.path().contains("/recent") {
                pairs.append_pair("order[createdAt]", "desc");
            } else if order_param.as_deref() == Some("followedCount") {
                pairs.append_pair("order[followedCount]", "desc");
            } else if order_param.as_deref() == Some("rating") {
                pairs.append_pair("order[rating]", "desc");
            } else {
                pairs.append_pair("order[latestUploadedChapter]", "desc");
            }

            if let Some(tid) = tag_id.as_deref() {
                pairs.append_pair("includedTags[]", tid);
            }
            for (k, v) in url.query_pairs() {
                let val = v.trim();
                if val.is_empty() {
                    continue;
                }
                match k.as_ref() {
                    "includedTags[]" | "includedTags" if is_uuid(val) => {
                        pairs.append_pair("includedTags[]", val);
                    }
                    "excludedTags[]" | "excludedTags" if is_uuid(val) => {
                        pairs.append_pair("excludedTags[]", val);
                    }
                    "status[]" | "status" => {
                        pairs.append_pair("status[]", val);
                    }
                    "publicationDemographic[]" | "publicationDemographic" => {
                        pairs.append_pair("publicationDemographic[]", val);
                    }
                    _ => {}
                }
            }
        }

        let resp = http
            .fetch_json(&api_url, "mangadex", Some(MANGADEX_USER_AGENT))
            .await?;
        parse_catalog_json(&resp.html, url, url_kind, query)
    }
}

#[async_trait]
impl Source for MangaDexSource {
    fn id(&self) -> &'static str {
        "mangadex"
    }

    fn metadata(&self) -> crate::source::AdapterMetadata {
        crate::source::AdapterMetadata {
            id: "mangadex",
            name: "MangaDex",
            version: "1.2.0",
            domains: &["mangadex.org", "api.mangadex.org"],
            auth_support: crate::source::AdapterAuthSupport::PublicOnly,
            strategies: &[
                crate::ExtractionStrategyKind::SiteAdapter,
                crate::ExtractionStrategyKind::StructuredDataAndApi,
            ],
            known_limitations: &[
                "Chapters linked externally (externalUrl) open on publisher websites instead of MangaDex@Home.",
            ],
        }
    }

    fn matches(&self, url: &Url) -> bool {
        url.host_str()
            .is_some_and(|h| is_mangadex_host(&h.to_ascii_lowercase()))
    }

    async fn resolve(&self, http: &HttpClient, url: &Url) -> Result<Resolved, ScrapeError> {
        let kind = classify_url(url).unwrap_or(UrlKind::UnsupportedUrl);
        match kind {
            UrlKind::WebsiteHome | UrlKind::CatalogPage | UrlKind::SearchPage => {
                let catalog = self.fetch_catalog(http, url, kind).await?;
                Ok(Resolved::Catalog(Box::new(catalog)))
            }
            UrlKind::MangaDetail => {
                let id = extract_manga_uuid(url).ok_or_else(|| ScrapeError::NotRecognized {
                    url: url.clone(),
                    reason: "MangaDex manga URL is missing a valid UUID",
                })?;
                let manga = self.fetch_manga_by_id(http, &id).await?;
                Ok(Resolved::Manga(Box::new(manga)))
            }
            UrlKind::ChapterPage => {
                let ch_id =
                    extract_chapter_uuid(url).ok_or_else(|| ScrapeError::NotRecognized {
                        url: url.clone(),
                        reason: "MangaDex chapter URL is missing a valid UUID",
                    })?;
                let meta_url = Url::parse(&format!("{API_BASE}/chapter/{ch_id}?includes[]=manga"))
                    .map_err(|e| ScrapeError::InvalidUrl(e.to_string()))?;
                let meta_resp = http
                    .fetch_json(&meta_url, "mangadex", Some(MANGADEX_USER_AGENT))
                    .await?;
                let (parent_id, ch_name, ch_num) =
                    parse_chapter_meta_json(&meta_resp.html, &meta_url)?;
                let canon_ch_url = canonical_chapter_url(&ch_id)?;

                if let Some(pid) = parent_id {
                    let manga = self.fetch_manga_by_id(http, &pid).await?;
                    let at = manga
                        .chapters
                        .iter()
                        .find(|c| c.url == canon_ch_url)
                        .cloned()
                        .unwrap_or_else(|| ChapterRef {
                            url: canon_ch_url,
                            name: ch_name,
                            number: ch_num,
                            volume: None,
                            published_at: None,
                            language: None,
                            source_id: Some(format!("mangadex:{ch_id}")),
                        });
                    return Ok(Resolved::Chapter {
                        manga: Box::new(manga),
                        at: Box::new(at),
                    });
                }

                let ch_ref = ChapterRef::simple(canon_ch_url.clone(), ch_name.clone(), ch_num);
                let fallback = Manga::minimal(
                    canon_ch_url,
                    SiteId::known("mangadex"),
                    ch_name,
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
                reason: "unsupported MangaDex page path",
            }),
        }
    }

    async fn chapter(&self, http: &HttpClient, url: &Url) -> Result<Chapter, ScrapeError> {
        let ch_id = extract_chapter_uuid(url).ok_or_else(|| ScrapeError::NotRecognized {
            url: url.clone(),
            reason: "MangaDex chapter URL is missing a valid UUID",
        })?;
        let canon_ch_url = canonical_chapter_url(&ch_id)?;
        let at_home_url = Url::parse(&format!("{API_BASE}/at-home/server/{ch_id}"))
            .map_err(|e| ScrapeError::InvalidUrl(e.to_string()))?;
        let at_home_resp = http
            .fetch_json(&at_home_url, "mangadex", Some(MANGADEX_USER_AGENT))
            .await?;
        let images = parse_at_home_json(&at_home_resp.html, &canon_ch_url, &at_home_url)?;

        let mut name = "Chapter".to_owned();
        let mut manga_url = None;
        if let Ok(meta_url) = Url::parse(&format!("{API_BASE}/chapter/{ch_id}?includes[]=manga"))
            && let Ok(meta_resp) = http
                .fetch_json(&meta_url, "mangadex", Some(MANGADEX_USER_AGENT))
                .await
            && let Ok((parent_id, parsed_name, _)) =
                parse_chapter_meta_json(&meta_resp.html, &meta_url)
        {
            name = parsed_name;
            if let Some(pid) = parent_id {
                manga_url = canonical_manga_url(&pid).ok();
            }
        }

        Ok(Chapter {
            url: canon_ch_url,
            name,
            images,
            prev: None,
            next: None,
            manga_url,
        })
    }
}

/// Builds the official `MangaDex` REST API v5 filter schema (UUID tags + status/demographic/sort axes).
#[must_use]
pub fn mangadex_filter_schema() -> SiteFilterSchema {
    let tags = MANGADEX_DEFAULT_TAGS
        .iter()
        .map(|(id, label)| FilterTagOption {
            id: (*id).to_owned(),
            label: (*label).to_owned(),
            url: Some(format!("https://mangadex.org/tag/{id}")),
        })
        .collect();

    let extra_axes = vec![
        FilterAxis {
            key: "status[]".to_owned(),
            label: "Status".to_owned(),
            kind: FilterAxisKind::MultiSelect,
            options: vec![
                FilterOptionItem {
                    value: "ongoing".to_owned(),
                    label: "Ongoing".to_owned(),
                },
                FilterOptionItem {
                    value: "completed".to_owned(),
                    label: "Completed".to_owned(),
                },
                FilterOptionItem {
                    value: "hiatus".to_owned(),
                    label: "Hiatus".to_owned(),
                },
                FilterOptionItem {
                    value: "cancelled".to_owned(),
                    label: "Cancelled".to_owned(),
                },
            ],
        },
        FilterAxis {
            key: "publicationDemographic[]".to_owned(),
            label: "Demographic".to_owned(),
            kind: FilterAxisKind::MultiSelect,
            options: vec![
                FilterOptionItem {
                    value: "shounen".to_owned(),
                    label: "Shounen".to_owned(),
                },
                FilterOptionItem {
                    value: "seinen".to_owned(),
                    label: "Seinen".to_owned(),
                },
                FilterOptionItem {
                    value: "shoujo".to_owned(),
                    label: "Shoujo".to_owned(),
                },
                FilterOptionItem {
                    value: "josei".to_owned(),
                    label: "Josei".to_owned(),
                },
            ],
        },
        FilterAxis {
            key: "order".to_owned(),
            label: "Sort".to_owned(),
            kind: FilterAxisKind::SingleSelect,
            options: vec![
                FilterOptionItem {
                    value: "latestUploadedChapter".to_owned(),
                    label: "Latest Updates".to_owned(),
                },
                FilterOptionItem {
                    value: "followedCount".to_owned(),
                    label: "Most Popular".to_owned(),
                },
                FilterOptionItem {
                    value: "rating".to_owned(),
                    label: "Top Rated".to_owned(),
                },
                FilterOptionItem {
                    value: "createdAt".to_owned(),
                    label: "Recently Added".to_owned(),
                },
            ],
        },
    ];

    SiteFilterSchema {
        filter_endpoint: Some("https://mangadex.org/titles".to_owned()),
        include_tag_param: Some("includedTags[]".to_owned()),
        exclude_tag_param: Some("excludedTags[]".to_owned()),
        comma_joined_tags: false,
        min_chapters_param: None,
        keyword_param: Some("q".to_owned()),
        tags,
        extra_axes,
    }
}

pub(crate) fn build_mangadex_filtered_url(
    clean_q: Option<&str>,
    filters: &CatalogFilterQuery,
    page_num: u32,
) -> Result<(Url, UnhandledFilterMask), ScrapeError> {
    let mut u = Url::parse("https://mangadex.org/titles")
        .map_err(|e| ScrapeError::InvalidUrl(e.to_string()))?;
    {
        let mut qp = u.query_pairs_mut();
        if let Some(q) = clean_q {
            qp.append_pair("q", q);
        }
        for inc in &filters.include_tags {
            qp.append_pair("includedTags[]", inc);
        }
        for exc in &filters.exclude_tags {
            qp.append_pair("excludedTags[]", exc);
        }
        for (k, vals) in &filters.extra {
            for v in vals {
                if !v.trim().is_empty() {
                    qp.append_pair(k, v.trim());
                }
            }
        }
        if page_num > 1 {
            qp.append_pair("page", &page_num.to_string());
        }
    }
    let unhandled = UnhandledFilterMask {
        include_tags: false,
        exclude_tags: false,
        min_chapters: filters.min_chapters.unwrap_or(0) > 0,
        updated_within: filters.updated_within != UpdatedWithinFilter::Any,
    };
    Ok((u, unhandled))
}

const MANGADEX_DEFAULT_TAGS: &[(&str, &str)] = &[
    ("391b0423-d847-456f-aff0-8b0cfc03066b", "Action"),
    ("87cc87cd-a395-47af-b27a-93258283bbc6", "Adventure"),
    ("4d32cc48-9f00-4cca-9b5a-a839f0764984", "Comedy"),
    ("b9af3a63-f058-46de-a9a0-e0c13906197a", "Drama"),
    ("cdc58593-87dd-415e-bbc0-2ec27bf404cc", "Fantasy"),
    ("33771934-028e-4cb3-8744-691e866a923e", "Historical"),
    ("cdad7e68-1419-41dd-bdce-27753074a640", "Horror"),
    ("ace04997-f6bd-436e-b261-779182193d3d", "Isekai"),
    ("799c202e-7daa-44eb-9cf7-8a3c0441531e", "Martial Arts"),
    ("50880a9d-5440-4732-9afb-8f457127e836", "Mecha"),
    ("ee968100-4191-4968-93d3-f82d72be7e46", "Mystery"),
    ("3b60b75c-a2d7-4860-ab56-05f391bb889c", "Psychological"),
    ("423e2eae-a7a2-4a8b-ac03-a8351462d71d", "Romance"),
    ("caaa44eb-cd40-4177-b930-79d3ef2afe87", "School Life"),
    ("256c8bd9-4904-4360-bf4f-508a76d67183", "Sci-Fi"),
    ("e5301a23-ebd9-49dd-a0cb-2add944c7fe9", "Slice of Life"),
    ("69964a64-2f90-4d33-beeb-f3ed2875eb4c", "Sports"),
    ("eabc5b4c-6aff-42f3-b657-3e90cbd00b75", "Supernatural"),
    ("07251805-a27e-4d59-b488-f0bfbec15168", "Thriller"),
    ("f8f62932-27da-4fe4-8ee1-6779a8c5edba", "Tragedy"),
    ("e197df38-d0e7-43b5-9b09-2842d0c326dd", "Web Comic"),
    ("f5ba408b-0e7a-484d-8d49-4e9125ac96de", "Full Color"),
    ("3e2b8dae-350e-4ab8-a8ce-016e844b9f0d", "Long Strip"),
    ("0bc90acb-ccc1-44ca-a34a-b9f3a73259d0", "Reincarnation"),
];
