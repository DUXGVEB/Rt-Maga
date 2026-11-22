//! Thin Tauri IPC command handlers delegating to [`crate::services`].

use std::sync::Arc;

use mangacore::{ProxyEndpoint, ProxyTestResult};
use tauri::{AppHandle, Emitter, State};
use tauri_plugin_opener::OpenerExt;

use crate::cache::CacheInfo;
use crate::error::AppError;
use crate::services::app_release::{AppReleaseInfo, CURRENT_APP_VERSION, InstallUpdateOutcome};
use crate::services::library::{FetchMode, HistoryItemView, MangaView, ResolveResponse};
use crate::services::reader::ChapterView;
use crate::services::sync::{SyncDecision, SyncSenderMode, SyncStatusView};
use crate::services::updater::{CheckScope, UpdateSummary};
use crate::services::{app_release, library, reader, sync, updater};
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

/// Persists updated settings, restarts the background updater only when schedule changes,
/// and broadcasts the change.
#[tauri::command]
pub(crate) async fn set_settings(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    settings: Settings,
) -> Result<Settings, AppError> {
    let saved = state.db.save_settings(&settings).await?;
    let (schedule_changed, proxy_changed, conn_changed) = {
        let mut guard = state.settings.write().await;
        let sched = guard.updates.enabled != saved.updates.enabled
            || guard.updates.interval_hours != saved.updates.interval_hours;
        let prox = guard.proxy != saved.proxy;
        let conn = guard.sources.max_conn_per_host != saved.sources.max_conn_per_host;
        *guard = saved.clone();
        (sched, prox, conn)
    };
    if proxy_changed {
        state.registry.http().set_proxy_config(saved.proxy.clone());
    }
    if conn_changed {
        state
            .registry
            .http()
            .set_max_conn_per_host(usize::from(saved.sources.max_conn_per_host));
    }
    let _ = app.emit("settings://changed", &saved);
    state.images.evict_if_needed(saved.cache.max_mb).await;
    if schedule_changed {
        updater::restart_background_loop(app, Arc::clone(&state)).await;
    }
    Ok(saved)
}

/// Returns the current runtime platform (`"windows"`, `"android"`, or `"other"`).
#[tauri::command]
pub(crate) fn get_platform() -> &'static str {
    if cfg!(target_os = "android") {
        "android"
    } else if cfg!(target_os = "windows") {
        "windows"
    } else {
        "other"
    }
}

/// Tests an in-app proxy endpoint and returns connectivity status and latency in milliseconds.
#[tauri::command]
pub(crate) async fn test_proxy(
    state: State<'_, Arc<AppState>>,
    endpoint: ProxyEndpoint,
    test_url: Option<String>,
) -> Result<ProxyTestResult, AppError> {
    state
        .registry
        .http()
        .test_proxy(&endpoint, test_url.as_deref())
        .await
        .map_err(AppError::from)
}

/// Returns the current serverless LAN sync status snapshot.
#[tauri::command]
pub(crate) async fn sync_get_status(
    state: State<'_, Arc<AppState>>,
) -> Result<SyncStatusView, AppError> {
    Ok(state.sync.status().await)
}

/// Starts a 5-minute LAN sync sender session (`pc_qr` or `phone_pin`).
#[tauri::command]
pub(crate) async fn sync_start_sender(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    mode: SyncSenderMode,
) -> Result<SyncStatusView, AppError> {
    sync::start_sender(&app, &state, mode).await
}

/// Cancels any active LAN sync session and scrubs session tokens & buffers from memory.
#[tauri::command]
pub(crate) async fn sync_cancel(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
) -> Result<SyncStatusView, AppError> {
    Ok(state.sync.cancel(&app).await)
}

/// Approves or declines a pending peer sync request on the Sender device.
#[tauri::command]
pub(crate) async fn sync_host_respond(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    decision: SyncDecision,
) -> Result<SyncStatusView, AppError> {
    sync::host_respond(&app, &state, decision).await
}

/// Connects to a PC QR sender session over LAN using a scanned QR string.
#[tauri::command]
pub(crate) async fn sync_connect_qr(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    qr_payload: String,
) -> Result<SyncStatusView, AppError> {
    sync::connect_via_qr(&app, &state, &qr_payload).await
}

/// Connects to a Phone Pairing Code sender over LAN (UDP discovery or optional manual `IP:Port`).
#[tauri::command]
pub(crate) async fn sync_connect_pin(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    pin: String,
    manual_addr: Option<String>,
) -> Result<SyncStatusView, AppError> {
    sync::connect_via_pin(&app, &state, &pin, manual_addr.as_deref()).await
}

/// Approves or declines a paired LAN sync transfer on the Receiver device.
#[tauri::command]
pub(crate) async fn sync_receiver_confirm(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    decision: SyncDecision,
) -> Result<SyncStatusView, AppError> {
    sync::receiver_confirm(&app, &state, decision).await
}

/// Returns the compiled `RtMaga` application version string.
#[tauri::command]
pub(crate) fn get_app_version() -> &'static str {
    CURRENT_APP_VERSION
}

/// Returns the most recently cached application release check result, if any.
#[tauri::command]
pub(crate) async fn get_cached_app_release(
    state: State<'_, Arc<AppState>>,
) -> Result<Option<AppReleaseInfo>, AppError> {
    Ok(state.latest_release.read().await.clone())
}

/// Queries the official release endpoint now to detect whether a newer `RtMaga` version exists.
#[tauri::command]
pub(crate) async fn check_app_release(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
) -> Result<AppReleaseInfo, AppError> {
    let info = app_release::check_latest_release(&state).await?;
    if info.update_available {
        let _ = app.emit("app://release-available", &info);
    }
    Ok(info)
}

/// Opens a verified official release or installer URL in the user's system browser.
#[tauri::command]
pub(crate) async fn open_release_url(app: AppHandle, url: String) -> Result<(), AppError> {
    if !app_release::is_allowed_release_url(&url) {
        return Err(AppError::InvalidUrl {
            message: "only official release links may be opened".to_owned(),
        });
    }
    app.opener()
        .open_url(url, None::<&str>)
        .map_err(|e| AppError::Internal {
            message: format!("failed to open release link: {e}"),
        })
}

/// Downloads the verified platform installer, backs up user data, and launches the update.
#[tauri::command]
pub(crate) async fn download_and_install_update(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
) -> Result<InstallUpdateOutcome, AppError> {
    app_release::download_and_install_update(&app, &state).await
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
