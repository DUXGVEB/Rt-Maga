//! Manga resolution, detail loading, and reading history workflows.

use mangacore::{Resolved, normalize_url};
use serde::Serialize;

use crate::error::AppError;
use crate::image_proxy::encode_cover_token;
use crate::repo::bookmark::{BookmarkEntryRow, BookmarkSort};
use crate::repo::diagnostics::NewDiagnosticLog;
use crate::repo::history::{HistoryEntryRow, HistoryProgressRow, LastReadingDestination};
use crate::repo::manga::{ChapterRow, HideAction, MangaRow};
use crate::repo::notification::NotificationRow;
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
    pub page_index: i64,
    pub read_at: i64,
}

/// Full manga detail payload sent to the Svelte UI.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
#[expect(
    clippy::struct_excessive_bools,
    reason = "JSON view DTO carrying independent UI status flags for Svelte"
)]
pub(crate) struct MangaView {
    pub id: i64,
    pub url: String,
    pub site: String,
    pub title: String,
    pub cover_token: Option<String>,
    pub has_new: bool,
    pub bookmarked: bool,
    pub hidden: bool,
    pub unread_count: i64,
    pub is_completed: bool,
    pub reader_mode_override: Option<String>,
    pub detected_webtoon: bool,
    pub latest_chapter_name: Option<String>,
    pub chapters: Vec<ChapterRefView>,
    pub progress: Option<ProgressView>,
}

/// Hidden manga record shown in Settings -> Hidden Manga Manager.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct HiddenMangaView {
    pub id: i64,
    pub url: String,
    pub site: String,
    pub title: String,
    pub cover_token: Option<String>,
    pub latest_chapter_name: Option<String>,
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
    pub page_index: i64,
    pub read_at: i64,
    pub latest_chapter_url: Option<String>,
    pub latest_chapter_name: Option<String>,
    pub has_new: bool,
    pub bookmarked: bool,
    pub total_chapters: i64,
    pub unread_count: i64,
    pub is_completed: bool,
    pub current_chapter_idx: Option<i64>,
}

/// Bookmarked manga entry rendered on the dedicated Bookmarks screen.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BookmarkItemView {
    pub manga_id: i64,
    pub manga_url: String,
    pub site: String,
    pub title: String,
    pub cover_token: Option<String>,
    pub bookmarked_at: i64,
    pub latest_chapter_url: Option<String>,
    pub latest_chapter_name: Option<String>,
    pub has_new: bool,
    pub total_chapters: i64,
    pub unread_count: i64,
    pub is_completed: bool,
    pub chapter_url: Option<String>,
    pub chapter_name: Option<String>,
    pub scroll_ratio: Option<f64>,
    pub page_index: Option<i64>,
    pub read_at: Option<i64>,
    pub current_chapter_idx: Option<i64>,
    pub reader_mode_override: Option<String>,
    pub detected_webtoon: bool,
}

/// Notification Center entry rendered in the in-app notification drawer.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NotificationItemView {
    pub id: i64,
    pub manga_id: i64,
    pub manga_title: String,
    pub cover_token: Option<String>,
    pub chapter_range: String,
    pub latest_chapter_url: String,
    pub chapter_count: i64,
    pub read: bool,
    pub created_at: i64,
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
    let resolved = match state.registry.resolve(raw_url, policy).await {
        Ok(r) => r,
        Err(err) => {
            let app_err = AppError::from(err);
            let _ = state
                .db
                .record_diagnostic_log(&NewDiagnosticLog {
                    level: "error".to_owned(),
                    category: app_err.category().to_owned(),
                    message: "Failed to resolve pasted manga/chapter URL".to_owned(),
                    detail: Some(app_err.to_string()),
                    source: None,
                    chapter_url: Some(raw_url.to_owned()),
                    stack_trace: None,
                })
                .await;
            return Err(app_err);
        }
    };

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
    page_index: i64,
) -> Result<(), AppError> {
    state
        .db
        .save_history(
            manga_id,
            chapter_url,
            chapter_name,
            scroll_ratio,
            page_index,
        )
        .await
}

/// Returns the persisted last reading destination used for History double-tap resume.
///
/// # Errors
///
/// Returns [`AppError::Db`] on SQLite failure.
pub(crate) async fn get_last_reading_destination(
    state: &AppState,
) -> Result<Option<LastReadingDestination>, AppError> {
    state.db.get_last_reading_destination().await
}

/// Toggles bookmark status for a manga and returns the updated state (`true` = bookmarked).
///
/// # Errors
///
/// Returns [`AppError::Db`] on SQLite failure.
pub(crate) async fn toggle_bookmark(state: &AppState, manga_id: i64) -> Result<bool, AppError> {
    state.db.toggle_bookmark(manga_id).await
}

/// Lists all bookmarked manga with signed cover proxy tokens.
///
/// # Errors
///
/// Returns [`AppError::Db`] on SQLite failure.
pub(crate) async fn list_bookmarks(
    state: &AppState,
    sort: BookmarkSort,
) -> Result<Vec<BookmarkItemView>, AppError> {
    let rows = state.db.list_bookmarks(sort).await?;
    Ok(rows
        .into_iter()
        .map(|row| map_bookmark_row(row, &state.proxy_key))
        .collect())
}

/// Sets or clears the per-manga manual reader mode override.
///
/// # Errors
///
/// Returns [`AppError::Db`] on SQLite failure.
pub(crate) async fn set_manga_reader_override(
    state: &AppState,
    manga_id: i64,
    override_mode: Option<&str>,
) -> Result<(), AppError> {
    state
        .db
        .set_manga_reader_override(manga_id, override_mode)
        .await
}

/// Records that passive Webtoon / Long-Strip detection matched this manga.
///
/// # Errors
///
/// Returns [`AppError::Db`] on SQLite failure.
pub(crate) async fn mark_manga_detected_webtoon(
    state: &AppState,
    manga_id: i64,
) -> Result<(), AppError> {
    state.db.mark_manga_detected_webtoon(manga_id).await
}

/// Hides or unhides a manga from normal library views.
///
/// # Errors
///
/// Returns [`AppError::Db`] on SQLite failure.
pub(crate) async fn set_manga_hidden(
    state: &AppState,
    manga_id: i64,
    action: HideAction,
) -> Result<bool, AppError> {
    state.db.set_manga_hidden(manga_id, action).await
}

/// Lists all hidden manga so they can be unhidden or deleted.
///
/// # Errors
///
/// Returns [`AppError::Db`] on SQLite failure.
pub(crate) async fn list_hidden_manga(state: &AppState) -> Result<Vec<HiddenMangaView>, AppError> {
    let rows = state.db.list_hidden_manga().await?;
    Ok(rows
        .into_iter()
        .map(|row| {
            let cover_token = row.cover_url.as_deref().map(|c| {
                encode_cover_token(c, &row.url, Some(row.id), Some(&row.site), &state.proxy_key)
            });
            HiddenMangaView {
                id: row.id,
                url: row.url,
                site: row.site,
                title: row.title,
                cover_token,
                latest_chapter_name: row.latest_chapter_name,
            }
        })
        .collect())
}

/// Permanently deletes a manga, its chapters, history, bookmarks, notifications, and cached images.
///
/// # Errors
///
/// Returns [`AppError::Db`] on SQLite failure.
pub(crate) async fn delete_manga_permanently(
    state: &AppState,
    manga_id: i64,
) -> Result<(), AppError> {
    let _ = state.images.clear_by_manga(manga_id).await;
    state.db.delete_manga_permanently(manga_id).await?;
    state.chapters.invalidate_all();
    Ok(())
}

/// Lists stored chapter notifications for the Notification Center.
///
/// # Errors
///
/// Returns [`AppError::Db`] on SQLite failure.
pub(crate) async fn list_notifications(
    state: &AppState,
) -> Result<Vec<NotificationItemView>, AppError> {
    let rows = state.db.list_notifications().await?;
    Ok(rows
        .into_iter()
        .map(|row| map_notification_row(row, &state.proxy_key))
        .collect())
}

/// Marks a single notification as read.
///
/// # Errors
///
/// Returns [`AppError::Db`] on SQLite failure.
pub(crate) async fn mark_notification_read(state: &AppState, id: i64) -> Result<(), AppError> {
    state.db.mark_notification_read(id).await
}

/// Marks all notifications as read.
///
/// # Errors
///
/// Returns [`AppError::Db`] on SQLite failure.
pub(crate) async fn mark_all_notifications_read(state: &AppState) -> Result<(), AppError> {
    state.db.mark_all_notifications_read().await
}

/// Deletes a single notification by ID.
///
/// # Errors
///
/// Returns [`AppError::Db`] on SQLite failure.
pub(crate) async fn delete_notification(state: &AppState, id: i64) -> Result<(), AppError> {
    state.db.delete_notification(id).await
}

/// Clears all notifications from the Notification Center.
///
/// # Errors
///
/// Returns [`AppError::Db`] on SQLite failure.
pub(crate) async fn clear_notifications(state: &AppState) -> Result<(), AppError> {
    state.db.clear_notifications().await
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
    let bookmarked = state.db.is_bookmarked(manga_id).await?;
    let (unread_count, is_completed) = state.db.get_manga_read_summary(manga_id).await?;

    Ok(map_manga_view(
        row,
        chapters,
        progress,
        bookmarked,
        unread_count,
        is_completed,
        &state.proxy_key,
    ))
}

#[expect(
    clippy::fn_params_excessive_bools,
    reason = "internal view mapper receiving boolean display flags"
)]
fn map_manga_view(
    row: MangaRow,
    chapters: Vec<ChapterRow>,
    progress: Option<HistoryProgressRow>,
    bookmarked: bool,
    unread_count: i64,
    is_completed: bool,
    proxy_key: &[u8; 32],
) -> MangaView {
    let cover_token = row
        .cover_url
        .as_deref()
        .map(|c| encode_cover_token(c, &row.url, Some(row.id), Some(&row.site), proxy_key));

    MangaView {
        id: row.id,
        url: row.url,
        site: row.site,
        title: row.title,
        cover_token,
        has_new: row.has_new != 0,
        bookmarked,
        hidden: row.hidden != 0,
        unread_count,
        is_completed,
        reader_mode_override: row.reader_mode_override,
        detected_webtoon: row.detected_webtoon != 0,
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
        progress: progress.map(|p| ProgressView {
            chapter_url: p.chapter_url,
            chapter_name: p.chapter_name,
            scroll_ratio: p.scroll_ratio,
            page_index: p.page_index,
            read_at: p.read_at,
        }),
    }
}

fn map_history_row(row: HistoryEntryRow, proxy_key: &[u8; 32]) -> HistoryItemView {
    let cover_token = row.cover_url.as_deref().map(|c| {
        encode_cover_token(
            c,
            &row.manga_url,
            Some(row.manga_id),
            Some(&row.site),
            proxy_key,
        )
    });

    HistoryItemView {
        manga_id: row.manga_id,
        manga_url: row.manga_url,
        site: row.site,
        title: row.title,
        cover_token,
        chapter_url: row.chapter_url,
        chapter_name: row.chapter_name,
        scroll_ratio: row.scroll_ratio,
        page_index: row.page_index,
        read_at: row.read_at,
        latest_chapter_url: row.latest_chapter_url,
        latest_chapter_name: row.latest_chapter_name,
        has_new: row.has_new != 0,
        bookmarked: row.bookmarked != 0,
        total_chapters: row.total_chapters,
        unread_count: row.unread_count,
        is_completed: row.is_completed != 0,
        current_chapter_idx: row.current_chapter_idx,
    }
}

fn map_bookmark_row(row: BookmarkEntryRow, proxy_key: &[u8; 32]) -> BookmarkItemView {
    let cover_token = row.cover_url.as_deref().map(|c| {
        encode_cover_token(
            c,
            &row.manga_url,
            Some(row.manga_id),
            Some(&row.site),
            proxy_key,
        )
    });

    BookmarkItemView {
        manga_id: row.manga_id,
        manga_url: row.manga_url,
        site: row.site,
        title: row.title,
        cover_token,
        bookmarked_at: row.bookmarked_at,
        latest_chapter_url: row.latest_chapter_url,
        latest_chapter_name: row.latest_chapter_name,
        has_new: row.has_new != 0,
        total_chapters: row.total_chapters,
        unread_count: row.unread_count,
        is_completed: row.is_completed != 0,
        chapter_url: row.chapter_url,
        chapter_name: row.chapter_name,
        scroll_ratio: row.scroll_ratio,
        page_index: row.page_index,
        read_at: row.read_at,
        current_chapter_idx: row.current_chapter_idx,
        reader_mode_override: row.reader_mode_override,
        detected_webtoon: row.detected_webtoon != 0,
    }
}

fn map_notification_row(row: NotificationRow, proxy_key: &[u8; 32]) -> NotificationItemView {
    let cover_token = row
        .cover_url
        .as_deref()
        .map(|c| encode_cover_token(c, &row.manga_url, Some(row.manga_id), None, proxy_key));

    NotificationItemView {
        id: row.id,
        manga_id: row.manga_id,
        manga_title: row.manga_title,
        cover_token,
        chapter_range: row.chapter_range,
        latest_chapter_url: row.latest_chapter_url,
        chapter_count: row.chapter_count,
        read: row.read != 0,
        created_at: row.created_at,
    }
}
