//! Core manga scraping, parsing, and URL resolution library for `RtMaga`.
//!
//! `mangacore` has no dependency on Tauri or SQLite. Its HTML parsing heuristics live in [`parse`]
//! as pure functions that operate on HTML strings and URLs, making them fast to test offline
//! against saved fixtures.

#![warn(missing_docs)]

pub mod error;
pub mod http;
pub mod model;
pub mod parse;
pub mod registry;
pub mod source;

pub use error::ScrapeError;
pub use http::{FetchedBytes, FetchedPage, HttpClient, HttpPolicy, normalize_url};
pub use model::{Chapter, ChapterRef, Manga, PageImage, Resolved, SiteId};
pub use registry::{HeadlessPolicy, Registry};
pub use source::Source;
