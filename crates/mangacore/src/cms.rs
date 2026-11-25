//! Structure-first website CMS classification, catalog/search execution, and mirror fallback.
//!
//! Classifies websites primarily from DOM structure and API shapes rather than hardcoding
//! third-party domain lists, and preserves genuine upstream errors instead of masking them.

use std::collections::HashSet;
use std::fmt;
use std::sync::LazyLock;

use regex::Regex;
use scraper::{Html, Selector};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use url::Url;

use crate::error::ScrapeError;
use crate::http::HttpClient;
use crate::model::{CatalogEntry, CatalogPage, ChapterRef, PaginationInfo, SiteId};
use crate::parse::chapters::extract_chapters;
use crate::parse::dom::{
    abs_url, clean_text, effective_base_url, image_candidates, is_ignored_image_url,
};
use crate::parse::gallery::looks_like_gallery_page;
use crate::parse::meta::{
    detect_adult_signals, extract_favicon_candidates, extract_tags_and_genres, extract_title,
};
use crate::recognize::UrlKind;
use crate::rule_pack::{
    CustomRuleSpec, execute_rule_catalog, match_rule_by_domain, match_rule_by_fingerprint,
};
use crate::source::generic::{GenericSource, discover_search_template};
use crate::source::mangadex::is_mangadex_host;
use crate::source::moetruyen::is_moetruyen_host;
use crate::source::truyenqq::is_truyenqq_host;

static MADARA_ITEM_SEL: LazyLock<Selector> = LazyLock::new(|| {
    Selector::parse(".page-item-detail, .c-tabs-item__content, .manga-item")
        .expect("static selector is valid")
});
static THEMESIA_ITEM_SEL: LazyLock<Selector> = LazyLock::new(|| {
    Selector::parse(".listupd .bsx, .utao .uta, .bs .bsx").expect("static selector is valid")
});
static WPCOMICS_ITEM_SEL: LazyLock<Selector> = LazyLock::new(|| {
    Selector::parse(".ModuleContent .items .item, .items .item").expect("static selector is valid")
});
static GALLERY_CARD_SEL: LazyLock<Selector> = LazyLock::new(|| {
    Selector::parse(".gallery, .gallery-item, .thumb-item, .doujin-item")
        .expect("static selector is valid")
});
static LINK_SEL: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("a[href]").expect("static selector is valid"));
static IMG_SEL: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("img").expect("static selector is valid"));
static TITLE_HINT_SEL: LazyLock<Selector> = LazyLock::new(|| {
    Selector::parse("h3 a, h4 a, .post-title a, .tt, .title, figcaption")
        .expect("static selector is valid")
});
static LATEST_CH_SEL: LazyLock<Selector> = LazyLock::new(|| {
    Selector::parse(".chapter-item a, .epxs, .chapter a, .latest-chapter")
        .expect("static selector is valid")
});
static WP_ID_ATTR_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"data-id=["'](\d+)["']"#).expect("static regex is valid"));
static MANGADEX_CANONICAL_BASE: LazyLock<Url> =
    LazyLock::new(|| Url::parse("https://mangadex.org").expect("valid static url"));

/// Recognized website architecture / CMS family.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum CmsKind {
    /// Matched one of the compiled built-in dedicated adapters.
    BuiltInAdapter,
    /// `WordPress` Madara (`wp-manga`) theme structure.
    Madara,
    /// `WordPress` `MangaThemesia` / `MangaStream` / `MangaReader` theme structure.
    MangaThemesia,
    /// `WPComics`-style item grid structure.
    WpComics,
    /// Single-work gallery / doujinshi card grid structure.
    FlatGallery,
    /// `MangaDex` REST API v5 JSON format.
    MangaDexApi,
    /// Comick-compatible REST API JSON format.
    ComickApi,
    /// `FoolSlide2` / Guya-compatible JSON endpoint structure.
    FoolSlide,
    /// Matched a declarative [`CustomRuleSpec`] JSON rule pack.
    CustomRule,
    /// Generic HTML catalog & heuristic extraction engine.
    #[default]
    UniversalHeuristic,
    /// Site requires interactive browser verification or JS rendering.
    BrowserOnly,
}

impl CmsKind {
    /// Stable lowercase database identifier string.
    #[must_use]
    pub const fn as_storage_str(self) -> &'static str {
        match self {
            Self::BuiltInAdapter => "builtin_adapter",
            Self::Madara => "madara",
            Self::MangaThemesia => "manga_themesia",
            Self::WpComics => "wp_comics",
            Self::FlatGallery => "flat_gallery",
            Self::MangaDexApi => "mangadex_api",
            Self::ComickApi => "comick_api",
            Self::FoolSlide => "fool_slide",
            Self::CustomRule => "custom_rule",
            Self::UniversalHeuristic => "universal_heuristic",
            Self::BrowserOnly => "browser_only",
        }
    }

    /// Parses a stored database identifier string back into [`CmsKind`].
    #[must_use]
    pub fn from_storage_str(raw: &str) -> Self {
        match raw.trim().to_ascii_lowercase().as_str() {
            "builtin_adapter" => Self::BuiltInAdapter,
            "madara" => Self::Madara,
            "manga_themesia" => Self::MangaThemesia,
            "wp_comics" => Self::WpComics,
            "flat_gallery" => Self::FlatGallery,
            "mangadex_api" => Self::MangaDexApi,
            "comick_api" => Self::ComickApi,
            "fool_slide" => Self::FoolSlide,
            "custom_rule" => Self::CustomRule,
            "browser_only" => Self::BrowserOnly,
            _ => Self::UniversalHeuristic,
        }
    }
}

impl fmt::Display for CmsKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_storage_str())
    }
}

/// Reachability status reported during site structure onboarding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum SiteReachability {
    /// Site responded normally to HTTP inspection.
    Reachable,
    /// Site responded with an interactive verification / anti-bot challenge.
    ChallengeProtected,
    /// Network or HTTP failure prevented reaching the homepage.
    Unreachable,
}

/// Complete result of onboarding and classifying a manga website URL.
#[derive(Debug, Clone)]
pub struct SiteDetectionOutcome {
    /// Normalized root URL (preserving subpath if any).
    pub base_url: Url,
    /// Suggested display name (site title or bare domain).
    pub display_name: String,
    /// Classified CMS or engine kind.
    pub kind: CmsKind,
    /// Built-in adapter ID or custom rule ID when applicable.
    pub parser_ref: Option<String>,
    /// Discovered `{query}` search URL template if available.
    pub search_template: Option<String>,
    /// Ordered local/HTML favicon URL candidates (no external third-party favicon APIs).
    pub icon_candidates: Vec<Url>,
    /// `true` when adult / 18+ signals were detected from metadata, tags, or structure.
    pub adult_detected: bool,
    /// Reachability state during onboarding.
    pub reachability: SiteReachability,
}

/// Normalizes a website URL down to its root origin (`https://host`) for source deduplication.
///
/// # Errors
///
/// Returns [`ScrapeError::InvalidUrl`] or [`ScrapeError::DisallowedHost`] if invalid.
pub fn normalize_source_root(raw: &str) -> Result<Url, ScrapeError> {
    let parsed = crate::http::normalize_url(raw)?;
    let host = parsed
        .host_str()
        .ok_or_else(|| ScrapeError::InvalidUrl("missing host".to_owned()))?;
    let port_part = parsed.port().map_or_else(String::new, |p| format!(":{p}"));
    let root_str = format!("{}://{host}{port_part}", parsed.scheme());
    Url::parse(&root_str).map_err(|e| ScrapeError::InvalidUrl(e.to_string()))
}

/// Matches a built-in compiled adapter ID from a normalized hostname at zero network cost.
#[must_use]
pub fn match_builtin_adapter_id(host_lower: &str) -> Option<&'static str> {
    if is_mangadex_host(host_lower) {
        Some("mangadex")
    } else if is_truyenqq_host(host_lower) {
        Some("truyenqq")
    } else if is_moetruyen_host(host_lower) {
        Some("moetruyen")
    } else {
        None
    }
}

/// Executes the 3-step structure-first onboarding pipeline on `raw_url`:
/// 1) Built-in adapters & zero-cost custom domain rules (`0` requests)
/// 2) Single homepage HTML fetch -> structure-first CMS signature inspection & favicon harvest
/// 3) Custom rule HTML fingerprints / probe paths -> Universal Heuristic -> Browser fallback
///
/// # Errors
///
/// Returns [`ScrapeError::InvalidUrl`] or [`ScrapeError::DisallowedHost`] if the URL is invalid.
pub async fn detect_site_structure(
    http: &HttpClient,
    raw_url: &str,
    custom_rules: &[CustomRuleSpec],
) -> Result<SiteDetectionOutcome, ScrapeError> {
    let base_url = normalize_source_root(raw_url)?;
    let host_lower = base_url
        .host_str()
        .unwrap_or("unknown")
        .to_ascii_lowercase();
    let bare_host = host_lower.trim_start_matches("www.").to_owned();
    let default_icons = default_origin_favicons(&base_url);

    // Step 1a: Built-in adapter domain lookup (0 requests).
    if let Some(adapter_id) = match_builtin_adapter_id(&host_lower) {
        return Ok(build_builtin_outcome(
            base_url,
            adapter_id,
            &bare_host,
            default_icons,
        ));
    }

    // Step 1b: Custom rule domain lookup (0 requests).
    if let Some(rule) = match_rule_by_domain(&host_lower, custom_rules) {
        let search_template = build_rule_search_hint(&base_url, rule);
        return Ok(SiteDetectionOutcome {
            base_url,
            display_name: rule.name.clone(),
            kind: CmsKind::CustomRule,
            parser_ref: Some(rule.id.clone()),
            search_template,
            icon_candidates: default_icons,
            adult_detected: false,
            reachability: SiteReachability::Reachable,
        });
    }

    // Step 2: Single homepage HTML fetch + structure-first CMS detection.
    match http.fetch_html(&base_url).await {
        Ok(page) => {
            Ok(classify_fetched_site_html(http, &base_url, &bare_host, &page, custom_rules).await)
        }
        Err(ScrapeError::Blocked { .. }) => Ok(SiteDetectionOutcome {
            base_url,
            display_name: bare_host,
            kind: CmsKind::BrowserOnly,
            parser_ref: None,
            search_template: None,
            icon_candidates: default_icons,
            adult_detected: false,
            reachability: SiteReachability::ChallengeProtected,
        }),
        Err(_) => Ok(probe_unreachable_html_site(http, base_url, bare_host, default_icons).await),
    }
}

fn build_builtin_outcome(
    base_url: Url,
    adapter_id: &str,
    bare_host: &str,
    icon_candidates: Vec<Url>,
) -> SiteDetectionOutcome {
    let search_template = match adapter_id {
        "mangadex" => Some("https://mangadex.org/search?q={query}".to_owned()),
        "truyenqq" => Some(format!("{base_url}/tim-kiem.html?q={{query}}")),
        "moetruyen" => Some(format!("{base_url}/manga?q={{query}}")),
        _ => None,
    };
    let display_name = match adapter_id {
        "mangadex" => "MangaDex".to_owned(),
        "truyenqq" => "TruyenQQ".to_owned(),
        "moetruyen" => "MoeTruyen".to_owned(),
        _ => bare_host.to_owned(),
    };
    SiteDetectionOutcome {
        base_url,
        display_name,
        kind: CmsKind::BuiltInAdapter,
        parser_ref: Some(adapter_id.to_owned()),
        search_template,
        icon_candidates,
        adult_detected: false,
        reachability: SiteReachability::Reachable,
    }
}

async fn probe_unreachable_html_site(
    http: &HttpClient,
    base_url: Url,
    bare_host: String,
    icon_candidates: Vec<Url>,
) -> SiteDetectionOutcome {
    if let Some((api_kind, tmpl)) = probe_known_api_endpoints(http, &base_url).await {
        return SiteDetectionOutcome {
            base_url,
            display_name: bare_host,
            kind: api_kind,
            parser_ref: None,
            search_template: tmpl,
            icon_candidates,
            adult_detected: false,
            reachability: SiteReachability::Reachable,
        };
    }
    SiteDetectionOutcome {
        base_url,
        display_name: bare_host,
        kind: CmsKind::BrowserOnly,
        parser_ref: None,
        search_template: None,
        icon_candidates,
        adult_detected: false,
        reachability: SiteReachability::Unreachable,
    }
}

struct SiteHtmlSummary {
    root_final: Url,
    icon_candidates: Vec<Url>,
    adult_detected: bool,
    display_name: String,
    discovered_search: Option<String>,
    detected_cms: Option<CmsKind>,
}

fn summarize_fetched_site_html(
    base_url: &Url,
    bare_host: &str,
    fetched: &crate::http::FetchedPage,
) -> SiteHtmlSummary {
    let doc = Html::parse_document(&fetched.html);
    let base = effective_base_url(&doc, &fetched.final_url);
    let root_final =
        normalize_source_root(fetched.final_url.as_str()).unwrap_or_else(|_| base_url.clone());
    let icon_candidates = extract_favicon_candidates(&doc, &base);
    let (genres, tags) = extract_tags_and_genres(&doc);
    let adult_detected = detect_adult_signals(&fetched.final_url, &fetched.html, &genres, &tags);
    let display_name = extract_title(&doc)
        .filter(|s| !s.is_empty() && s.len() <= 48)
        .unwrap_or_else(|| {
            root_final
                .host_str()
                .unwrap_or(bare_host)
                .trim_start_matches("www.")
                .to_owned()
        });
    let discovered_search = discover_search_template(&doc, &root_final);
    let detected_cms = detect_cms_from_html(&doc, &fetched.html);
    SiteHtmlSummary {
        root_final,
        icon_candidates,
        adult_detected,
        display_name,
        discovered_search,
        detected_cms,
    }
}

async fn classify_fetched_site_html(
    http: &HttpClient,
    base_url: &Url,
    bare_host: &str,
    fetched: &crate::http::FetchedPage,
    custom_rules: &[CustomRuleSpec],
) -> SiteDetectionOutcome {
    let SiteHtmlSummary {
        root_final,
        icon_candidates,
        adult_detected,
        display_name,
        discovered_search,
        detected_cms,
    } = summarize_fetched_site_html(base_url, bare_host, fetched);

    if let Some(cms_kind) = detected_cms {
        let search_template =
            discovered_search.or_else(|| default_cms_search_template(&root_final, cms_kind));
        return SiteDetectionOutcome {
            base_url: root_final,
            display_name,
            kind: cms_kind,
            parser_ref: None,
            search_template,
            icon_candidates,
            adult_detected,
            reachability: SiteReachability::Reachable,
        };
    }

    if let Some(rule) = match_rule_by_fingerprint(&fetched.html, custom_rules) {
        return SiteDetectionOutcome {
            base_url: root_final.clone(),
            display_name,
            kind: CmsKind::CustomRule,
            parser_ref: Some(rule.id.clone()),
            search_template: build_rule_search_hint(&root_final, rule).or(discovered_search),
            icon_candidates,
            adult_detected,
            reachability: SiteReachability::Reachable,
        };
    }

    if let Some((api_kind, tmpl)) = probe_known_api_endpoints(http, &root_final).await {
        return SiteDetectionOutcome {
            base_url: root_final,
            display_name,
            kind: api_kind,
            parser_ref: None,
            search_template: tmpl,
            icon_candidates,
            adult_detected,
            reachability: SiteReachability::Reachable,
        };
    }

    if let Some(rule) = probe_rule_endpoints(http, &root_final, custom_rules).await {
        return SiteDetectionOutcome {
            base_url: root_final.clone(),
            display_name,
            kind: CmsKind::CustomRule,
            parser_ref: Some(rule.id.clone()),
            search_template: build_rule_search_hint(&root_final, rule).or(discovered_search),
            icon_candidates,
            adult_detected,
            reachability: SiteReachability::Reachable,
        };
    }

    let kind = if GenericSource::parse_catalog_html(
        &fetched.html,
        &fetched.final_url,
        UrlKind::WebsiteHome,
    )
    .is_ok()
    {
        CmsKind::UniversalHeuristic
    } else {
        CmsKind::BrowserOnly
    };

    SiteDetectionOutcome {
        base_url: root_final.clone(),
        display_name,
        kind,
        parser_ref: None,
        search_template: discovered_search
            .or_else(|| default_cms_search_template(&root_final, kind)),
        icon_candidates,
        adult_detected,
        reachability: SiteReachability::Reachable,
    }
}

/// Classifies an already-fetched HTML document purely by structural DOM/script signatures.
#[must_use]
pub fn detect_cms_from_html(doc: &Html, html: &str) -> Option<CmsKind> {
    let lower = html.to_ascii_lowercase();

    // 1) Gallery / Doujinshi card grids and gallery embedded metadata (check before generic WordPress)
    let has_gallery_cards = doc.select(&GALLERY_CARD_SEL).count() >= 4;
    if has_gallery_cards || looks_like_gallery_page(doc, html) {
        return Some(CmsKind::FlatGallery);
    }

    // 2) Madara (WordPress)
    let has_madara_dom = doc.select(&MADARA_ITEM_SEL).count() >= 2;
    if lower.contains("wp-content/themes/madara")
        || lower.contains("wp-manga")
        || (lower.contains("tab-summary") && lower.contains("wp-content"))
        || has_madara_dom
    {
        return Some(CmsKind::Madara);
    }

    // 3) MangaThemesia / MangaStream / MangaReader
    let has_themesia_dom = doc.select(&THEMESIA_ITEM_SEL).count() >= 2;
    if lower.contains("wp-content/themes/mangastream")
        || lower.contains("wp-content/themes/mangareader")
        || lower.contains("ts_reader.run")
        || has_themesia_dom
    {
        return Some(CmsKind::MangaThemesia);
    }

    // 4) WPComics-style item structure
    let has_wpcomics_dom = doc.select(&WPCOMICS_ITEM_SEL).count() >= 3;
    if has_wpcomics_dom || lower.contains("ctl00_divcenter") {
        return Some(CmsKind::WpComics);
    }

    // 5) Embedded client-side directory variables
    if lower.contains("vm.directory =") || lower.contains("vm.chapters =") {
        return Some(CmsKind::UniversalHeuristic);
    }

    None
}

/// Browses or searches a saved/detected source using its [`CmsKind`], automatically applying
/// domain mirror overrides and one-shot CMS fallback if a dedicated parser fails structurally.
///
/// Genuine network/gate/HTTP errors are returned directly (`Err(ScrapeError)`) instead of being
/// masked as empty search results.
///
/// # Errors
///
/// Returns [`ScrapeError`] when upstream requests fail or parsing cannot interpret the page.
#[expect(
    clippy::too_many_arguments,
    reason = "orchestrates saved source CMS dispatch, search template, and custom rule fallback"
)]
pub async fn browse_or_search_cms(
    http: &HttpClient,
    kind: CmsKind,
    base_url: &Url,
    parser_ref: Option<&str>,
    search_template: Option<&str>,
    query: Option<&str>,
    page: u32,
    custom_rules: &[CustomRuleSpec],
) -> Result<CatalogPage, ScrapeError> {
    let page_num = page.max(1);
    let clean_query = query.map(str::trim).filter(|q| !q.is_empty());

    match kind {
        CmsKind::CustomRule => {
            if let Some(rule_id) = parser_ref
                && let Some(spec) = custom_rules.iter().find(|r| r.id == rule_id)
            {
                match execute_rule_catalog(http, base_url, spec, clean_query, page_num).await {
                    Ok(cat) if !cat.entries.is_empty() || clean_query.is_some() => return Ok(cat),
                    Err(err) if is_gate_or_transport_err(&err) => return Err(err),
                    Ok(_) | Err(_) => {}
                }
            }
            browse_generic_or_cms_fallback(http, base_url, search_template, clean_query, page_num)
                .await
        }
        CmsKind::BuiltInAdapter => {
            browse_builtin_adapter(
                http,
                base_url,
                parser_ref,
                search_template,
                clean_query,
                page_num,
            )
            .await
        }
        CmsKind::MangaDexApi => {
            browse_mangadex_compatible_api(http, base_url, clean_query, page_num).await
        }
        CmsKind::ComickApi => {
            browse_comick_compatible_api(http, base_url, clean_query, page_num).await
        }
        CmsKind::Madara
        | CmsKind::MangaThemesia
        | CmsKind::WpComics
        | CmsKind::FlatGallery
        | CmsKind::FoolSlide => {
            browse_cms_html_family(
                http,
                kind,
                base_url,
                parser_ref,
                search_template,
                clean_query,
                page_num,
            )
            .await
        }
        CmsKind::UniversalHeuristic | CmsKind::BrowserOnly => {
            browse_generic_or_cms_fallback(http, base_url, search_template, clean_query, page_num)
                .await
        }
    }
}

async fn browse_builtin_adapter(
    http: &HttpClient,
    base_url: &Url,
    parser_ref: Option<&str>,
    search_template: Option<&str>,
    clean_query: Option<&str>,
    page_num: u32,
) -> Result<CatalogPage, ScrapeError> {
    let target_url = build_cms_target_url(
        base_url,
        CmsKind::BuiltInAdapter,
        parser_ref,
        search_template,
        clean_query,
        page_num,
    )?;
    let adapters = crate::source::default_adapters();
    if let Some(adapter_id) = parser_ref
        && let Some(adapter) = adapters.iter().find(|a| a.id() == adapter_id)
    {
        let mut effective_target = target_url;
        if adapter_id == "mangadex" && !is_mangadex_host(base_url.host_str().unwrap_or("")) {
            effective_target = build_cms_target_url(
                &MANGADEX_CANONICAL_BASE,
                CmsKind::BuiltInAdapter,
                Some("mangadex"),
                None,
                clean_query,
                page_num,
            )?;
        }
        match adapter.resolve(http, &effective_target).await {
            Ok(crate::model::Resolved::Catalog(cat)) => {
                if !cat.entries.is_empty() || clean_query.is_some() {
                    return Ok(*cat);
                }
            }
            Err(err) if is_gate_or_transport_err(&err) || adapter_id == "mangadex" => {
                return Err(err);
            }
            Ok(_) | Err(_) => {}
        }
    }
    browse_generic_or_cms_fallback(http, base_url, search_template, clean_query, page_num).await
}

async fn browse_cms_html_family(
    http: &HttpClient,
    kind: CmsKind,
    base_url: &Url,
    parser_ref: Option<&str>,
    search_template: Option<&str>,
    clean_query: Option<&str>,
    page_num: u32,
) -> Result<CatalogPage, ScrapeError> {
    let target_url = build_cms_target_url(
        base_url,
        kind,
        parser_ref,
        search_template,
        clean_query,
        page_num,
    )?;
    let fetched = http.fetch_html(&target_url).await?;
    if let Some(cat) = parse_cms_catalog_html(
        kind,
        &fetched.html,
        &fetched.final_url,
        base_url,
        clean_query,
        page_num,
    ) {
        return Ok(cat);
    }
    GenericSource::parse_catalog_html(
        &fetched.html,
        &fetched.final_url,
        if clean_query.is_some() {
            UrlKind::SearchPage
        } else {
            UrlKind::CatalogPage
        },
    )
    .map(|mut cat| {
        cat.query = clean_query.map(str::to_owned);
        cat.pagination.current_page = page_num;
        if cat.search_url_template.is_none() {
            cat.search_url_template = search_template
                .map(str::to_owned)
                .or_else(|| default_cms_search_template(base_url, kind));
        }
        cat
    })
}

/// Attempts Madara `WordPress` POST AJAX chapter listing (`/ajax/chapters/` or `/wp-admin/admin-ajax.php`)
/// when static HTML contains an empty chapter placeholder.
pub async fn fetch_madara_chapters_ajax(
    http: &HttpClient,
    manga_url: &Url,
    html: &str,
) -> Vec<ChapterRef> {
    let mut path = manga_url.path().trim_end_matches('/').to_owned();
    path.push_str("/ajax/chapters/");
    if let Ok(ajax_url) = manga_url.join(&path)
        && let Ok(resp) = http.post_form(&ajax_url, &[], Some(manga_url)).await
    {
        let doc = Html::parse_document(&resp.html);
        let base = effective_base_url(&doc, manga_url);
        let chaps = extract_chapters(&doc, &base);
        if !chaps.is_empty() {
            return chaps;
        }
    }

    if let Some(caps) = WP_ID_ATTR_RE.captures(html)
        && let Some(manga_id) = caps.get(1).map(|m| m.as_str())
        && let Ok(admin_ajax) = manga_url.join("/wp-admin/admin-ajax.php")
        && let Ok(resp) = http
            .post_form(
                &admin_ajax,
                &[("action", "manga_get_chapters"), ("manga", manga_id)],
                Some(manga_url),
            )
            .await
    {
        let doc = Html::parse_document(&resp.html);
        let base = effective_base_url(&doc, manga_url);
        let chaps = extract_chapters(&doc, &base);
        if !chaps.is_empty() {
            return chaps;
        }
    }

    Vec::new()
}

async fn browse_generic_or_cms_fallback(
    http: &HttpClient,
    base_url: &Url,
    search_template: Option<&str>,
    clean_query: Option<&str>,
    page_num: u32,
) -> Result<CatalogPage, ScrapeError> {
    let target = build_cms_target_url(
        base_url,
        CmsKind::UniversalHeuristic,
        None,
        search_template,
        clean_query,
        page_num,
    )?;
    let fetched = http.fetch_html(&target).await?;
    let doc = Html::parse_document(&fetched.html);
    if let Some(detected) = detect_cms_from_html(&doc, &fetched.html)
        && let Some(cat) = parse_cms_catalog_html(
            detected,
            &fetched.html,
            &fetched.final_url,
            base_url,
            clean_query,
            page_num,
        )
    {
        return Ok(cat);
    }
    let kind = if clean_query.is_some() {
        UrlKind::SearchPage
    } else if page_num == 1 && fetched.final_url.path() == "/" {
        UrlKind::WebsiteHome
    } else {
        UrlKind::CatalogPage
    };
    let mut cat = GenericSource::parse_catalog_html(&fetched.html, &fetched.final_url, kind)?;
    cat.query = clean_query.map(str::to_owned);
    cat.pagination.current_page = page_num;
    if page_num > 1 {
        cat.pagination.prev_url = build_cms_target_url(
            base_url,
            CmsKind::UniversalHeuristic,
            None,
            search_template,
            clean_query,
            page_num - 1,
        )
        .ok();
    }
    if !cat.entries.is_empty() {
        cat.pagination.next_url = build_cms_target_url(
            base_url,
            CmsKind::UniversalHeuristic,
            None,
            search_template,
            clean_query,
            page_num + 1,
        )
        .ok();
    }
    Ok(cat)
}

fn build_cms_target_url(
    base_url: &Url,
    kind: CmsKind,
    parser_ref: Option<&str>,
    search_template: Option<&str>,
    clean_query: Option<&str>,
    page_num: u32,
) -> Result<Url, ScrapeError> {
    if let Some(q) = clean_query {
        if let Some(tmpl) = search_template.filter(|t| t.contains("{query}")) {
            let encoded = percent_encode_query(q);
            let raw = tmpl.replace("{query}", &encoded);
            let mut u = Url::parse(&raw).map_err(|e| ScrapeError::InvalidUrl(e.to_string()))?;
            if page_num > 1 {
                u.query_pairs_mut()
                    .append_pair("page", &page_num.to_string());
            }
            return Ok(u);
        }

        let mut u = base_url.clone();
        match kind {
            CmsKind::BuiltInAdapter => {
                match parser_ref {
                    Some("mangadex") => {
                        u.set_path("/search");
                        u.query_pairs_mut().append_pair("q", q);
                    }
                    Some("truyenqq") => {
                        u.set_path("/tim-kiem.html");
                        u.query_pairs_mut().append_pair("q", q);
                    }
                    Some("moetruyen") => {
                        u.set_path("/manga");
                        u.query_pairs_mut().append_pair("q", q);
                    }
                    _ => {
                        u.query_pairs_mut().append_pair("s", q);
                    }
                }
                if page_num > 1 {
                    u.query_pairs_mut()
                        .append_pair("page", &page_num.to_string());
                }
            }
            CmsKind::Madara => {
                if page_num > 1 {
                    u.set_path(&format!("/page/{page_num}/"));
                }
                u.query_pairs_mut()
                    .append_pair("s", q)
                    .append_pair("post_type", "wp-manga");
            }
            CmsKind::MangaThemesia => {
                if page_num > 1 {
                    u.set_path(&format!("/page/{page_num}/"));
                }
                u.query_pairs_mut().append_pair("s", q);
            }
            CmsKind::WpComics => {
                u.set_path("/tim-truyen");
                u.query_pairs_mut().append_pair("keyword", q);
                if page_num > 1 {
                    u.query_pairs_mut()
                        .append_pair("page", &page_num.to_string());
                }
            }
            CmsKind::FlatGallery => {
                u.set_path("/search/");
                u.query_pairs_mut().append_pair("q", q);
                if page_num > 1 {
                    u.query_pairs_mut()
                        .append_pair("page", &page_num.to_string());
                }
            }
            _ => {
                u.query_pairs_mut().append_pair("s", q);
                if page_num > 1 {
                    u.query_pairs_mut()
                        .append_pair("page", &page_num.to_string());
                }
            }
        }
        return Ok(u);
    }

    let mut u = base_url.clone();
    match kind {
        CmsKind::BuiltInAdapter => {
            if parser_ref == Some("mangadex") {
                u.set_path("/titles");
            }
            if page_num > 1 {
                u.query_pairs_mut()
                    .append_pair("page", &page_num.to_string());
            }
        }
        CmsKind::Madara | CmsKind::MangaThemesia => {
            if page_num > 1 {
                u.set_path(&format!("/page/{page_num}/"));
            }
        }
        _ => {
            if page_num > 1 {
                u.query_pairs_mut()
                    .append_pair("page", &page_num.to_string());
            }
        }
    }
    Ok(u)
}

fn parse_cms_catalog_html(
    kind: CmsKind,
    html: &str,
    page_url: &Url,
    base_root: &Url,
    clean_query: Option<&str>,
    page_num: u32,
) -> Option<CatalogPage> {
    let doc = Html::parse_document(html);
    let base = effective_base_url(&doc, page_url);
    let entries = extract_cms_cards(kind, &doc, &base)?;
    if entries.is_empty() && clean_query.is_none() {
        return None;
    }

    let host = base_root
        .host_str()
        .unwrap_or("unknown")
        .to_ascii_lowercase();
    let site_name = host.trim_start_matches("www.").to_owned();
    let title = extract_title(&doc).unwrap_or_else(|| site_name.clone());
    let next_url = if entries.is_empty() {
        None
    } else {
        build_cms_target_url(base_root, kind, None, None, clean_query, page_num + 1).ok()
    };
    let prev_url = if page_num > 1 {
        build_cms_target_url(base_root, kind, None, None, clean_query, page_num - 1).ok()
    } else {
        None
    };

    Some(CatalogPage {
        url: page_url.clone(),
        site: SiteId::Known(format!("cms:{kind}")),
        site_name,
        title: if let Some(q) = clean_query {
            format!("{title} — {q}")
        } else {
            title
        },
        url_kind: if clean_query.is_some() {
            UrlKind::SearchPage
        } else {
            UrlKind::CatalogPage
        },
        query: clean_query.map(str::to_owned),
        search_url_template: discover_search_template(&doc, base_root)
            .or_else(|| default_cms_search_template(base_root, kind)),
        entries,
        categories: Vec::new(),
        pagination: PaginationInfo {
            current_page: page_num,
            total_pages: None,
            prev_url,
            next_url,
        },
    })
}

fn extract_cms_cards(kind: CmsKind, doc: &Html, base: &Url) -> Option<Vec<CatalogEntry>> {
    let item_sel = match kind {
        CmsKind::Madara => &*MADARA_ITEM_SEL,
        CmsKind::MangaThemesia => &*THEMESIA_ITEM_SEL,
        CmsKind::WpComics => &*WPCOMICS_ITEM_SEL,
        CmsKind::FlatGallery => &*GALLERY_CARD_SEL,
        _ => return None,
    };

    let mut entries = Vec::new();
    let mut seen = HashSet::new();
    for card in doc.select(item_sel) {
        let Some(link) = card.select(&LINK_SEL).next() else {
            continue;
        };
        let Some(href) = link.value().attr("href") else {
            continue;
        };
        let Some(url) = abs_url(base, href) else {
            continue;
        };
        let title = card
            .select(&TITLE_HINT_SEL)
            .next()
            .map(|el| clean_text(&el.text().collect::<String>()))
            .filter(|s| !s.is_empty())
            .or_else(|| {
                link.value()
                    .attr("title")
                    .map(clean_text)
                    .filter(|s| !s.is_empty())
            })
            .or_else(|| {
                card.select(&IMG_SEL)
                    .next()
                    .and_then(|img| img.value().attr("alt"))
                    .map(clean_text)
                    .filter(|s| !s.is_empty())
            })
            .or_else(|| {
                let txt = clean_text(&link.text().collect::<String>());
                if txt.is_empty() { None } else { Some(txt) }
            });
        let Some(title) = title else {
            continue;
        };

        let cover = card
            .select(&IMG_SEL)
            .next()
            .and_then(|img| image_candidates(img, base).into_iter().next())
            .filter(|u| !is_ignored_image_url(u));

        let latest_chapter = card
            .select(&LATEST_CH_SEL)
            .next()
            .map(|el| clean_text(&el.text().collect::<String>()))
            .filter(|s| !s.is_empty());

        let key = url.as_str().trim_end_matches('/').to_owned();
        if !seen.insert(key) {
            continue;
        }
        entries.push(CatalogEntry {
            url,
            title,
            alt_titles: Vec::new(),
            cover,
            latest_chapter,
            status: None,
            genres: Vec::new(),
            source_id: None,
        });
    }
    Some(entries)
}

async fn probe_known_api_endpoints(
    http: &HttpClient,
    base_url: &Url,
) -> Option<(CmsKind, Option<String>)> {
    if let Ok(md_url) = base_url.join("/manga?limit=1")
        && let Ok(page) = http.fetch_json(&md_url, "cms_probe", None).await
        && let Ok(val) = serde_json::from_str::<Value>(&page.html)
        && val.get("result").and_then(Value::as_str) == Some("ok")
        && val.get("data").and_then(Value::as_array).is_some()
    {
        return Some((
            CmsKind::MangaDexApi,
            Some(format!("{base_url}/manga?title={{query}}")),
        ));
    }

    if let Ok(ck_url) = base_url.join("/comic?page=1&limit=1")
        && let Ok(page) = http.fetch_json(&ck_url, "cms_probe", None).await
        && let Ok(val) = serde_json::from_str::<Value>(&page.html)
        && val.get("hid").is_some()
        && val.get("slug").is_some()
    {
        return Some((
            CmsKind::ComickApi,
            Some(format!("{base_url}/v1.0/search?q={{query}}")),
        ));
    }

    if let Ok(fs_url) = base_url.join("/api/reader/chapter_groups")
        && let Ok(page) = http.fetch_json(&fs_url, "cms_probe", None).await
        && let Ok(val) = serde_json::from_str::<Value>(&page.html)
        && val.get("chapter_groups").is_some()
    {
        return Some((CmsKind::FoolSlide, None));
    }

    None
}

async fn probe_rule_endpoints<'a>(
    http: &HttpClient,
    base_url: &Url,
    rules: &'a [CustomRuleSpec],
) -> Option<&'a CustomRuleSpec> {
    for rule in rules {
        for path in &rule.probe_paths {
            let trimmed = path.trim();
            if trimmed.is_empty() {
                continue;
            }
            let Ok(u) = base_url.join(trimmed) else {
                continue;
            };
            if let Ok(resp) = http.fetch_html(&u).await
                && match_rule_by_fingerprint(&resp.html, std::slice::from_ref(rule)).is_some()
            {
                return Some(rule);
            }
        }
    }
    None
}

async fn browse_mangadex_compatible_api(
    http: &HttpClient,
    base_url: &Url,
    query: Option<&str>,
    page_num: u32,
) -> Result<CatalogPage, ScrapeError> {
    let offset = page_num.saturating_sub(1).saturating_mul(24);
    let mut url = base_url
        .join("/manga")
        .map_err(|e| ScrapeError::InvalidUrl(e.to_string()))?;
    {
        let mut qp = url.query_pairs_mut();
        qp.append_pair("limit", "24");
        qp.append_pair("offset", &offset.to_string());
        if let Some(q) = query {
            qp.append_pair("title", q);
        }
    }
    let page = http.fetch_json(&url, "mangadex_compat", None).await?;
    let val: Value = serde_json::from_str(&page.html).map_err(|_| ScrapeError::NotRecognized {
        url: page.final_url.clone(),
        reason: "invalid json from api",
    })?;
    let mut entries = Vec::new();
    if let Some(arr) = val.get("data").and_then(Value::as_array) {
        for item in arr {
            let Some(id) = item.get("id").and_then(Value::as_str) else {
                continue;
            };
            let title = item
                .pointer("/attributes/title/en")
                .or_else(|| item.pointer("/attributes/title/vi"))
                .or_else(|| {
                    item.pointer("/attributes/title")
                        .and_then(Value::as_object)
                        .and_then(|m| m.values().next())
                })
                .and_then(Value::as_str)
                .unwrap_or(id)
                .to_owned();
            if let Ok(manga_url) = base_url.join(&format!("/title/{id}")) {
                entries.push(CatalogEntry {
                    url: manga_url,
                    title,
                    alt_titles: Vec::new(),
                    cover: None,
                    latest_chapter: None,
                    status: item
                        .pointer("/attributes/status")
                        .and_then(Value::as_str)
                        .map(str::to_owned),
                    genres: Vec::new(),
                    source_id: Some(id.to_owned()),
                });
            }
        }
    }
    let host = base_url.host_str().unwrap_or("api").to_owned();
    Ok(CatalogPage {
        url: page.final_url,
        site: SiteId::Known("cms:mangadex_api".to_owned()),
        site_name: host.clone(),
        title: query.map_or(host, |q| format!("Search: {q}")),
        url_kind: if query.is_some() {
            UrlKind::SearchPage
        } else {
            UrlKind::CatalogPage
        },
        query: query.map(str::to_owned),
        search_url_template: Some(format!("{base_url}/manga?title={{query}}")),
        entries,
        categories: Vec::new(),
        pagination: PaginationInfo {
            current_page: page_num,
            total_pages: None,
            prev_url: None,
            next_url: None,
        },
    })
}

async fn browse_comick_compatible_api(
    http: &HttpClient,
    base_url: &Url,
    query: Option<&str>,
    page_num: u32,
) -> Result<CatalogPage, ScrapeError> {
    let mut url = base_url
        .join("/v1.0/search")
        .map_err(|e| ScrapeError::InvalidUrl(e.to_string()))?;
    {
        let mut qp = url.query_pairs_mut();
        qp.append_pair("limit", "24");
        qp.append_pair("page", &page_num.to_string());
        if let Some(q) = query {
            qp.append_pair("q", q);
        }
    }
    let page = http.fetch_json(&url, "comick_compat", None).await?;
    let val: Value = serde_json::from_str(&page.html).map_err(|_| ScrapeError::NotRecognized {
        url: page.final_url.clone(),
        reason: "invalid json from comick-style api",
    })?;
    let mut entries = Vec::new();
    if let Some(arr) = val.as_array() {
        for item in arr {
            let Some(slug) = item.get("slug").and_then(Value::as_str) else {
                continue;
            };
            let title = item
                .get("title")
                .and_then(Value::as_str)
                .unwrap_or(slug)
                .to_owned();
            if let Ok(manga_url) = base_url.join(&format!("/comic/{slug}")) {
                entries.push(CatalogEntry {
                    url: manga_url,
                    title,
                    alt_titles: Vec::new(),
                    cover: None,
                    latest_chapter: None,
                    status: None,
                    genres: Vec::new(),
                    source_id: item.get("hid").and_then(Value::as_str).map(str::to_owned),
                });
            }
        }
    }
    let host = base_url.host_str().unwrap_or("api").to_owned();
    Ok(CatalogPage {
        url: page.final_url,
        site: SiteId::Known("cms:comick_api".to_owned()),
        site_name: host.clone(),
        title: query.map_or(host, |q| format!("Search: {q}")),
        url_kind: if query.is_some() {
            UrlKind::SearchPage
        } else {
            UrlKind::CatalogPage
        },
        query: query.map(str::to_owned),
        search_url_template: Some(format!("{base_url}/v1.0/search?q={{query}}")),
        entries,
        categories: Vec::new(),
        pagination: PaginationInfo {
            current_page: page_num,
            total_pages: None,
            prev_url: None,
            next_url: None,
        },
    })
}

fn default_origin_favicons(base: &Url) -> Vec<Url> {
    let mut out = Vec::new();
    for suffix in [
        "/favicon.ico",
        "/apple-touch-icon.png",
        "/apple-touch-icon-precomposed.png",
    ] {
        if let Ok(u) = base.join(suffix) {
            out.push(u);
        }
    }
    out
}

fn default_cms_search_template(base_url: &Url, kind: CmsKind) -> Option<String> {
    match kind {
        CmsKind::Madara => Some(format!("{base_url}/?s={{query}}&post_type=wp-manga")),
        CmsKind::MangaThemesia | CmsKind::UniversalHeuristic => {
            Some(format!("{base_url}/?s={{query}}"))
        }
        CmsKind::WpComics => Some(format!("{base_url}/tim-truyen?keyword={{query}}")),
        CmsKind::FlatGallery => Some(format!("{base_url}/search/?q={{query}}")),
        _ => None,
    }
}

fn build_rule_search_hint(base_url: &Url, rule: &CustomRuleSpec) -> Option<String> {
    let ep = rule
        .catalog
        .search_endpoint
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or("/");
    let param = rule
        .catalog
        .search_param
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or("s");
    base_url.join(ep).ok().map(|u| {
        let sep = if u.query().is_some() { "&" } else { "?" };
        format!("{u}{sep}{param}={{query}}")
    })
}

fn is_gate_or_transport_err(err: &ScrapeError) -> bool {
    matches!(
        err,
        ScrapeError::Blocked { .. }
            | ScrapeError::AccessRestricted { .. }
            | ScrapeError::RateLimited { .. }
            | ScrapeError::Timeout { .. }
            | ScrapeError::DnsFailure { .. }
            | ScrapeError::TlsFailure { .. }
            | ScrapeError::Proxy(_)
            | ScrapeError::DisallowedHost(_)
    )
}

fn percent_encode_query(raw: &str) -> String {
    let mut serializer = url::form_urlencoded::Serializer::new(String::new());
    serializer.append_pair("q", raw);
    let encoded = serializer.finish();
    encoded.strip_prefix("q=").unwrap_or(&encoded).to_owned()
}
