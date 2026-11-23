//! Chapter loading, neighbor-chapter fallback from SQLite, and image token signing.

use std::sync::Arc;

use mangacore::normalize_url;
use serde::Serialize;

use crate::error::AppError;
use crate::image_proxy::encode_page_token;
use crate::state::AppState;

/// Resolved chapter payload delivered to the Svelte vertical reader.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ChapterView {
    pub url: String,
    pub name: String,
    pub images: Vec<String>,
    pub prev: Option<String>,
    pub next: Option<String>,
    pub manga_id: Option<i64>,
}

/// Fetches a chapter's ordered image tokens and neighbor URLs, using the in-memory chapter cache
/// and filling any missing `prev`/`next` links from the stored manga chapter list.
///
/// # Errors
///
/// Returns [`AppError`] on invalid URL, scraping failure, or database error.
pub(crate) async fn get_chapter(
    state: &AppState,
    raw_url: &str,
    known_manga_id: Option<i64>,
) -> Result<ChapterView, AppError> {
    let url = normalize_url(raw_url)?;

    let chapter = if let Some(cached) = state.chapters.get(&url).await {
        cached
    } else {
        let policy = state.current_settings().await.headless_policy();
        let fetched = Arc::new(state.registry.chapter(&url, policy).await?);
        state
            .chapters
            .insert(url.clone(), Arc::clone(&fetched))
            .await;
        fetched
    };

    let manga_id = match known_manga_id {
        Some(id) => Some(id),
        None => {
            state
                .db
                .find_manga_id_by_chapter_url(chapter.url.as_str())
                .await?
        }
    };

    let mut prev = chapter.prev.as_ref().map(|u| u.as_str().to_owned());
    let mut next = chapter.next.as_ref().map(|u| u.as_str().to_owned());
    let mut display_name = chapter.name.clone();

    // Fill missing prev/next and canonical chapter name from SQLite chapter list.
    if let Some(id) = manga_id {
        let chapters = state.db.list_chapters(id).await?;
        let target_norm = chapter.url.as_str().trim_end_matches('/');
        if let Some(pos) = chapters
            .iter()
            .position(|c| c.url.trim_end_matches('/') == target_norm)
        {
            display_name.clone_from(&chapters[pos].name);
            if prev.is_none() && pos > 0 {
                prev = Some(chapters[pos - 1].url.clone());
            }
            if next.is_none() && pos + 1 < chapters.len() {
                next = Some(chapters[pos + 1].url.clone());
            }
        }
    }

    let site_for_cache = if let Some(id) = manga_id {
        state.db.get_manga_by_id(id).await?.map(|m| m.site)
    } else {
        chapter
            .url
            .host_str()
            .map(|h| h.trim_start_matches("www.").to_ascii_lowercase())
    };

    let images = chapter
        .images
        .iter()
        .map(|img| encode_page_token(img, manga_id, site_for_cache.as_deref(), &state.proxy_key))
        .collect();

    Ok(ChapterView {
        url: chapter.url.as_str().to_owned(),
        name: display_name,
        images,
        prev,
        next,
        manga_id,
    })
}
