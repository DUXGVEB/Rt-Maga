//! Manga resolution, detail loading, and reading history workflows.

use mangacore::{CatalogPage, RecognitionDiagnostic, Resolved, UrlKind, normalize_url};
use serde::Serialize;

use crate::error::AppError;
use crate::image_proxy::encode_cover_token;
use crate::repo::bookmark::{BookmarkEntryRow, BookmarkSort};
use crate::repo::diagnostics::NewDiagnosticLog;
use crate::repo::history::{HistoryEntryRow, HistoryProgressRow, LastReadingDestination};
use crate::repo::manga::{ChapterRow, HideAction, MangaRow};
use crate::repo::notification::NotificationRow;
use crate::settings::ReaderSettings;
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
    pub volume: Option<String>,
    pub published_at: Option<String>,
    pub language: Option<String>,
    pub source_id: Option<String>,
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
    pub reader_settings: Option<ReaderSettings>,
    pub detected_webtoon: bool,
    pub latest_chapter_name: Option<String>,
    pub source_id: Option<String>,
    pub canonical_url: Option<String>,
    pub alt_titles: Vec<String>,
    pub original_title: Option<String>,
    pub authors: Vec<String>,
    pub artists: Vec<String>,
    pub description: Option<String>,
    pub genres: Vec<String>,
    pub tags: Vec<String>,
    pub status: Option<String>,
    pub language: Option<String>,
    pub available_languages: Vec<String>,
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

/// Single manga entry extracted from a website homepage, catalog, category, or search page.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CatalogEntryView {
    pub url: String,
    pub title: String,
    pub alt_titles: Vec<String>,
    pub cover_token: Option<String>,
    pub latest_chapter: Option<String>,
    pub status: Option<String>,
    pub genres: Vec<String>,
    pub source_id: Option<String>,
}

/// Category or navigation shortcut extracted from a website catalog/homepage.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NavLinkView {
    pub label: String,
    pub url: String,
    pub kind: UrlKind,
}

/// Pagination metadata for in-app catalog browsing.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PaginationView {
    pub current_page: u32,
    pub total_pages: Option<u32>,
    pub prev_url: Option<String>,
    pub next_url: Option<String>,
}

/// Resolved catalog/homepage/search view sent to the Svelte Browse page.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CatalogPageView {
    pub url: String,
    pub site: String,
    pub site_name: String,
    pub title: String,
    pub url_kind: UrlKind,
    pub query: Option<String>,
    pub search_url_template: Option<String>,
    pub entries: Vec<CatalogEntryView>,
    pub categories: Vec<NavLinkView>,
    pub pagination: PaginationView,
}

/// Response returned when the user submits any homepage, catalog, search, manga, or chapter URL.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ResolveResponse {
    pub kind: String,
    pub manga: Option<MangaView>,
    pub start_at: Option<ChapterRefView>,
    pub catalog: Option<CatalogPageView>,
    pub diagnostic: RecognitionDiagnostic,
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
    let (resolved, diagnostic) = match state
        .registry
        .resolve_with_diagnostics(raw_url, policy)
        .await
    {
        Ok(pair) => pair,
        Err(boxed) => {
            let (err, diag) = *boxed;
            let report_json = diag
                .report
                .as_ref()
                .and_then(|r| serde_json::to_string_pretty(r).ok())
                .unwrap_or_else(|| diag.detail.clone());
            let app_err = match diag.report.clone() {
                Some(report) => AppError::from_scrape_and_report(&err, report),
                None => AppError::from(err),
            };
            let _ = state
                .db
                .record_diagnostic_log(&NewDiagnosticLog {
                    level: "error".to_owned(),
                    category: app_err.category().to_owned(),
                    message: format!("Failed to resolve URL ({:?})", diag.status),
                    detail: Some(report_json),
                    source: diag.website.clone(),
                    chapter_url: Some(diag.submitted_url.clone()),
                    failed_url: Some(diag.submitted_url.clone()),
                    resolved_url: diag.resolved_url.clone(),
                    http_status: diag.http_status.map(i64::from),
                    adapter: diag.adapter.clone(),
                    parsing_stage: Some(diag.stage.clone()),
                    api_error: matches!(app_err, AppError::ApiFailure { .. })
                        .then(|| app_err.to_string()),
                    stack_trace: None,
                })
                .await;
            return Err(app_err);
        }
    };

    match resolved {
        Resolved::Catalog(cat) => Ok(ResolveResponse {
            kind: "catalog".to_owned(),
            manga: None,
            start_at: None,
            catalog: Some(map_catalog_view(*cat, &state.proxy_key)),
            diagnostic,
        }),
        Resolved::Manga(manga) => {
            let manga_id = state.db.upsert_manga(&manga).await?;
            let view = build_manga_view(state, manga_id).await?;
            Ok(ResolveResponse {
                kind: "manga".to_owned(),
                manga: Some(view),
                start_at: None,
                catalog: None,
                diagnostic,
            })
        }
        Resolved::Chapter { manga, at } => {
            let manga_id = state.db.upsert_manga(&manga).await?;
            let view = build_manga_view(state, manga_id).await?;
            let target_norm = at.url.as_str().trim_end_matches('/');
            let matched = view
                .chapters
                .iter()
                .find(|c| c.url.trim_end_matches('/') == target_norm)
                .cloned()
                .unwrap_or_else(|| ChapterRefView {
                    url: at.url.into(),
                    name: at.name,
                    number: at.number.map(f64::from),
                    idx: 0,
                    read: false,
                    volume: at.volume,
                    published_at: at.published_at,
                    language: at.language,
                    source_id: at.source_id,
                });
            Ok(ResolveResponse {
                kind: "chapter".to_owned(),
                manga: Some(view),
                start_at: Some(matched),
                catalog: None,
                diagnostic,
            })
        }
        _ => Err(AppError::Internal {
            message: "unexpected resolve variant".to_owned(),
        }),
    }
}

fn map_catalog_view(cat: CatalogPage, proxy_key: &[u8; 32]) -> CatalogPageView {
    let site_key = cat.site.as_storage_key();
    let referer = cat.url.as_str();
    let entries =
        cat.entries
            .into_iter()
            .map(|entry| {
                let cover_token = entry.cover.as_ref().map(|c| {
                    encode_cover_token(c.as_str(), referer, None, Some(&site_key), proxy_key)
                });
                CatalogEntryView {
                    url: entry.url.into(),
                    title: entry.title,
                    alt_titles: entry.alt_titles,
                    cover_token,
                    latest_chapter: entry.latest_chapter,
                    status: entry.status,
                    genres: entry.genres,
                    source_id: entry.source_id,
                }
            })
            .collect();

    let categories = cat
        .categories
        .into_iter()
        .map(|nav| NavLinkView {
            label: nav.label,
            url: nav.url.into(),
            kind: nav.kind,
        })
        .collect();

    CatalogPageView {
        url: cat.url.into(),
        site: site_key,
        site_name: cat.site_name,
        title: cat.title,
        url_kind: cat.url_kind,
        query: cat.query,
        search_url_template: cat.search_url_template,
        entries,
        categories,
        pagination: PaginationView {
            current_page: cat.pagination.current_page,
            total_pages: cat.pagination.total_pages,
            prev_url: cat.pagination.prev_url.map(Into::into),
            next_url: cat.pagination.next_url.map(Into::into),
        },
    }
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
        let (resolved, _diag) = match state
            .registry
            .resolve_with_diagnostics(&row.url, policy)
            .await
        {
            Ok(pair) => pair,
            Err(boxed) => {
                let (err, diag) = *boxed;
                let report_json = diag
                    .report
                    .as_ref()
                    .and_then(|r| serde_json::to_string_pretty(r).ok())
                    .unwrap_or_else(|| diag.detail.clone());
                let app_err = match diag.report.clone() {
                    Some(report) => AppError::from_scrape_and_report(&err, report),
                    None => AppError::from(err),
                };
                let _ = state
                    .db
                    .record_diagnostic_log(&NewDiagnosticLog {
                        level: "error".to_owned(),
                        category: app_err.category().to_owned(),
                        message: format!("Failed to refresh chapter list for '{}'", row.title),
                        detail: Some(report_json),
                        source: Some(row.site.clone()),
                        chapter_url: Some(diag.submitted_url.clone()),
                        failed_url: Some(diag.submitted_url.clone()),
                        resolved_url: diag.resolved_url.clone(),
                        http_status: diag.http_status.map(i64::from),
                        adapter: diag.adapter.clone(),
                        parsing_stage: Some(diag.stage.clone()),
                        ..NewDiagnosticLog::default()
                    })
                    .await;
                return Err(app_err);
            }
        };
        if let Some(refreshed_manga) = resolved.as_manga() {
            let _ = state
                .db
                .upsert_manga_with_id(Some(manga_id), refreshed_manga)
                .await?;
            state.db.clear_has_new(manga_id).await?;
        }
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

/// Persists or clears a manga's full per-title [`ReaderSettings`] configuration.
///
/// # Errors
///
/// Returns [`AppError::Db`] or [`AppError::Internal`] on serialization/SQLite failure.
pub(crate) async fn set_manga_reader_settings(
    state: &AppState,
    manga_id: i64,
    reader_settings: Option<ReaderSettings>,
) -> Result<Option<ReaderSettings>, AppError> {
    let clean = reader_settings.map(ReaderSettings::sanitized);
    let json = match clean.as_ref() {
        Some(rs) => Some(serde_json::to_string(rs).map_err(|e| AppError::Internal {
            message: format!("failed to serialize per-manga reader settings: {e}"),
        })?),
        None => None,
    };
    state
        .db
        .set_manga_reader_settings(manga_id, json.as_deref())
        .await?;
    Ok(clean)
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

fn parse_json_vec(raw: Option<String>) -> Vec<String> {
    raw.and_then(|s| serde_json::from_str::<Vec<String>>(&s).ok())
        .unwrap_or_default()
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

    let reader_settings = row
        .reader_settings_json
        .as_deref()
        .and_then(|raw| serde_json::from_str::<ReaderSettings>(raw).ok())
        .map(ReaderSettings::sanitized);

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
        reader_settings,
        detected_webtoon: row.detected_webtoon != 0,
        latest_chapter_name: row.latest_chapter_name,
        source_id: row.source_id,
        canonical_url: row.canonical_url,
        alt_titles: parse_json_vec(row.alt_titles_json),
        original_title: row.original_title,
        authors: parse_json_vec(row.authors_json),
        artists: parse_json_vec(row.artists_json),
        description: row.description,
        genres: parse_json_vec(row.genres_json),
        tags: parse_json_vec(row.tags_json),
        status: row.status,
        language: row.language,
        available_languages: parse_json_vec(row.available_languages_json),
        chapters: chapters
            .into_iter()
            .map(|c| ChapterRefView {
                url: c.url,
                name: c.name,
                number: c.number,
                idx: c.idx,
                read: c.read != 0,
                volume: c.volume,
                published_at: c.published_at,
                language: c.language,
                source_id: c.source_id,
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
