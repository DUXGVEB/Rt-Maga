//! Thin Tauri IPC command handlers delegating to [`crate::services`].

use std::sync::Arc;

use tauri::{AppHandle, Emitter, State};

use crate::cache::CacheInfo;
use crate::error::AppError;
use crate::services::library::{FetchMode, HistoryItemView, MangaView, ResolveResponse};
use crate::services::reader::ChapterView;
use crate::services::updater::{CheckScope, UpdateSummary};
use crate::services::{library, reader, updater};
use crate::settings::Settings;
use crate::state::AppState;

/// Resolves any pasted manga or chapter URL and persists the manga metadata locally.
#[tauri::command]
pub(crate) async fn resolve_url(
    state: State<'_, Arc<AppState>>,
    url: String,
) -> Result<ResolveResponse, AppError> {
    library::resolve_url(&state, &url).await
}

/// Loads a locally stored manga detail view by ID.
#[tauri::command]
pub(crate) async fn get_manga(
    state: State<'_, Arc<AppState>>,
    id: i64,
) -> Result<MangaView, AppError> {
    library::get_manga(&state, id, FetchMode::LocalOnly).await
}

/// Refreshes a stored manga's chapter list directly from its remote website.
#[tauri::command]
pub(crate) async fn refresh_manga(
    state: State<'_, Arc<AppState>>,
    id: i64,
) -> Result<MangaView, AppError> {
    library::get_manga(&state, id, FetchMode::RefreshFromWeb).await
}

/// Opens a visible browser window so the user can manually complete a site verification check.
#[tauri::command]
pub(crate) async fn open_manual_verification(
    state: State<'_, Arc<AppState>>,
    url: String,
) -> Result<(), AppError> {
    library::open_manual_verification(&state, &url).await
}

/// Fetches ordered image tokens and neighbor chapter links for a chapter URL.
#[tauri::command]
pub(crate) async fn get_chapter(
    state: State<'_, Arc<AppState>>,
    url: String,
    manga_id: Option<i64>,
) -> Result<ChapterView, AppError> {
    reader::get_chapter(&state, &url, manga_id).await
}

/// Saves the user's current reading position into `history`.
#[tauri::command]
pub(crate) async fn save_progress(
    state: State<'_, Arc<AppState>>,
    manga_id: i64,
    chapter_url: String,
    chapter_name: String,
    scroll_ratio: f64,
) -> Result<(), AppError> {
    library::save_progress(&state, manga_id, &chapter_url, &chapter_name, scroll_ratio).await
}

/// Marks a chapter as read in SQLite.
#[tauri::command]
pub(crate) async fn mark_read(
    state: State<'_, Arc<AppState>>,
    manga_id: i64,
    chapter_url: String,
) -> Result<(), AppError> {
    library::mark_read(&state, manga_id, &chapter_url).await
}

/// Lists all reading history items ordered most-recent first.
#[tauri::command]
pub(crate) async fn list_history(
    state: State<'_, Arc<AppState>>,
) -> Result<Vec<HistoryItemView>, AppError> {
    library::list_history(&state).await
}

/// Removes one manga from reading history and stops update checks for it.
#[tauri::command]
pub(crate) async fn remove_history(
    state: State<'_, Arc<AppState>>,
    manga_id: i64,
) -> Result<(), AppError> {
    library::remove_history(&state, manga_id).await
}

/// Clears the entire reading history.
#[tauri::command]
pub(crate) async fn clear_history(state: State<'_, Arc<AppState>>) -> Result<(), AppError> {
    library::clear_history(&state).await
}

/// Triggers an immediate new-chapter check for all manga currently in reading history.
#[tauri::command]
pub(crate) async fn check_updates_now(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
) -> Result<UpdateSummary, AppError> {
    updater::run_update_check(&app, &state, CheckScope::ManualNow).await
}

/// Returns the current application settings.
#[tauri::command]
pub(crate) async fn get_settings(state: State<'_, Arc<AppState>>) -> Result<Settings, AppError> {
    Ok(state.current_settings().await)
}

/// Persists updated settings, restarts the background updater if needed, and broadcasts the change.
#[tauri::command]
pub(crate) async fn set_settings(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    settings: Settings,
) -> Result<Settings, AppError> {
    let saved = state.db.save_settings(&settings).await?;
    {
        let mut guard = state.settings.write().await;
        *guard = saved.clone();
    }
    let _ = app.emit("settings://changed", &saved);
    state.images.evict_if_needed(saved.cache.max_mb).await;
    updater::restart_background_loop(app, Arc::clone(&state)).await;
    Ok(saved)
}

/// Returns current disk image cache usage statistics.
#[tauri::command]
pub(crate) async fn cache_info(state: State<'_, Arc<AppState>>) -> Result<CacheInfo, AppError> {
    state.images.info().await
}

/// Clears both the local disk image cache and the in-memory chapter cache.
#[tauri::command]
pub(crate) async fn clear_image_cache(
    state: State<'_, Arc<AppState>>,
) -> Result<CacheInfo, AppError> {
    state.images.clear().await?;
    state.chapters.invalidate_all();
    state.images.info().await
}
