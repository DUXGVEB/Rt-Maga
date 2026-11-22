//! Manga resolution, detail loading, and reading history workflows.

use mangacore::{Resolved, normalize_url};
use serde::Serialize;

use crate::error::AppError;
use crate::image_proxy::encode_cover_token;
use crate::repo::history::HistoryEntryRow;
use crate::repo::manga::{ChapterRow, MangaRow};
use crate::state::AppState;

/// Controls whether loading a manga uses local DB data or refetches from the remote website.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FetchMode {
    /// Return locally stored metadata and chapters.
    LocalOnly,
    /// Re-scrape the manga page from the web and update SQLite.
    RefreshFromWeb,
}

/// Chapter entry sent to the Svelte UI.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ChapterRefView {
    pub url: String,
    pub name: String,
    pub number: Option<f64>,
    pub idx: i64,
    pub read: bool,
}

/// Reading progress snapshot attached to a manga view.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProgressView {
    pub chapter_url: String,
    pub chapter_name: String,
    pub scroll_ratio: f64,
    pub read_at: i64,
}

/// Full manga detail payload sent to the Svelte UI.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MangaView {
    pub id: i64,
    pub url: String,
    pub site: String,
    pub title: String,
    pub cover_token: Option<String>,
    pub has_new: bool,
    pub latest_chapter_name: Option<String>,
    pub chapters: Vec<ChapterRefView>,
    pub progress: Option<ProgressView>,
}

/// Response returned when the user pastes any manga or chapter URL.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ResolveResponse {
    pub manga: MangaView,
    pub start_at: Option<ChapterRefView>,
}

/// Reading history entry rendered on Home and History pages.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct HistoryItemView {
    pub manga_id: i64,
    pub manga_url: String,
    pub site: String,
    pub title: String,
    pub cover_token: Option<String>,
    pub chapter_url: String,
    pub chapter_name: String,
    pub scroll_ratio: f64,
    pub read_at: i64,
    pub latest_chapter_url: Option<String>,
    pub latest_chapter_name: Option<String>,
    pub has_new: bool,
    pub total_chapters: i64,
    pub current_chapter_idx: Option<i64>,
}

/// Resolves an arbitrary pasted link, persists the manga in SQLite, and returns the UI view.
///
/// # Errors
///
/// Returns [`AppError`] on invalid URL, network/scrape failure, or database error.
pub(crate) async fn resolve_url(
    state: &AppState,
    raw_url: &str,
) -> Result<ResolveResponse, AppError> {
    let policy = state.current_settings().await.headless_policy();
    let resolved = state.registry.resolve(raw_url, policy).await?;

    let (manga, start_chapter) = match resolved {
        Resolved::Manga(m) => (m, None),
        Resolved::Chapter { manga, at } => (manga, Some(at)),
        _ => {
            return Err(AppError::Internal {
                message: "unexpected resolve variant".to_owned(),
            });
        }
    };

    let manga_id = state.db.upsert_manga(&manga).await?;
    let view = build_manga_view(state, manga_id).await?;
    let start_at = start_chapter.map(|target| {
        let target_norm = target.url.as_str().trim_end_matches('/');
        view.chapters
            .iter()
            .find(|c| c.url.trim_end_matches('/') == target_norm)
            .cloned()
            .unwrap_or_else(|| ChapterRefView {
                url: target.url.into(),
                name: target.name,
                number: target.number.map(f64::from),
                idx: 0,
                read: false,
            })
    });

    Ok(ResolveResponse {
        manga: view,
        start_at,
    })
}

/// Loads a stored manga by ID, optionally refreshing its chapter list from the web first.
///
/// # Errors
///
/// Returns [`AppError::NotFound`] if the manga ID does not exist, or scraper/DB errors.
pub(crate) async fn get_manga(
    state: &AppState,
    manga_id: i64,
    mode: FetchMode,
) -> Result<MangaView, AppError> {
    let row = state
        .db
        .get_manga_by_id(manga_id)
        .await?
        .ok_or_else(|| AppError::NotFound {
            message: format!("manga {manga_id} not found"),
        })?;

    if mode == FetchMode::RefreshFromWeb {
        let policy = state.current_settings().await.headless_policy();
        let resolved = state.registry.resolve(&row.url, policy).await?;
        let _ = state
            .db
            .upsert_manga_with_id(Some(manga_id), resolved.manga())
            .await?;
        state.db.clear_has_new(manga_id).await?;
    }

    build_manga_view(state, manga_id).await
}

/// Launches a visible browser window so the user can manually complete a site verification check.
///
/// # Errors
///
/// Returns [`AppError`] if the URL is invalid or the browser fails to launch/complete.
pub(crate) async fn open_manual_verification(
    state: &AppState,
    raw_url: &str,
) -> Result<(), AppError> {
    let url = normalize_url(raw_url)?;
    state.registry.open_manual_verification(&url).await?;
    Ok(())
}

/// Persists the user's current reading position into `history`.
///
/// # Errors
///
/// Returns [`AppError::Db`] on SQLite failure.
pub(crate) async fn save_progress(
    state: &AppState,
    manga_id: i64,
    chapter_url: &str,
    chapter_name: &str,
    scroll_ratio: f64,
) -> Result<(), AppError> {
    state
        .db
        .save_history(manga_id, chapter_url, chapter_name, scroll_ratio)
        .await
}

/// Marks a chapter as read in SQLite.
///
/// # Errors
///
/// Returns [`AppError::Db`] on SQLite failure.
pub(crate) async fn mark_read(
    state: &AppState,
    manga_id: i64,
    chapter_url: &str,
) -> Result<(), AppError> {
    state.db.mark_chapter_read(manga_id, chapter_url).await
}

/// Lists all reading history entries with signed cover proxy tokens.
///
/// # Errors
///
/// Returns [`AppError::Db`] on SQLite failure.
pub(crate) async fn list_history(state: &AppState) -> Result<Vec<HistoryItemView>, AppError> {
    let rows = state.db.list_history().await?;
    Ok(rows
        .into_iter()
        .map(|row| map_history_row(row, &state.proxy_key))
        .collect())
}

/// Removes one manga from reading history.
///
/// # Errors
///
/// Returns [`AppError::Db`] on SQLite failure.
pub(crate) async fn remove_history(state: &AppState, manga_id: i64) -> Result<(), AppError> {
    state.db.remove_history(manga_id).await
}

/// Clears the entire reading history.
///
/// # Errors
///
/// Returns [`AppError::Db`] on SQLite failure.
pub(crate) async fn clear_history(state: &AppState) -> Result<(), AppError> {
    state.db.clear_history().await
}

async fn build_manga_view(state: &AppState, manga_id: i64) -> Result<MangaView, AppError> {
    let row = state
        .db
        .get_manga_by_id(manga_id)
        .await?
        .ok_or_else(|| AppError::NotFound {
            message: format!("manga {manga_id} not found"),
        })?;
    let chapters = state.db.list_chapters(manga_id).await?;
    let progress = state.db.get_history_for_manga(manga_id).await?;

    Ok(map_manga_view(row, chapters, progress, &state.proxy_key))
}

fn map_manga_view(
    row: MangaRow,
    chapters: Vec<ChapterRow>,
    progress: Option<(String, String, f64, i64)>,
    proxy_key: &[u8; 32],
) -> MangaView {
    let cover_token = row
        .cover_url
        .as_deref()
        .map(|c| encode_cover_token(c, &row.url, proxy_key));

    MangaView {
        id: row.id,
        url: row.url,
        site: row.site,
        title: row.title,
        cover_token,
        has_new: row.has_new != 0,
        latest_chapter_name: row.latest_chapter_name,
        chapters: chapters
            .into_iter()
            .map(|c| ChapterRefView {
                url: c.url,
                name: c.name,
                number: c.number,
                idx: c.idx,
                read: c.read != 0,
            })
            .collect(),
        progress: progress.map(
            |(chapter_url, chapter_name, scroll_ratio, read_at)| ProgressView {
                chapter_url,
                chapter_name,
                scroll_ratio,
                read_at,
            },
        ),
    }
}

fn map_history_row(row: HistoryEntryRow, proxy_key: &[u8; 32]) -> HistoryItemView {
    let cover_token = row
        .cover_url
        .as_deref()
        .map(|c| encode_cover_token(c, &row.manga_url, proxy_key));

    HistoryItemView {
        manga_id: row.manga_id,
        manga_url: row.manga_url,
        site: row.site,
        title: row.title,
        cover_token,
        chapter_url: row.chapter_url,
        chapter_name: row.chapter_name,
        scroll_ratio: row.scroll_ratio,
        read_at: row.read_at,
        latest_chapter_url: row.latest_chapter_url,
        latest_chapter_name: row.latest_chapter_name,
        has_new: row.has_new != 0,
        total_chapters: row.total_chapters,
        current_chapter_idx: row.current_chapter_idx,
    }
}
