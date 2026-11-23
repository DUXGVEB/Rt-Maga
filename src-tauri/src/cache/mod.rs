//! In-memory chapter cache and disk-backed LRU image cache.

pub(crate) mod chapter;
pub(crate) mod image;

pub(crate) use chapter::ChapterCache;
pub(crate) use image::{CacheInfo, CacheMangaStats, CacheSourceStats, ImageCache};
