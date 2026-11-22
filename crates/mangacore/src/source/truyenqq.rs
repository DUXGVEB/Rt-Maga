//! Dedicated adapter for `TruyenQQ` mirror domains (`truyenqq*.com`, `truyenqq*.net`, etc.).

use std::collections::HashSet;
use std::sync::LazyLock;

use async_trait::async_trait;
use regex::Regex;
use scraper::{Html, Selector};
use url::Url;

use crate::error::ScrapeError;
use crate::http::HttpClient;
use crate::model::{Chapter, ChapterRef, Manga, PageImage, Resolved, SiteId};
use crate::parse::chapters::extract_chapter_number;
use crate::parse::dom::{abs_url, clean_text, effective_base_url};
use crate::parse::nav::extract_manga_url_from_chapter;
use crate::source::Source;

static TITLE_SEL: LazyLock<Selector> = LazyLock::new(|| {
    Selector::parse(r#"h1[itemprop="name"], .book_other h1, h1"#).expect("valid selector")
});
static COVER_SEL: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse(".book_avatar img").expect("valid selector"));
static CHAPTER_LINK_SEL: LazyLock<Selector> = LazyLock::new(|| {
    Selector::parse(".works-chapter-item .name-chap a[href], .works-chapter-list a[href]")
        .expect("valid selector")
});
static PAGE_IMG_SEL: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse(".page-chapter img").expect("valid selector"));
static PREV_LINK_SEL: LazyLock<Selector> = LazyLock::new(|| {
    Selector::parse("a.link-prev-chap[href], a.prev[href]").expect("valid selector")
});
static NEXT_LINK_SEL: LazyLock<Selector> = LazyLock::new(|| {
    Selector::parse("a.link-next-chap[href], a.next[href]").expect("valid selector")
});
static CHAPTER_TITLE_SEL: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("h1.detail-title, .txt-primary, h1").expect("valid selector"));
static CHAP_URL_SUFFIX_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)-chap-\d+(?:[.-]\d+)?(?:\.html?)?/?$").expect("valid regex"));

/// Site adapter for `TruyenQQ` mirror domains.
#[derive(Debug, Default, Clone, Copy)]
pub struct TruyenQqSource;

impl TruyenQqSource {
    /// Returns `true` if `url` points to a `TruyenQQ` chapter URL (`...-chap-<N>`).
    pub fn is_chapter_url(url: &Url) -> bool {
        CHAP_URL_SUFFIX_RE.is_match(url.path())
    }

    /// Parses a `TruyenQQ` manga detail page from raw HTML.
    ///
    /// # Errors
    ///
    /// Returns [`ScrapeError::NotRecognized`] if the page lacks both a title and chapter items.
    pub fn parse_manga_html(html: &str, page_url: &Url) -> Result<Manga, ScrapeError> {
        let doc = Html::parse_document(html);
        let base = effective_base_url(&doc, page_url);

        let title = doc
            .select(&TITLE_SEL)
            .next()
            .map(|el| clean_text(&el.text().collect::<String>()))
            .filter(|s| !s.is_empty())
            .ok_or_else(|| ScrapeError::NotRecognized {
                url: page_url.clone(),
                reason: "truyenqq title not found",
            })?;

        let cover = doc.select(&COVER_SEL).next().and_then(|img| {
            img.value()
                .attr("src")
                .or_else(|| img.value().attr("data-original"))
                .or_else(|| img.value().attr("data-fb"))
                .and_then(|raw| abs_url(&base, raw))
        });

        let mut seen = HashSet::new();
        let mut chapters: Vec<(usize, ChapterRef)> = Vec::new();

        for (dom_idx, a) in doc.select(&CHAPTER_LINK_SEL).enumerate() {
            let Some(href) = a.value().attr("href") else {
                continue;
            };
            let Some(url) = abs_url(&base, href) else {
                continue;
            };
            if !seen.insert(url.as_str().to_owned()) {
                continue;
            }
            let name = clean_text(&a.text().collect::<String>());
            if name.is_empty() {
                continue;
            }
            let number =
                extract_chapter_number(&name).or_else(|| extract_chapter_number(url.path()));
            chapters.push((dom_idx, ChapterRef { url, name, number }));
        }

        if chapters.is_empty() {
            return Err(ScrapeError::NotRecognized {
                url: page_url.clone(),
                reason: "truyenqq chapter list empty",
            });
        }

        // TruyenQQ lists chapters newest-first; sort ascending for consistent reading order.
        chapters.sort_by(|(idx_a, a), (idx_b, b)| match (a.number, b.number) {
            (Some(na), Some(nb)) => na.total_cmp(&nb).then_with(|| idx_b.cmp(idx_a)),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => idx_b.cmp(idx_a),
        });

        Ok(Manga {
            url: page_url.clone(),
            site: SiteId::known("truyenqq"),
            title,
            cover,
            chapters: chapters.into_iter().map(|(_, ch)| ch).collect(),
        })
    }

    /// Parses a `TruyenQQ` chapter reading page from raw HTML.
    ///
    /// # Errors
    ///
    /// Returns [`ScrapeError::NotRecognized`] if no `.page-chapter img` elements are found.
    pub fn parse_chapter_html(html: &str, page_url: &Url) -> Result<Chapter, ScrapeError> {
        let doc = Html::parse_document(html);
        let base = effective_base_url(&doc, page_url);

        let name = doc
            .select(&CHAPTER_TITLE_SEL)
            .next()
            .map(|el| clean_text(&el.text().collect::<String>()))
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| {
                extract_chapter_number(page_url.path()).map_or_else(
                    || "Chương".to_owned(),
                    |n| format!("Chương {}", crate::parse::chapters::format_chapter_num(n)),
                )
            });

        let mut seen = HashSet::new();
        let mut images = Vec::new();

        for img in doc.select(&PAGE_IMG_SEL) {
            let mut candidates = Vec::new();
            for attr in ["data-original", "src", "data-cdn", "data-src"] {
                if let Some(raw) = img.value().attr(attr)
                    && let Some(url) = abs_url(&base, raw)
                    && !candidates.contains(&url)
                {
                    candidates.push(url);
                }
            }
            let Some((primary, rest)) = candidates.split_first() else {
                continue;
            };
            if !seen.insert(primary.as_str().to_owned()) {
                continue;
            }
            images.push(PageImage {
                url: primary.clone(),
                fallbacks: rest.to_vec(),
                referer: page_url.clone(),
            });
        }

        if images.is_empty() {
            return Err(ScrapeError::NotRecognized {
                url: page_url.clone(),
                reason: "truyenqq chapter images not found",
            });
        }

        let prev = doc
            .select(&PREV_LINK_SEL)
            .find_map(|a| a.value().attr("href").and_then(|h| abs_url(&base, h)))
            .filter(|u| u != page_url && Self::is_chapter_url(u));

        let next = doc
            .select(&NEXT_LINK_SEL)
            .find_map(|a| a.value().attr("href").and_then(|h| abs_url(&base, h)))
            .filter(|u| u != page_url && Self::is_chapter_url(u));

        let manga_url = derive_manga_url(page_url)
            .or_else(|| extract_manga_url_from_chapter(&doc, page_url, &base));

        Ok(Chapter {
            url: page_url.clone(),
            name,
            images,
            prev,
            next,
            manga_url,
        })
    }
}

fn derive_manga_url(chapter_url: &Url) -> Option<Url> {
    let stripped = CHAP_URL_SUFFIX_RE.replace(chapter_url.path(), "");
    if stripped.len() < chapter_url.path().len() && stripped.len() > 1 {
        let mut url = chapter_url.clone();
        url.set_path(&stripped);
        url.set_query(None);
        Some(url)
    } else {
        None
    }
}

#[async_trait]
impl Source for TruyenQqSource {
    fn id(&self) -> &'static str {
        "truyenqq"
    }

    fn matches(&self, url: &Url) -> bool {
        url.host_str().is_some_and(|host| {
            let lower = host.to_ascii_lowercase();
            lower.starts_with("truyenqq") || lower.contains(".truyenqq")
        })
    }

    async fn resolve(&self, http: &HttpClient, url: &Url) -> Result<Resolved, ScrapeError> {
        let page = http.fetch_html(url).await?;

        if Self::is_chapter_url(&page.final_url) {
            let chapter = Self::parse_chapter_html(&page.html, &page.final_url)?;
            let chapter_ref = ChapterRef {
                url: chapter.url.clone(),
                name: chapter.name.clone(),
                number: extract_chapter_number(&chapter.name)
                    .or_else(|| extract_chapter_number(chapter.url.path())),
            };

            if let Some(manga_url) = chapter.manga_url.as_ref()
                && let Ok(manga_page) = http.fetch_html(manga_url).await
                && let Ok(manga) = Self::parse_manga_html(&manga_page.html, &manga_page.final_url)
            {
                let matched = manga
                    .chapters
                    .iter()
                    .find(|c| c.url == chapter.url)
                    .cloned()
                    .unwrap_or(chapter_ref);
                return Ok(Resolved::Chapter { manga, at: matched });
            }

            let fallback_manga = Manga {
                url: chapter.manga_url.unwrap_or_else(|| chapter.url.clone()),
                site: SiteId::known("truyenqq"),
                title: chapter.name.clone(),
                cover: None,
                chapters: vec![chapter_ref.clone()],
            };
            return Ok(Resolved::Chapter {
                manga: fallback_manga,
                at: chapter_ref,
            });
        }

        let manga = Self::parse_manga_html(&page.html, &page.final_url)?;
        Ok(Resolved::Manga(manga))
    }

    async fn chapter(&self, http: &HttpClient, url: &Url) -> Result<Chapter, ScrapeError> {
        let page = http.fetch_html(url).await?;
        Self::parse_chapter_html(&page.html, &page.final_url)
    }
}
