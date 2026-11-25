//! Declarative JSON CSS-selector rule packs for custom comic websites.
//!
//! Rule packs execute locally on-device in strict read-only fashion without remote code
//! execution or external API calls.

use std::collections::HashSet;
use std::sync::LazyLock;

use scraper::{Html, Selector};

static IMG_FALLBACK_SEL: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("img").expect("valid img selector"));
static CMS_READER_FALLBACK_SEL: LazyLock<Selector> = LazyLock::new(|| {
    Selector::parse(
        "div.reading-content img, .wp-manga-chapter-img img, #readerarea img, .page-chapter img",
    )
    .expect("valid static fallback selector")
});
use serde::{Deserialize, Serialize};
use url::Url;

use crate::error::ScrapeError;
use crate::extract::{
    HtmlMinimumPolicy, extract_chapter_html_adaptive, extract_structured_chapter,
};
use crate::http::HttpClient;
use crate::model::{
    CatalogEntry, CatalogPage, Chapter, ChapterRef, Manga, PageImage, PaginationInfo, SiteId,
};
use crate::parse::chapters::extract_chapter_number;
use crate::parse::dom::{
    abs_url, clean_text, effective_base_url, image_candidates, is_ignored_image_url,
};
use crate::parse::meta::{extract_cover, extract_creators, extract_tags_and_genres, extract_title};
use crate::parse::nav::{extract_manga_url_from_chapter, extract_prev_next};
use crate::recognize::UrlKind;
use crate::resolver::{CandidateOrigin, RawImageCandidate, resolve_page_images};
use crate::source::generic::GenericSource;

/// Pagination scheme declared by a custom JSON rule pack.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum RulePaginationKind {
    /// Query string pagination (`?page=2`), omitting `?page=1` on the first page.
    #[default]
    QueryPage,
    /// Offset-based query pagination (`?offset=24`).
    Offset,
    /// `WordPress` / path segment pagination (`/page/2/`).
    PathSegment,
}

/// Catalog listing and search selectors inside a [`CustomRuleSpec`].
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct RuleCatalogConfig {
    /// Relative or absolute listing path (defaults to `"/"`).
    pub endpoint: String,
    /// Optional search path (defaults to `endpoint`).
    pub search_endpoint: Option<String>,
    /// Query parameter name used when searching (defaults to `"s"`).
    pub search_param: Option<String>,
    /// Pagination format (`query_page`, `offset`, `path_segment`).
    pub pagination: RulePaginationKind,
    /// Query parameter name when `pagination` is `query_page` or `offset` (defaults to `"page"`).
    pub page_param: Option<String>,
    /// Page step multiplier when `pagination` is `offset` (defaults to `24`).
    pub page_size: Option<u32>,
    /// CSS selector matching each manga card container on the catalog page.
    pub item_selector: String,
    /// CSS selector relative to an item matching the title text (falls back to link text/title).
    pub title_selector: Option<String>,
    /// CSS selector relative to an item matching the cover `<img>`.
    pub cover_selector: Option<String>,
    /// CSS selector relative to an item matching the detail `<a>` link (defaults to `"a[href]"`).
    pub link_selector: Option<String>,
}

/// Series detail selectors inside a [`CustomRuleSpec`].
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct RuleDetailConfig {
    /// CSS selector for series title.
    pub title_selector: Option<String>,
    /// CSS selector for series cover image.
    pub cover_selector: Option<String>,
    /// CSS selector for series synopsis.
    pub description_selector: Option<String>,
    /// CSS selector for author elements.
    pub author_selector: Option<String>,
    /// CSS selector for status text.
    pub status_selector: Option<String>,
    /// CSS selector for genre/tag elements.
    pub tag_selector: Option<String>,
}

/// Chapter list selectors inside a [`CustomRuleSpec`].
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct RuleChaptersConfig {
    /// CSS selector matching each chapter row or anchor element.
    pub selector: String,
    /// CSS selector relative to chapter row for chapter label (defaults to element text).
    pub title_selector: Option<String>,
    /// CSS selector relative to chapter row for chapter link (defaults to row itself or `"a[href]"`).
    pub link_selector: Option<String>,
    /// Optional AJAX POST endpoint path (e.g. `"/wp-admin/admin-ajax.php"`).
    pub ajax_endpoint: Option<String>,
    /// Optional AJAX `action` form field name.
    pub ajax_action: Option<String>,
}

/// Page image selectors inside a [`CustomRuleSpec`].
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct RulePagesConfig {
    /// CSS selector matching reader page `<img>` elements.
    pub image_selector: String,
    /// Optional preferred image attribute name (e.g. `"data-src"` or `"data-original"`).
    pub image_attr: Option<String>,
}

/// Complete declarative JSON rule specification for parsing a custom manga website.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct CustomRuleSpec {
    /// Stable unique identifier (slug).
    pub id: String,
    /// Human-readable rule pack display name.
    pub name: String,
    /// Schema/rule revision (`>= 1`).
    pub version: u32,
    /// Associated bare hostnames matched at zero network cost.
    pub domains: Vec<String>,
    /// Substring fingerprints checked against the first 64 KiB of homepage HTML.
    pub fingerprints: Vec<String>,
    /// Optional sub-paths probed if neither domain nor homepage fingerprint matched.
    pub probe_paths: Vec<String>,
    /// Catalog & search extraction rules.
    pub catalog: RuleCatalogConfig,
    /// Series detail extraction rules.
    pub detail: RuleDetailConfig,
    /// Chapter list extraction rules.
    pub chapters: RuleChaptersConfig,
    /// Chapter page image extraction rules.
    pub pages: RulePagesConfig,
}

/// Validates a [`CustomRuleSpec`] for required identifiers, version bounds, and valid CSS selectors.
///
/// # Errors
///
/// Returns a descriptive validation message if any required field or CSS selector is invalid.
pub fn validate_rule_spec(spec: &CustomRuleSpec) -> Result<(), String> {
    if spec.id.trim().is_empty() {
        return Err("rule id must not be empty".to_owned());
    }
    if spec.name.trim().is_empty() {
        return Err("rule name must not be empty".to_owned());
    }
    if spec.version < 1 {
        return Err("rule version must be >= 1".to_owned());
    }
    if spec.catalog.item_selector.trim().is_empty() {
        return Err("catalog.itemSelector must not be empty".to_owned());
    }
    if spec.chapters.selector.trim().is_empty() {
        return Err("chapters.selector must not be empty".to_owned());
    }
    if spec.pages.image_selector.trim().is_empty() {
        return Err("pages.imageSelector must not be empty".to_owned());
    }

    let selectors: [(&str, Option<&str>); 12] = [
        (
            "catalog.itemSelector",
            Some(spec.catalog.item_selector.as_str()),
        ),
        (
            "catalog.titleSelector",
            spec.catalog.title_selector.as_deref(),
        ),
        (
            "catalog.coverSelector",
            spec.catalog.cover_selector.as_deref(),
        ),
        (
            "catalog.linkSelector",
            spec.catalog.link_selector.as_deref(),
        ),
        (
            "detail.titleSelector",
            spec.detail.title_selector.as_deref(),
        ),
        (
            "detail.coverSelector",
            spec.detail.cover_selector.as_deref(),
        ),
        (
            "detail.descriptionSelector",
            spec.detail.description_selector.as_deref(),
        ),
        (
            "detail.authorSelector",
            spec.detail.author_selector.as_deref(),
        ),
        (
            "detail.statusSelector",
            spec.detail.status_selector.as_deref(),
        ),
        ("detail.tagSelector", spec.detail.tag_selector.as_deref()),
        ("chapters.selector", Some(spec.chapters.selector.as_str())),
        (
            "pages.imageSelector",
            Some(spec.pages.image_selector.as_str()),
        ),
    ];
    for (field, maybe_sel) in selectors {
        if let Some(raw) = maybe_sel.map(str::trim).filter(|s| !s.is_empty())
            && let Err(err) = Selector::parse(raw)
        {
            return Err(format!("invalid CSS selector in {field}: {err:?}"));
        }
    }
    if let Some(s) = spec
        .chapters
        .title_selector
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        && let Err(err) = Selector::parse(s)
    {
        return Err(format!(
            "invalid CSS selector in chapters.titleSelector: {err:?}"
        ));
    }
    if let Some(s) = spec
        .chapters
        .link_selector
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        && let Err(err) = Selector::parse(s)
    {
        return Err(format!(
            "invalid CSS selector in chapters.linkSelector: {err:?}"
        ));
    }

    Ok(())
}

/// Checks zero-cost domain matches (`0` requests) and returns the matching rule if found.
#[must_use]
pub fn match_rule_by_domain<'a>(
    host_lower: &str,
    rules: &'a [CustomRuleSpec],
) -> Option<&'a CustomRuleSpec> {
    let bare = host_lower.trim_start_matches("www.");
    rules.iter().find(|r| {
        r.domains.iter().any(|d| {
            let d_clean = d.trim().trim_start_matches("www.").to_ascii_lowercase();
            !d_clean.is_empty() && (bare == d_clean || bare.ends_with(&format!(".{d_clean}")))
        })
    })
}

/// Checks homepage HTML fingerprint markers on the first 64 KiB slice (at most 1 request).
#[must_use]
pub fn match_rule_by_fingerprint<'a>(
    html: &str,
    rules: &'a [CustomRuleSpec],
) -> Option<&'a CustomRuleSpec> {
    let prefix = slice_utf8_prefix(html, 64 * 1024).to_ascii_lowercase();
    rules.iter().find(|r| {
        !r.fingerprints.is_empty()
            && r.fingerprints
                .iter()
                .any(|fp| !fp.trim().is_empty() && prefix.contains(&fp.trim().to_ascii_lowercase()))
    })
}

/// Builds a target listing or search URL for `spec`, omitting `?page=1` on page 1 so sites that
/// reject explicit page-1 parameters still succeed cleanly.
///
/// # Errors
///
/// Returns [`ScrapeError::InvalidUrl`] if URL joining fails.
pub fn build_rule_catalog_url(
    base_url: &Url,
    spec: &CustomRuleSpec,
    query: Option<&str>,
    page: u32,
) -> Result<Url, ScrapeError> {
    let clean_query = query.map(str::trim).filter(|q| !q.is_empty());
    let endpoint_raw = match clean_query {
        Some(_) => spec
            .catalog
            .search_endpoint
            .as_deref()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or(spec.catalog.endpoint.as_str()),
        None => spec.catalog.endpoint.as_str(),
    };
    let ep = if endpoint_raw.trim().is_empty() {
        "/"
    } else {
        endpoint_raw.trim()
    };

    let mut target = if ep.starts_with("http://") || ep.starts_with("https://") {
        Url::parse(ep).map_err(|e| ScrapeError::InvalidUrl(e.to_string()))?
    } else {
        base_url
            .join(ep)
            .map_err(|e| ScrapeError::InvalidUrl(e.to_string()))?
    };

    if page > 1 && spec.catalog.pagination == RulePaginationKind::PathSegment {
        let p = format!("{}/page/{page}/", target.path().trim_end_matches('/'));
        target.set_path(&p);
    }

    if clean_query.is_some()
        || (page > 1 && spec.catalog.pagination != RulePaginationKind::PathSegment)
    {
        let search_key = spec
            .catalog
            .search_param
            .as_deref()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or("s");
        let page_key = spec
            .catalog
            .page_param
            .as_deref()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or("page");

        let mut pairs: Vec<(String, String)> = target
            .query_pairs()
            .map(|(k, v)| (k.into_owned(), v.into_owned()))
            .collect();
        if let Some(q) = clean_query {
            pairs.retain(|(k, _)| k != search_key);
            pairs.push((search_key.to_owned(), q.to_owned()));
        }
        if page > 1 {
            match spec.catalog.pagination {
                RulePaginationKind::QueryPage => {
                    pairs.retain(|(k, _)| k != page_key);
                    pairs.push((page_key.to_owned(), page.to_string()));
                }
                RulePaginationKind::Offset => {
                    let step = spec.catalog.page_size.unwrap_or(24).max(1);
                    let offset = (page - 1).saturating_mul(step);
                    pairs.retain(|(k, _)| k != page_key);
                    pairs.push((page_key.to_owned(), offset.to_string()));
                }
                RulePaginationKind::PathSegment => {}
            }
        }
        target.query_pairs_mut().clear().extend_pairs(pairs);
    }

    Ok(target)
}

/// Fetches and parses a catalog or search page using `spec`.
///
/// # Errors
///
/// Returns [`ScrapeError`] on transport, invalid selector, or URL construction errors.
pub async fn execute_rule_catalog(
    http: &HttpClient,
    base_url: &Url,
    spec: &CustomRuleSpec,
    query: Option<&str>,
    page: u32,
) -> Result<CatalogPage, ScrapeError> {
    let target_url = build_rule_catalog_url(base_url, spec, query, page.max(1))?;
    let fetched = http.fetch_html(&target_url).await?;
    parse_rule_catalog_html(
        &fetched.html,
        &fetched.final_url,
        base_url,
        spec,
        query,
        page.max(1),
    )
}

/// Parses fetched HTML into a [`CatalogPage`] according to `spec`.
///
/// # Errors
///
/// Returns [`ScrapeError::NotRecognized`] if the configured CSS selectors fail to parse.
pub fn parse_rule_catalog_html(
    html: &str,
    page_url: &Url,
    base_root: &Url,
    spec: &CustomRuleSpec,
    query: Option<&str>,
    page: u32,
) -> Result<CatalogPage, ScrapeError> {
    let doc = Html::parse_document(html);
    let base = effective_base_url(&doc, page_url);
    let entries = extract_rule_catalog_entries(&doc, &base, page_url, spec)?;

    let search_param = spec
        .catalog
        .search_param
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or("s");
    let search_ep = spec
        .catalog
        .search_endpoint
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or(if spec.catalog.endpoint.trim().is_empty() {
            "/"
        } else {
            spec.catalog.endpoint.as_str()
        });
    let search_template = base_root.join(search_ep).ok().map(|u| {
        let sep = if u.query().is_some() { "&" } else { "?" };
        format!("{u}{sep}{search_param}={{query}}")
    });

    let next_url = if entries.is_empty() {
        None
    } else {
        build_rule_catalog_url(base_root, spec, query, page + 1).ok()
    };
    let prev_url = if page > 1 {
        build_rule_catalog_url(base_root, spec, query, page - 1).ok()
    } else {
        None
    };

    let clean_q = query
        .map(str::trim)
        .filter(|q| !q.is_empty())
        .map(str::to_owned);
    let url_kind = if clean_q.is_some() {
        UrlKind::SearchPage
    } else {
        UrlKind::CatalogPage
    };
    Ok(CatalogPage {
        url: page_url.clone(),
        site: SiteId::Known(format!("rule:{}", spec.id)),
        site_name: spec.name.clone(),
        title: if let Some(ref q) = clean_q {
            format!("{} — {q}", spec.name)
        } else {
            spec.name.clone()
        },
        url_kind,
        query: clean_q,
        search_url_template: search_template,
        entries,
        categories: Vec::new(),
        pagination: PaginationInfo {
            current_page: page,
            total_pages: None,
            prev_url,
            next_url,
        },
    })
}

fn extract_rule_catalog_entries(
    doc: &Html,
    base: &Url,
    page_url: &Url,
    spec: &CustomRuleSpec,
) -> Result<Vec<CatalogEntry>, ScrapeError> {
    let item_sel = parse_sel(&spec.catalog.item_selector, page_url)?;
    let link_sel = parse_opt_sel(spec.catalog.link_selector.as_deref(), "a[href]", page_url)?;
    let title_sel = spec
        .catalog
        .title_selector
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .map(|s| parse_sel(s, page_url))
        .transpose()?;
    let cover_sel = spec
        .catalog
        .cover_selector
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .map(|s| parse_sel(s, page_url))
        .transpose()?;

    let mut entries = Vec::new();
    let mut seen = HashSet::new();
    for item in doc.select(&item_sel) {
        let Some(anchor) = item.select(&link_sel).next() else {
            continue;
        };
        let Some(href) = anchor.value().attr("href") else {
            continue;
        };
        let Some(url) = abs_url(base, href) else {
            continue;
        };
        let title = title_sel
            .as_ref()
            .and_then(|sel| item.select(sel).next())
            .map(|el| clean_text(&el.text().collect::<String>()))
            .filter(|s| !s.is_empty())
            .or_else(|| {
                anchor
                    .value()
                    .attr("title")
                    .map(clean_text)
                    .filter(|s| !s.is_empty())
            })
            .or_else(|| {
                let text = clean_text(&anchor.text().collect::<String>());
                if text.is_empty() { None } else { Some(text) }
            });
        let Some(title) = title else {
            continue;
        };

        let cover = cover_sel
            .as_ref()
            .and_then(|sel| item.select(sel).next())
            .or_else(|| item.select(&IMG_FALLBACK_SEL).next())
            .and_then(|img| image_candidates(img, base).into_iter().next());

        let key = url.as_str().trim_end_matches('/').to_owned();
        if !seen.insert(key) {
            continue;
        }
        entries.push(CatalogEntry {
            url,
            title,
            alt_titles: Vec::new(),
            cover,
            latest_chapter: None,
            status: None,
            genres: Vec::new(),
            source_id: None,
        });
    }
    Ok(entries)
}

/// Fetches and parses a manga detail page using `spec` (supporting optional AJAX chapter endpoints).
///
/// # Errors
///
/// Returns [`ScrapeError`] on transport or parse failure.
pub async fn execute_rule_manga(
    http: &HttpClient,
    page_url: &Url,
    spec: &CustomRuleSpec,
) -> Result<Manga, ScrapeError> {
    let fetched = http.fetch_html(page_url).await?;
    let mut manga = parse_rule_manga_html(&fetched.html, &fetched.final_url, spec)?;

    if manga.chapters.is_empty()
        && let Some(ajax_ep) = spec
            .chapters
            .ajax_endpoint
            .as_deref()
            .filter(|s| !s.trim().is_empty())
        && let Ok(ajax_url) = fetched.final_url.join(ajax_ep.trim())
    {
        let action = spec
            .chapters
            .ajax_action
            .as_deref()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or("manga_get_chapters");
        let slug_id = fetched
            .final_url
            .path_segments()
            .and_then(|mut s| s.rfind(|seg| !seg.is_empty()))
            .unwrap_or("");
        if let Ok(ajax_page) = http
            .post_form(
                &ajax_url,
                &[("action", action), ("manga", slug_id)],
                Some(&fetched.final_url),
            )
            .await
        {
            let doc = Html::parse_document(&ajax_page.html);
            let base = effective_base_url(&doc, &fetched.final_url);
            if let Ok(chaps) = extract_rule_chapters(&doc, &base, spec)
                && !chaps.is_empty()
            {
                manga.chapters = chaps;
            }
        }
    }

    if manga.chapters.is_empty() {
        return GenericSource::parse_manga_html(&fetched.html, &fetched.final_url);
    }

    Ok(manga)
}

/// Parses a manga detail HTML document using `spec`.
///
/// # Errors
///
/// Returns [`ScrapeError::NotRecognized`] if a configured selector is invalid.
pub fn parse_rule_manga_html(
    html: &str,
    page_url: &Url,
    spec: &CustomRuleSpec,
) -> Result<Manga, ScrapeError> {
    let doc = Html::parse_document(html);
    let base = effective_base_url(&doc, page_url);

    let title = spec
        .detail
        .title_selector
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .and_then(|s| Selector::parse(s).ok())
        .and_then(|sel| doc.select(&sel).next())
        .map(|el| clean_text(&el.text().collect::<String>()))
        .filter(|s| !s.is_empty())
        .or_else(|| extract_title(&doc))
        .unwrap_or_else(|| page_url.host_str().unwrap_or("Manga").to_owned());

    let cover = spec
        .detail
        .cover_selector
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .and_then(|s| Selector::parse(s).ok())
        .and_then(|sel| doc.select(&sel).next())
        .and_then(|img| image_candidates(img, &base).into_iter().next())
        .or_else(|| extract_cover(&doc, &base));

    let description = spec
        .detail
        .description_selector
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .and_then(|s| Selector::parse(s).ok())
        .and_then(|sel| doc.select(&sel).next())
        .map(|el| clean_text(&el.text().collect::<String>()))
        .filter(|s| !s.is_empty());

    let (genres, mut tags) = extract_tags_and_genres(&doc);
    if let Some(tag_sel) = spec
        .detail
        .tag_selector
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .and_then(|s| Selector::parse(s).ok())
    {
        for el in doc.select(&tag_sel) {
            let label = clean_text(&el.text().collect::<String>());
            if !label.is_empty()
                && !genres.iter().any(|g| g.eq_ignore_ascii_case(&label))
                && !tags.iter().any(|t| t.eq_ignore_ascii_case(&label))
            {
                tags.push(label);
            }
        }
    }

    let (mut authors, artists) = extract_creators(&doc);
    if authors.is_empty()
        && let Some(auth_sel) = spec
            .detail
            .author_selector
            .as_deref()
            .filter(|s| !s.trim().is_empty())
            .and_then(|s| Selector::parse(s).ok())
    {
        for el in doc.select(&auth_sel) {
            let a = clean_text(&el.text().collect::<String>());
            if !a.is_empty() && !authors.contains(&a) {
                authors.push(a);
            }
        }
    }

    let status = spec
        .detail
        .status_selector
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .and_then(|s| Selector::parse(s).ok())
        .and_then(|sel| doc.select(&sel).next())
        .map(|el| clean_text(&el.text().collect::<String>()))
        .filter(|s| !s.is_empty());

    let chapters = extract_rule_chapters(&doc, &base, spec)?;

    let mut manga = Manga::minimal(
        page_url.clone(),
        SiteId::Known(format!("rule:{}", spec.id)),
        title,
        cover,
        chapters,
    );
    manga.description = description;
    manga.genres = genres;
    manga.tags = tags;
    manga.authors = authors;
    manga.artists = artists;
    manga.status = status;
    Ok(manga)
}

/// Fetches and parses a chapter page using `spec` with a 4-tier extraction fallback chain.
///
/// # Errors
///
/// Returns [`ScrapeError`] if transport fails or no images can be extracted.
pub async fn execute_rule_chapter(
    http: &HttpClient,
    page_url: &Url,
    spec: &CustomRuleSpec,
) -> Result<Chapter, ScrapeError> {
    let fetched = http.fetch_html(page_url).await?;
    parse_rule_chapter_html(&fetched.html, &fetched.final_url, spec)
}

/// Parses chapter HTML using the 4-tier extraction strategy:
/// 1) Declared `pages.imageSelector` (+ optional `imageAttr`)
/// 2) Standard Madara/Themesia reader selectors
/// 3) Embedded structured JSON / Base64 page arrays
/// 4) Adaptive DOM content `<img>` scan
///
/// # Errors
///
/// Returns [`ScrapeError::NotRecognized`] if no chapter images are discovered.
pub fn parse_rule_chapter_html(
    html: &str,
    page_url: &Url,
    spec: &CustomRuleSpec,
) -> Result<Chapter, ScrapeError> {
    let doc = Html::parse_document(html);
    let base = effective_base_url(&doc, page_url);

    // Tier 1: declared template selector
    let img_sel = parse_sel(&spec.pages.image_selector, page_url)?;
    let pages = collect_imgs_with_selector(
        &doc,
        &img_sel,
        spec.pages.image_attr.as_deref(),
        page_url,
        &base,
    );
    if !pages.is_empty() {
        return Ok(build_chapter_from_pages(&doc, page_url, &base, pages));
    }

    // Tier 2: standard CMS reader containers
    let tier2 = collect_imgs_with_selector(&doc, &CMS_READER_FALLBACK_SEL, None, page_url, &base);
    if !tier2.is_empty() {
        return Ok(build_chapter_from_pages(&doc, page_url, &base, tier2));
    }

    // Tier 3: structured JSON arrays in <script>
    if let Some(structured) = extract_structured_chapter(&doc, html, page_url, &base) {
        return Ok(structured.chapter);
    }

    // Tier 4: adaptive content scan
    if let Some(ch) = extract_chapter_html_adaptive(
        &doc,
        page_url,
        &base,
        HtmlMinimumPolicy::ConfidentChapterOneOrMore,
    )
    .chapter
    {
        return Ok(ch);
    }

    Err(ScrapeError::NotRecognized {
        url: page_url.clone(),
        reason: "custom rule found no chapter images",
    })
}

fn extract_rule_chapters(
    doc: &Html,
    base: &Url,
    spec: &CustomRuleSpec,
) -> Result<Vec<ChapterRef>, ScrapeError> {
    let row_sel = parse_sel(&spec.chapters.selector, base)?;
    let title_sel = spec
        .chapters
        .title_selector
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .map(|s| parse_sel(s, base))
        .transpose()?;
    let link_sel = parse_opt_sel(spec.chapters.link_selector.as_deref(), "a[href]", base)?;

    let mut items = Vec::new();
    let mut seen = HashSet::new();
    for row in doc.select(&row_sel) {
        let row_anchor = if row.value().name() == "a" && row.value().attr("href").is_some() {
            Some(row)
        } else {
            row.select(&link_sel).next()
        };
        let Some(anchor) = row_anchor else {
            continue;
        };
        let Some(href) = anchor.value().attr("href") else {
            continue;
        };
        let Some(url) = abs_url(base, href) else {
            continue;
        };
        if !seen.insert(url.as_str().trim_end_matches('/').to_owned()) {
            continue;
        }
        let name = title_sel
            .as_ref()
            .and_then(|sel| row.select(sel).next())
            .map(|el| clean_text(&el.text().collect::<String>()))
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| clean_text(&anchor.text().collect::<String>()));
        let num = extract_chapter_number(&name).or_else(|| extract_chapter_number(url.path()));
        let final_name = if name.is_empty() {
            url.path().to_owned()
        } else {
            name
        };
        items.push(ChapterRef::simple(url, final_name, num));
    }

    // Reverse DOM order when chapters appear newest-first on the page so oldest comes first.
    if items.len() >= 2
        && let (Some(first), Some(last)) = (
            items.first().and_then(|c| c.number),
            items.last().and_then(|c| c.number),
        )
        && first > last
    {
        items.reverse();
    }
    Ok(items)
}

fn collect_imgs_with_selector(
    doc: &Html,
    sel: &Selector,
    preferred_attr: Option<&str>,
    page_url: &Url,
    base: &Url,
) -> Vec<PageImage> {
    let mut cands = Vec::new();
    let pref = preferred_attr.map(str::trim).filter(|s| !s.is_empty());
    for img in doc.select(sel) {
        let mut urls = Vec::new();
        if let Some(attr) = pref
            && let Some(raw) = img.value().attr(attr)
        {
            urls.push(raw.to_owned());
        }
        for u in image_candidates(img, base) {
            if !is_ignored_image_url(&u) {
                let s = u.to_string();
                if !urls.contains(&s) {
                    urls.push(s);
                }
            }
        }
        if let Some((primary, rest)) = urls.split_first() {
            cands.push(RawImageCandidate {
                raw_url: primary.clone(),
                fallbacks: rest.to_vec(),
                referer: page_url.clone(),
                origin: CandidateOrigin::HtmlImg,
            });
        }
    }
    resolve_page_images(cands, base).pages
}

fn build_chapter_from_pages(
    doc: &Html,
    page_url: &Url,
    base: &Url,
    pages: Vec<PageImage>,
) -> Chapter {
    let nav = extract_prev_next(doc, page_url, base);
    let manga_url = extract_manga_url_from_chapter(doc, page_url, base);
    let name = extract_title(doc).unwrap_or_else(|| "Chapter".to_owned());
    Chapter {
        url: page_url.clone(),
        name,
        images: pages,
        prev: nav.prev,
        next: nav.next,
        manga_url,
    }
}

fn parse_sel(raw: &str, url: &Url) -> Result<Selector, ScrapeError> {
    Selector::parse(raw.trim()).map_err(|_| ScrapeError::NotRecognized {
        url: url.clone(),
        reason: "invalid CSS selector in custom rule",
    })
}

fn parse_opt_sel(raw: Option<&str>, default: &str, url: &Url) -> Result<Selector, ScrapeError> {
    let chosen = raw
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(default);
    parse_sel(chosen, url)
}

fn slice_utf8_prefix(s: &str, max_bytes: usize) -> &str {
    if s.len() <= max_bytes {
        return s;
    }
    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}
