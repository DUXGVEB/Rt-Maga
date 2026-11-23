//! Domain models shared across adapters, parsers, and the desktop backend.

use std::fmt;

use serde::{Deserialize, Serialize};
use url::Url;

use crate::recognize::UrlKind;

/// Identifies which source produced a [`Manga`], [`Chapter`], or [`CatalogPage`].
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
#[non_exhaustive]
pub enum SiteId {
    /// A registered custom site adapter identifier.
    Known(String),
    /// The generic heuristic parser, tagged with the normalized hostname.
    Generic(String),
}

impl SiteId {
    /// Creates a [`SiteId::Known`] identifier.
    pub fn known(name: impl Into<String>) -> Self {
        Self::Known(name.into())
    }

    /// Creates a [`SiteId::Generic`] identifier for `host`.
    pub fn generic(host: impl Into<String>) -> Self {
        Self::Generic(host.into())
    }

    /// Returns a compact storage representation such as `"<adapter>"` or `"generic:example.com"`.
    pub fn as_storage_key(&self) -> String {
        match self {
            Self::Known(id) => id.clone(),
            Self::Generic(host) => format!("generic:{host}"),
        }
    }
}

impl fmt::Display for SiteId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Known(id) => f.write_str(id),
            Self::Generic(host) => f.write_str(host),
        }
    }
}

/// Summary of a manga series, its metadata, and its chapter listing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Manga {
    /// Canonical URL of the manga detail page.
    pub url: Url,
    /// Source adapter that resolved this manga.
    pub site: SiteId,
    /// Display title of the series.
    pub title: String,
    /// Optional cover image URL.
    pub cover: Option<Url>,
    /// Chapters ordered from oldest (lowest number) to newest (highest number).
    pub chapters: Vec<ChapterRef>,
    /// Stable source-specific identifier (e.g. `"<adapter>:<id>"`).
    #[serde(default)]
    pub source_id: Option<String>,
    /// Canonical manga URL reported by the website or API when different from `url`.
    #[serde(default)]
    pub canonical_url: Option<Url>,
    /// Alternative or localized titles reported by the source.
    #[serde(default)]
    pub alt_titles: Vec<String>,
    /// Original-language title when explicitly provided.
    #[serde(default)]
    pub original_title: Option<String>,
    /// Author names when provided by the source.
    #[serde(default)]
    pub authors: Vec<String>,
    /// Artist names when provided by the source.
    #[serde(default)]
    pub artists: Vec<String>,
    /// Series synopsis or description when provided.
    #[serde(default)]
    pub description: Option<String>,
    /// Genre labels reported by the source.
    #[serde(default)]
    pub genres: Vec<String>,
    /// Additional content/theme tags reported by the source.
    #[serde(default)]
    pub tags: Vec<String>,
    /// Publication status (e.g. `"ongoing"`, `"completed"`, `"hiatus"`).
    #[serde(default)]
    pub status: Option<String>,
    /// Translation status when explicitly reported.
    #[serde(default)]
    pub translation_status: Option<String>,
    /// Primary content/translation language code (e.g. `"vi"`, `"en"`).
    #[serde(default)]
    pub language: Option<String>,
    /// Available translated chapter language codes (especially for multi-language APIs).
    #[serde(default)]
    pub available_languages: Vec<String>,
    /// Total reported chapter count when known.
    #[serde(default)]
    pub chapter_count: Option<u32>,
}

impl Manga {
    /// Constructs a [`Manga`] with core required fields and empty optional metadata.
    #[must_use]
    pub fn minimal(
        url: Url,
        site: SiteId,
        title: String,
        cover: Option<Url>,
        chapters: Vec<ChapterRef>,
    ) -> Self {
        let count = u32::try_from(chapters.len()).ok();
        Self {
            canonical_url: Some(url.clone()),
            url,
            site,
            title,
            cover,
            chapters,
            source_id: None,
            alt_titles: Vec::new(),
            original_title: None,
            authors: Vec::new(),
            artists: Vec::new(),
            description: None,
            genres: Vec::new(),
            tags: Vec::new(),
            status: None,
            translation_status: None,
            language: None,
            available_languages: Vec::new(),
            chapter_count: count,
        }
    }
}

/// Lightweight chapter entry listed on a manga detail page.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChapterRef {
    /// Full URL of the chapter reading page.
    pub url: Url,
    /// Human-readable chapter label (e.g. `"Chapter 45"`).
    pub name: String,
    /// Parsed chapter number when present (supports fractional chapters like `12.5`).
    pub number: Option<f32>,
    /// Optional volume label when provided (e.g. `"3"`).
    #[serde(default)]
    pub volume: Option<String>,
    /// Optional release date or timestamp string as reported by the source.
    #[serde(default)]
    pub published_at: Option<String>,
    /// Optional chapter translation language code (e.g. `"vi"`, `"en"`).
    #[serde(default)]
    pub language: Option<String>,
    /// Optional source-specific chapter identifier (e.g. an upstream chapter UUID or ID).
    #[serde(default)]
    pub source_id: Option<String>,
}

impl ChapterRef {
    /// Constructs a [`ChapterRef`] with core required fields and `None` for optional metadata.
    #[must_use]
    pub fn simple(url: Url, name: String, number: Option<f32>) -> Self {
        Self {
            url,
            name,
            number,
            volume: None,
            published_at: None,
            language: None,
            source_id: None,
        }
    }
}

/// A single image page within a chapter, including fallback mirrors and required `Referer`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PageImage {
    /// Primary image URL.
    pub url: Url,
    /// Backup mirror URLs (such as `data-cdn`) tried if `url` fails.
    pub fallbacks: Vec<Url>,
    /// `Referer` header value required by hotlink-protected CDNs.
    pub referer: Url,
}

/// Fully resolved reading chapter with ordered page images and neighbor navigation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Chapter {
    /// Canonical URL of this chapter page.
    pub url: Url,
    /// Human-readable chapter title.
    pub name: String,
    /// Ordered list of manga page images.
    pub images: Vec<PageImage>,
    /// URL of the previous chapter, if available.
    pub prev: Option<Url>,
    /// URL of the next chapter, if available.
    pub next: Option<Url>,
    /// URL of the parent manga detail page, if discovered.
    pub manga_url: Option<Url>,
}

/// A single manga entry extracted from a website homepage, catalog, category, or search page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogEntry {
    /// Normalized URL pointing to this manga's detail page.
    pub url: Url,
    /// Display title of the manga.
    pub title: String,
    /// Alternative titles if exposed in the listing.
    #[serde(default)]
    pub alt_titles: Vec<String>,
    /// Optional cover thumbnail URL.
    pub cover: Option<Url>,
    /// Optional latest chapter label displayed on the card.
    pub latest_chapter: Option<String>,
    /// Optional publication status label.
    pub status: Option<String>,
    /// Optional genre/tag labels shown on the listing card.
    #[serde(default)]
    pub genres: Vec<String>,
    /// Optional source-specific manga ID for duplicate prevention.
    pub source_id: Option<String>,
}

/// Discoverable navigation or category shortcut extracted from a website homepage or catalog.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NavLink {
    /// Human-readable label (e.g. `"Latest Updates"`, `"Action"`).
    pub label: String,
    /// Absolute URL of the category or listing.
    pub url: Url,
    /// Semantic type of the linked page.
    pub kind: UrlKind,
}

/// Pagination metadata extracted from a catalog or search page.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PaginationInfo {
    /// Current 1-based page number.
    pub current_page: u32,
    /// Total pages if reported by the site or API.
    pub total_pages: Option<u32>,
    /// URL of the previous page, if available.
    pub prev_url: Option<Url>,
    /// URL of the next page, if available.
    pub next_url: Option<Url>,
}

/// Resolved website homepage, catalog directory, category listing, or search results page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogPage {
    /// Canonical or final URL of the listing page.
    pub url: Url,
    /// Source adapter that produced this catalog.
    pub site: SiteId,
    /// Human-readable site or adapter display name.
    pub site_name: String,
    /// Title of this listing (e.g. `"Latest Manga"`, `"Search: dragon"`).
    pub title: String,
    /// Whether this listing came from a homepage, catalog, or search page.
    pub url_kind: UrlKind,
    /// Active search query, if this is a search results page.
    pub query: Option<String>,
    /// Optional URL template with `{query}` placeholder for in-app catalog search.
    pub search_url_template: Option<String>,
    /// Validated manga entries extracted from the page.
    pub entries: Vec<CatalogEntry>,
    /// Navigation/category links available on this site.
    pub categories: Vec<NavLink>,
    /// Pagination links and page indices.
    pub pagination: PaginationInfo,
}

/// Result of resolving an arbitrary user-supplied link.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum Resolved {
    /// The user submitted a manga detail page URL.
    Manga(Box<Manga>),
    /// The user submitted a direct chapter URL; includes the parent manga and target chapter.
    Chapter {
        /// Parent manga series containing this chapter.
        manga: Box<Manga>,
        /// The chapter referenced by the submitted link.
        at: Box<ChapterRef>,
    },
    /// The user submitted a website homepage, catalog, category, or search results URL.
    Catalog(Box<CatalogPage>),
}

impl Resolved {
    /// Returns a reference to the resolved [`Manga`], if this result represents a manga or chapter.
    pub fn as_manga(&self) -> Option<&Manga> {
        match self {
            Self::Manga(manga) | Self::Chapter { manga, .. } => Some(manga.as_ref()),
            Self::Catalog(_) => None,
        }
    }
}
