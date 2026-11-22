//! Generic heuristic adapter that works across arbitrary manga websites.

use async_trait::async_trait;
use scraper::Html;
use url::Url;

use crate::error::ScrapeError;
use crate::http::HttpClient;
use crate::model::{Chapter, ChapterRef, Manga, Resolved, SiteId};
use crate::parse::chapters::{extract_chapter_number, extract_chapters, format_chapter_num};
use crate::parse::classify::{MIN_CHAPTER_IMAGES, PageKind, classify_page};
use crate::parse::dom::effective_base_url;
use crate::parse::images::extract_chapter_images;
use crate::parse::meta::{extract_cover, extract_title};
use crate::parse::nav::{extract_manga_url_from_chapter, extract_prev_next};
use crate::source::Source;

/// Generic heuristic manga source for websites without a dedicated adapter.
#[derive(Debug, Default, Clone, Copy)]
pub struct GenericSource;

impl GenericSource {
    /// Parses a manga overview page using generic heuristics.
    ///
    /// # Errors
    ///
    /// Returns [`ScrapeError::NotRecognized`] if no chapter list can be discovered.
    pub fn parse_manga_html(html: &str, page_url: &Url) -> Result<Manga, ScrapeError> {
        let doc = Html::parse_document(html);
        let base = effective_base_url(&doc, page_url);

        let chapters = extract_chapters(&doc, &base);
        if chapters.is_empty() {
            return Err(ScrapeError::NotRecognized {
                url: page_url.clone(),
                reason: "generic parser found no chapter list",
            });
        }

        let host = page_url.host_str().unwrap_or("unknown").to_owned();
        let title = extract_title(&doc).unwrap_or_else(|| host.clone());
        let cover = extract_cover(&doc, &base);

        Ok(Manga {
            url: page_url.clone(),
            site: SiteId::generic(host),
            title,
            cover,
            chapters,
        })
    }

    /// Parses a chapter reading page using generic image scoring and navigation heuristics.
    ///
    /// # Errors
    ///
    /// Returns [`ScrapeError::NotRecognized`] if fewer than [`MIN_CHAPTER_IMAGES`] images are found.
    pub fn parse_chapter_html(html: &str, page_url: &Url) -> Result<Chapter, ScrapeError> {
        let doc = Html::parse_document(html);
        let base = effective_base_url(&doc, page_url);

        let images = extract_chapter_images(&doc, page_url, &base);
        if images.len() < MIN_CHAPTER_IMAGES {
            return Err(ScrapeError::NotRecognized {
                url: page_url.clone(),
                reason: "generic parser found too few chapter images",
            });
        }

        let nav = extract_prev_next(&doc, page_url, &base);
        let manga_url = extract_manga_url_from_chapter(&doc, page_url, &base);
        let name = extract_title(&doc).unwrap_or_else(|| {
            extract_chapter_number(page_url.path()).map_or_else(
                || "Chapter".to_owned(),
                |n| format!("Chapter {}", format_chapter_num(n)),
            )
        });

        Ok(Chapter {
            url: page_url.clone(),
            name,
            images,
            prev: nav.prev,
            next: nav.next,
            manga_url,
        })
    }

    /// Classifies and resolves already-fetched HTML into a [`Resolved`] result.
    ///
    /// When the page is a chapter page and a parent manga URL is found, fetches the parent page
    /// through `http` to assemble the full chapter list.
    ///
    /// # Errors
    ///
    /// Returns [`ScrapeError::NotRecognized`] when the HTML does not resemble a manga or chapter page.
    pub async fn resolve_html(
        http: &HttpClient,
        html: &str,
        page_url: &Url,
    ) -> Result<Resolved, ScrapeError> {
        let kind = {
            let doc = Html::parse_document(html);
            classify_page(&doc, page_url)
        };

        match kind {
            PageKind::Chapter => {
                let chapter = Self::parse_chapter_html(html, page_url)?;
                let chapter_ref = ChapterRef {
                    url: chapter.url.clone(),
                    name: chapter.name.clone(),
                    number: extract_chapter_number(&chapter.name)
                        .or_else(|| extract_chapter_number(chapter.url.path())),
                };

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
                    return Ok(Resolved::Chapter { manga, at: matched });
                }

                let host = page_url.host_str().unwrap_or("unknown").to_owned();
                let fallback_manga = Manga {
                    url: chapter.manga_url.unwrap_or_else(|| chapter.url.clone()),
                    site: SiteId::generic(host),
                    title: chapter.name.clone(),
                    cover: None,
                    chapters: vec![chapter_ref.clone()],
                };
                Ok(Resolved::Chapter {
                    manga: fallback_manga,
                    at: chapter_ref,
                })
            }
            PageKind::Manga => {
                let manga = Self::parse_manga_html(html, page_url)?;
                Ok(Resolved::Manga(manga))
            }
            PageKind::Unknown => Err(ScrapeError::NotRecognized {
                url: page_url.clone(),
                reason: "page has neither chapter images nor chapter list",
            }),
        }
    }
}

#[async_trait]
impl Source for GenericSource {
    fn id(&self) -> &'static str {
        "generic"
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
