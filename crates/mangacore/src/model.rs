//! Domain models shared across adapters, parsers, and the desktop backend.

use std::fmt;

use serde::{Deserialize, Serialize};
use url::Url;

/// Identifies which source produced a [`Manga`] or [`Chapter`].
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
#[non_exhaustive]
pub enum SiteId {
    /// A built-in site adapter such as `"truyenqq"`.
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

    /// Returns a compact storage representation such as `"truyenqq"` or `"generic:example.com"`.
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

/// Summary of a manga series and its chapter listing.
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
}

/// Lightweight chapter entry listed on a manga detail page.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChapterRef {
    /// Full URL of the chapter reading page.
    pub url: Url,
    /// Human-readable chapter label (e.g. `"Chương 45"`).
    pub name: String,
    /// Parsed chapter number when present (supports fractional chapters like `12.5`).
    pub number: Option<f32>,
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

/// Result of resolving an arbitrary user-supplied link.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum Resolved {
    /// The user pasted a manga detail page URL.
    Manga(Manga),
    /// The user pasted a direct chapter URL; includes the parent manga and target chapter.
    Chapter {
        /// Parent manga series containing this chapter.
        manga: Manga,
        /// The chapter referenced by the pasted link.
        at: ChapterRef,
    },
}

impl Resolved {
    /// Returns a reference to the resolved [`Manga`].
    pub fn manga(&self) -> &Manga {
        match self {
            Self::Manga(manga) | Self::Chapter { manga, .. } => manga,
        }
    }
}
