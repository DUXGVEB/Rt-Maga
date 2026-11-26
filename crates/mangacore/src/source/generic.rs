//! Generic heuristic adapter that works across arbitrary manga websites.

use std::collections::HashSet;
use std::sync::LazyLock;

use async_trait::async_trait;
use scraper::{Html, Selector};
use url::Url;

use crate::ExtractionStrategyKind;
use crate::detect::{PageInspectInput, SessionPresence, analyze_html_page};
use crate::error::ScrapeError;
use crate::extract::{
    HtmlMinimumPolicy, extract_chapter_html_adaptive, extract_structured_chapter,
    extract_structured_manga,
};
use crate::http::HttpClient;
use crate::model::{
    CatalogEntry, CatalogPage, Chapter, ChapterRef, Manga, NavLink, Resolved, SiteId,
};
use crate::parse::chapters::{extract_chapter_number, extract_chapters, format_chapter_num};
use crate::parse::classify::{PageKind, classify_page};
use crate::parse::dom::{
    abs_url, clean_text, effective_base_url, image_candidates, is_inside_chrome,
};
use crate::parse::gallery::{parse_gallery_chapter, parse_gallery_manga};
use crate::parse::meta::{
    extract_cover, extract_creators, extract_description, extract_status, extract_tags_and_genres,
    extract_title,
};
use crate::parse::nav::{extract_manga_url_from_chapter, extract_prev_next};
use crate::recognize::{UrlKind, classify_normalized_url};
use crate::source::{AdapterAuthSupport, AdapterMetadata, Source};

static CARD_LINK_SEL: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("a[href]").expect("valid static selector"));
static IMG_SEL: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("img").expect("valid static selector"));
static SEARCH_FORM_SEL: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("form").expect("valid static selector"));
static INPUT_NAME_SEL: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("input[name]").expect("valid static selector"));

/// Generic heuristic manga source for websites without a dedicated adapter.
#[derive(Debug, Default, Clone, Copy)]
pub struct GenericSource;

impl GenericSource {
    /// Parses a manga overview page using generic HTML heuristics and structured JSON-LD / Next.js fallback.
    ///
    /// # Errors
    ///
    /// Returns [`ScrapeError::NotRecognized`] if no chapter list can be discovered.
    pub fn parse_manga_html(html: &str, page_url: &Url) -> Result<Manga, ScrapeError> {
        let doc = Html::parse_document(html);
        let base = effective_base_url(&doc, page_url);

        let chapters = extract_chapters(&doc, &base);
        if chapters.is_empty() {
            if let Some(structured) = extract_structured_manga(&doc, page_url, &base) {
                return Ok(structured);
            }
            if let Some(gallery_manga) = parse_gallery_manga(html, page_url, &base) {
                return Ok(gallery_manga);
            }
            return Err(ScrapeError::NotRecognized {
                url: page_url.clone(),
                reason: "generic parser found no chapter list",
            });
        }

        let host = page_url.host_str().unwrap_or("unknown").to_owned();
        let title = extract_title(&doc).unwrap_or_else(|| host.clone());
        let cover = extract_cover(&doc, &base);
        let description = extract_description(&doc);
        let (genres, tags) = extract_tags_and_genres(&doc);
        let (authors, artists) = extract_creators(&doc);
        let status = extract_status(&doc);

        let mut manga = Manga::minimal(
            page_url.clone(),
            SiteId::generic(host),
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

    /// Parses a chapter reading page using Strategy C structured data, upgraded Strategy A DOM
    /// extraction (`<img>`, `<picture>`, CSS `background-image`, lazy attrs), gallery thumbnail
    /// promotion, and confidence-aware page thresholds.
    ///
    /// # Errors
    ///
    /// Returns [`ScrapeError::NotRecognized`] if no readable comic chapter images are found.
    pub fn parse_chapter_html(html: &str, page_url: &Url) -> Result<Chapter, ScrapeError> {
        let doc = Html::parse_document(html);
        let base = effective_base_url(&doc, page_url);

        if page_url.fragment() == Some("rtmaga-gallery")
            && let Some(gallery_ch) = parse_gallery_chapter(html, page_url, &base)
        {
            return Ok(gallery_ch);
        }

        if let Some(structured) = extract_structured_chapter(&doc, html, page_url, &base) {
            return Ok(structured.chapter);
        }

        let analysis = analyze_html_page(PageInspectInput {
            submitted_url: page_url,
            final_url: page_url,
            status: 200,
            content_type: Some("text/html"),
            html,
            doc: &doc,
            session: SessionPresence::NoCookies,
        });
        let policy = if analysis.page_kind == UrlKind::ChapterPage && analysis.confidence >= 0.70 {
            HtmlMinimumPolicy::ConfidentChapterOneOrMore
        } else {
            HtmlMinimumPolicy::StandardThree
        };

        let adaptive = extract_chapter_html_adaptive(&doc, page_url, &base, policy);
        if let Some(chapter) = adaptive.chapter {
            return Ok(chapter);
        }

        let script_images = crate::parse::images::extract_chapter_images(&doc, page_url, &base);
        let min_needed = if policy == HtmlMinimumPolicy::ConfidentChapterOneOrMore {
            1
        } else {
            crate::parse::classify::MIN_CHAPTER_IMAGES
        };
        if script_images.len() >= min_needed {
            let nav = extract_prev_next(&doc, page_url, &base);
            let manga_url = extract_manga_url_from_chapter(&doc, page_url, &base);
            let name = extract_title(&doc).unwrap_or_else(|| {
                extract_chapter_number(page_url.path()).map_or_else(
                    || "Chapter".to_owned(),
                    |n| format!("Chapter {}", format_chapter_num(n)),
                )
            });
            return Ok(Chapter {
                url: page_url.clone(),
                name,
                images: script_images,
                prev: nav.prev,
                next: nav.next,
                manga_url,
            });
        }

        if let Some(gallery_ch) = parse_gallery_chapter(html, page_url, &base) {
            return Ok(gallery_ch);
        }

        Err(ScrapeError::NotRecognized {
            url: page_url.clone(),
            reason: "generic parser found too few chapter images",
        })
    }

    /// Attempts to extract a validated catalog/homepage listing from a generic manga site.
    ///
    /// Requires at least 4 distinct same-host card links that carry thumbnail images and titles.
    ///
    /// # Errors
    ///
    /// Returns [`ScrapeError::NotRecognized`] when too few validated catalog cards are present.
    #[expect(
        clippy::too_many_lines,
        reason = "generic heuristic catalog extractor scanning cards, categories, pagination, and filter schema"
    )]
    pub fn parse_catalog_html(
        html: &str,
        page_url: &Url,
        url_kind: UrlKind,
    ) -> Result<CatalogPage, ScrapeError> {
        let doc = Html::parse_document(html);
        let base = effective_base_url(&doc, page_url);
        let host = page_url
            .host_str()
            .unwrap_or("unknown")
            .to_ascii_lowercase();

        let mut entries = Vec::new();
        let mut seen = HashSet::new();

        for a in doc.select(&CARD_LINK_SEL) {
            if is_inside_chrome(a) {
                continue;
            }
            let Some(href) = a.value().attr("href") else {
                continue;
            };
            let Some(url) = abs_url(&base, href) else {
                continue;
            };
            if url.host_str().unwrap_or("").to_ascii_lowercase() != host {
                continue;
            }
            if url.path() == page_url.path() || url.path().trim_matches('/').is_empty() {
                continue;
            }
            // Skip obvious chapter links in favor of manga detail cards.
            if extract_chapter_number(url.path()).is_some() {
                continue;
            }
            let Some(img) = a.select(&IMG_SEL).next() else {
                continue;
            };
            let cover = image_candidates(img, &base).into_iter().next();
            if cover.is_none() {
                continue;
            }

            let title = a
                .value()
                .attr("title")
                .map(clean_text)
                .filter(|s| !s.is_empty())
                .or_else(|| {
                    img.value()
                        .attr("alt")
                        .map(clean_text)
                        .filter(|s| !s.is_empty())
                })
                .or_else(|| {
                    let t = clean_text(&a.text().collect::<Vec<_>>().join(" "));
                    if t.is_empty() { None } else { Some(t) }
                });
            let Some(title) = title else {
                continue;
            };
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
                chapter_count: None,
                updated_at: None,
            });
            if entries.len() >= 120 {
                break;
            }
        }

        if entries.len() < 4 {
            return Err(ScrapeError::NotRecognized {
                url: page_url.clone(),
                reason: "generic parser found too few catalog manga entries",
            });
        }

        let site_name = host.trim_start_matches("www.").to_owned();
        let title = extract_title(&doc).unwrap_or_else(|| site_name.clone());
        let search_url_template = discover_search_template(&doc, &base);
        let categories = discover_category_links(&doc, &base, &host);
        let pagination = crate::parse::pagination::extract_universal_pagination(
            &doc,
            &base,
            page_url,
            !entries.is_empty(),
        );
        let filter_schema = crate::parse::filters::extract_generic_filter_schema(&doc, &base, None);
        Ok(CatalogPage {
            url: page_url.clone(),
            site: SiteId::generic(host),
            site_name,
            title,
            url_kind,
            query: None,
            search_url_template,
            entries,
            categories,
            pagination,
            filter_schema,
        })
    }

    /// Classifies and resolves already-fetched HTML into a [`Resolved`] result.
    ///
    /// When the page is a chapter page and a parent manga URL is found, fetches the parent page
    /// through `http` to assemble the full chapter list.
    ///
    /// # Errors
    ///
    /// Returns [`ScrapeError::NotRecognized`] when the HTML does not resemble a manga, chapter,
    /// or catalog page.
    pub async fn resolve_html(
        http: &HttpClient,
        html: &str,
        page_url: &Url,
    ) -> Result<Resolved, ScrapeError> {
        let host = page_url
            .host_str()
            .unwrap_or("unknown")
            .to_ascii_lowercase();
        let (_, _, url_kind) = classify_normalized_url(page_url, &host);

        if matches!(
            url_kind,
            UrlKind::WebsiteHome | UrlKind::CatalogPage | UrlKind::SearchPage
        ) && let Ok(catalog) = Self::parse_catalog_html(html, page_url, url_kind)
        {
            return Ok(Resolved::Catalog(Box::new(catalog)));
        }

        let kind = {
            let doc = Html::parse_document(html);
            classify_page(&doc, page_url)
        };

        match kind {
            PageKind::Chapter => {
                let chapter = Self::parse_chapter_html(html, page_url)?;
                let chapter_ref = ChapterRef::simple(
                    chapter.url.clone(),
                    chapter.name.clone(),
                    extract_chapter_number(&chapter.name)
                        .or_else(|| extract_chapter_number(chapter.url.path())),
                );

                if let Some(manga_url) = chapter.manga_url.as_ref()
                    && let Ok(parent_page) = http.fetch_html(manga_url).await
                    && let Ok(manga) =
                        Self::parse_manga_html(&parent_page.html, &parent_page.final_url)
                {
                    let matched = manga
                        .chapters
                        .iter()
                        .find(|c| c.url == chapter.url)
                        .cloned()
                        .unwrap_or(chapter_ref);
                    return Ok(Resolved::Chapter {
                        manga: Box::new(manga),
                        at: Box::new(matched),
                    });
                }

                let fallback_manga = Manga::minimal(
                    chapter.manga_url.unwrap_or_else(|| chapter.url.clone()),
                    SiteId::generic(host),
                    chapter.name.clone(),
                    None,
                    vec![chapter_ref.clone()],
                );
                Ok(Resolved::Chapter {
                    manga: Box::new(fallback_manga),
                    at: Box::new(chapter_ref),
                })
            }
            PageKind::Manga => {
                let manga = Self::parse_manga_html(html, page_url)?;
                Ok(Resolved::Manga(Box::new(manga)))
            }
            PageKind::Unknown => {
                if html.contains("wp-manga") || html.contains("wp-content/themes/madara") {
                    let ajax_chaps =
                        crate::cms::fetch_madara_chapters_ajax(http, page_url, html).await;
                    if !ajax_chaps.is_empty() {
                        let doc = Html::parse_document(html);
                        let base = effective_base_url(&doc, page_url);
                        let title = extract_title(&doc).unwrap_or_else(|| host.clone());
                        let cover = extract_cover(&doc, &base);
                        let (genres, tags) = extract_tags_and_genres(&doc);
                        let (authors, artists) = extract_creators(&doc);
                        let mut manga = Manga::minimal(
                            page_url.clone(),
                            SiteId::generic(host),
                            title,
                            cover,
                            ajax_chaps,
                        );
                        manga.description = extract_description(&doc);
                        manga.genres = genres;
                        manga.tags = tags;
                        manga.authors = authors;
                        manga.artists = artists;
                        manga.status = extract_status(&doc);
                        return Ok(Resolved::Manga(Box::new(manga)));
                    }
                }
                if let Ok(catalog) = Self::parse_catalog_html(html, page_url, UrlKind::CatalogPage)
                {
                    return Ok(Resolved::Catalog(Box::new(catalog)));
                }
                Err(ScrapeError::NotRecognized {
                    url: page_url.clone(),
                    reason: "page has neither chapter images, chapter list, nor manga catalog",
                })
            }
        }
    }
}

#[async_trait]
impl Source for GenericSource {
    fn id(&self) -> &'static str {
        "generic"
    }

    fn metadata(&self) -> AdapterMetadata {
        AdapterMetadata {
            id: "generic",
            name: "Universal Heuristic Engine",
            version: "2.0.0",
            domains: &["*"],
            auth_support: AdapterAuthSupport::OptionalUserSession,
            strategies: &[
                ExtractionStrategyKind::StructuredDataAndApi,
                ExtractionStrategyKind::NativeHtml,
                ExtractionStrategyKind::BrowserRendering,
            ],
            known_limitations: &[
                "Sites with heavy obfuscation, paid accounts, or canvas DRM may require authorized browser reading.",
            ],
        }
    }

    fn matches(&self, _url: &Url) -> bool {
        true
    }

    async fn resolve(&self, http: &HttpClient, url: &Url) -> Result<Resolved, ScrapeError> {
        let page = http.fetch_html(url).await?;
        Self::resolve_html(http, &page.html, &page.final_url).await
    }

    async fn chapter(&self, http: &HttpClient, url: &Url) -> Result<Chapter, ScrapeError> {
        let page = http.fetch_html(url).await?;
        Self::parse_chapter_html(&page.html, &page.final_url)
    }
}

/// Inspects `<form>` elements on a catalog or homepage to synthesize a `{query}` search URL template.
#[must_use]
pub fn discover_search_template(doc: &Html, base: &Url) -> Option<String> {
    for form in doc.select(&SEARCH_FORM_SEL) {
        let method = form.value().attr("method").unwrap_or("get");
        if !method.eq_ignore_ascii_case("get") {
            continue;
        }
        let action_raw = form.value().attr("action").unwrap_or("/");
        let Some(action_url) = abs_url(base, action_raw) else {
            continue;
        };
        for input in form.select(&INPUT_NAME_SEL) {
            let input_type = input.value().attr("type").unwrap_or("text");
            if !matches!(
                input_type.to_ascii_lowercase().as_str(),
                "text" | "search" | ""
            ) {
                continue;
            }
            let Some(name) = input.value().attr("name").map(str::trim) else {
                continue;
            };
            if matches!(
                name.to_ascii_lowercase().as_str(),
                "q" | "s" | "search" | "keyword" | "query" | "title"
            ) {
                let sep = if action_url.query().is_some() {
                    "&"
                } else {
                    "?"
                };
                return Some(format!("{action_url}{sep}{name}={{query}}"));
            }
        }
    }
    None
}

fn discover_category_links(doc: &Html, base: &Url, host: &str) -> Vec<NavLink> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for a in doc.select(&CARD_LINK_SEL) {
        let Some(href) = a.value().attr("href") else {
            continue;
        };
        let Some(u) = abs_url(base, href) else {
            continue;
        };
        if u.host_str().unwrap_or("").to_ascii_lowercase() != host {
            continue;
        }
        let path_low = u.path().to_ascii_lowercase();
        if ![
            "/genre/",
            "/genres/",
            "/the-loai/",
            "/manga-genre/",
            "/tag/",
            "/tags/",
            "/category/",
            "/danh-muc/",
            "/latest",
            "/popular",
            "/completed",
            "/hoan-thanh",
            "/moi-cap-nhat",
        ]
        .iter()
        .any(|p| path_low.contains(p))
        {
            continue;
        }
        let label = clean_text(&a.text().collect::<String>());
        if label.len() < 2 || label.len() > 36 {
            continue;
        }
        if seen.insert(u.as_str().to_owned()) {
            out.push(NavLink {
                label,
                url: u,
                kind: UrlKind::CatalogPage,
            });
            if out.len() >= 96 {
                break;
            }
        }
    }
    out
}
