//! Thin Tauri IPC command handlers delegating to [`crate::services`].

use std::sync::Arc;

use mangacore::{
    DohConfig, DohTestResult, NetworkProviderInfo, ProxyEndpoint, ProxyTestResult,
    network_provider_catalog, test_doh_config,
};
use tauri::{AppHandle, Emitter, State};
use tauri_plugin_opener::OpenerExt;

use crate::android_env::{DeviceEnvInfo, NotificationPermissionInfo, OrientationLockReply};
use crate::cache::{CacheInfo, CacheMangaStats, CacheSourceStats};
use crate::error::AppError;
use crate::repo::bookmark::BookmarkSort;
use crate::repo::diagnostics::{DiagnosticLogEntry, NewDiagnosticLog};
use crate::repo::history::LastReadingDestination;
use crate::repo::manga::HideAction;
use crate::repo::session::CrashRecoverySession;
use crate::services::app_release::{AppReleaseInfo, CURRENT_APP_VERSION, InstallUpdateOutcome};
use crate::services::library::{
    BookmarkItemView, FetchMode, HiddenMangaView, HistoryItemView, MangaView, NotificationItemView,
    ResolveResponse,
};
use crate::services::reader::ChapterView;
use crate::services::sync::{SyncDecision, SyncSenderMode, SyncStatusView};
use crate::services::updater::{CheckScope, UpdateSummary};
use crate::services::{app_release, library, reader, sync, updater};
use crate::settings::{Settings, SettingsSection};
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

/// Saves the user's current reading position (including exact `page_index`) into `history`.
#[tauri::command]
pub(crate) async fn save_progress(
    state: State<'_, Arc<AppState>>,
    manga_id: i64,
    chapter_url: String,
    chapter_name: String,
    scroll_ratio: f64,
    page_index: Option<i64>,
) -> Result<(), AppError> {
    library::save_progress(
        &state,
        manga_id,
        &chapter_url,
        &chapter_name,
        scroll_ratio,
        page_index.unwrap_or(0),
    )
    .await
}

/// Returns the persisted last reading destination used for History double-tap resume.
#[tauri::command]
pub(crate) async fn get_last_reading_destination(
    state: State<'_, Arc<AppState>>,
) -> Result<Option<LastReadingDestination>, AppError> {
    library::get_last_reading_destination(&state).await
}

/// Toggles bookmark state for a manga and returns the updated boolean state.
#[tauri::command]
pub(crate) async fn toggle_bookmark(
    state: State<'_, Arc<AppState>>,
    manga_id: i64,
) -> Result<bool, AppError> {
    library::toggle_bookmark(&state, manga_id).await
}

/// Lists all bookmarked manga in the requested sort order.
#[tauri::command]
pub(crate) async fn list_bookmarks(
    state: State<'_, Arc<AppState>>,
    sort: Option<BookmarkSort>,
) -> Result<Vec<BookmarkItemView>, AppError> {
    library::list_bookmarks(&state, sort.unwrap_or_default()).await
}

/// Sets or clears a manga's per-title reader mode override.
#[tauri::command]
pub(crate) async fn set_manga_reader_override(
    state: State<'_, Arc<AppState>>,
    manga_id: i64,
    override_mode: Option<String>,
) -> Result<(), AppError> {
    library::set_manga_reader_override(&state, manga_id, override_mode.as_deref()).await
}

/// Persists that passive Webtoon / Long-Strip detection matched a manga.
#[tauri::command]
pub(crate) async fn mark_manga_detected_webtoon(
    state: State<'_, Arc<AppState>>,
    manga_id: i64,
) -> Result<(), AppError> {
    library::mark_manga_detected_webtoon(&state, manga_id).await
}

/// Lists persistent chapter notifications for the Notification Center.
#[tauri::command]
pub(crate) async fn list_notifications(
    state: State<'_, Arc<AppState>>,
) -> Result<Vec<NotificationItemView>, AppError> {
    library::list_notifications(&state).await
}

/// Marks a single notification as read.
#[tauri::command]
pub(crate) async fn mark_notification_read(
    state: State<'_, Arc<AppState>>,
    id: i64,
) -> Result<(), AppError> {
    library::mark_notification_read(&state, id).await
}

/// Marks all stored notifications as read.
#[tauri::command]
pub(crate) async fn mark_all_notifications_read(
    state: State<'_, Arc<AppState>>,
) -> Result<(), AppError> {
    library::mark_all_notifications_read(&state).await
}

/// Deletes a single notification from the Notification Center.
#[tauri::command]
pub(crate) async fn delete_notification(
    state: State<'_, Arc<AppState>>,
    id: i64,
) -> Result<(), AppError> {
    library::delete_notification(&state, id).await
}

/// Clears all notifications from the Notification Center.
#[tauri::command]
pub(crate) async fn clear_notifications(state: State<'_, Arc<AppState>>) -> Result<(), AppError> {
    library::clear_notifications(&state).await
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
    apply_settings_side_effects(&app, &state, saved).await
}

async fn apply_settings_side_effects(
    app: &AppHandle,
    state: &Arc<AppState>,
    saved: Settings,
) -> Result<Settings, AppError> {
    let (schedule_changed, doh_changed, proxy_changed, conn_changed, speed_changed) = {
        let mut guard = state.settings.write().await;
        let sched = guard.updates.enabled != saved.updates.enabled
            || guard.updates.interval_hours != saved.updates.interval_hours;
        let doh = guard.doh != saved.doh;
        let prox = guard.proxy != saved.proxy;
        let conn = guard.sources.max_conn_per_host != saved.sources.max_conn_per_host;
        let speed = guard.sources.speed_limit_kbps != saved.sources.speed_limit_kbps;
        *guard = saved.clone();
        (sched, doh, prox, conn, speed)
    };
    if doh_changed {
        state.registry.http().set_doh_config(saved.doh.clone());
    }
    if proxy_changed {
        state.registry.http().set_proxy_config(saved.proxy.clone());
    }
    if conn_changed {
        state
            .registry
            .http()
            .set_max_conn_per_host(usize::from(saved.sources.max_conn_per_host));
    }
    if speed_changed {
        state
            .registry
            .http()
            .set_speed_limit_kbps(saved.sources.speed_limit_kbps);
    }
    let _ = app.emit("settings://changed", &saved);
    state.images.evict_if_needed(saved.cache.max_mb).await;
    if schedule_changed {
        updater::restart_background_loop(app.clone(), Arc::clone(state)).await;
    }
    Ok(saved)
}

/// Resets a single settings section to factory defaults without touching user library/history data.
#[tauri::command]
pub(crate) async fn reset_settings_section(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    section: SettingsSection,
) -> Result<Settings, AppError> {
    let mut current = state.current_settings().await;
    current.reset_section(section);
    let saved = state.db.save_settings(&current).await?;
    apply_settings_side_effects(&app, &state, saved).await
}

/// Resets all application settings to factory defaults without touching manga, chapters, history, or cache.
#[tauri::command]
pub(crate) async fn reset_all_settings(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
) -> Result<Settings, AppError> {
    let saved = state.db.save_settings(&Settings::default()).await?;
    apply_settings_side_effects(&app, &state, saved).await
}

/// Hides or unhides a manga from normal library listings.
#[tauri::command]
pub(crate) async fn set_manga_hidden(
    state: State<'_, Arc<AppState>>,
    manga_id: i64,
    action: HideAction,
) -> Result<bool, AppError> {
    library::set_manga_hidden(&state, manga_id, action).await
}

/// Lists all hidden manga so they can be restored or permanently deleted.
#[tauri::command]
pub(crate) async fn list_hidden_manga(
    state: State<'_, Arc<AppState>>,
) -> Result<Vec<HiddenMangaView>, AppError> {
    library::list_hidden_manga(&state).await
}

/// Permanently deletes a manga, its chapter list, reading history, bookmarks, notifications, and cached images.
#[tauri::command]
pub(crate) async fn delete_manga_permanently(
    state: State<'_, Arc<AppState>>,
    manga_id: i64,
) -> Result<(), AppError> {
    library::delete_manga_permanently(&state, manga_id).await
}

/// Updates the Android keep-screen-on window flag while actively reading.
#[tauri::command]
#[expect(
    clippy::fn_params_excessive_bools,
    clippy::needless_pass_by_value,
    reason = "IPC command toggles Android FLAG_KEEP_SCREEN_ON with owned AppHandle"
)]
pub(crate) fn set_keep_screen_on(app: AppHandle, enabled: bool) -> Result<(), AppError> {
    crate::android_env::set_keep_screen_on(&app, enabled)
}

/// Checkpoints an active reader session for crash recovery.
#[tauri::command]
pub(crate) async fn upsert_reader_session(
    state: State<'_, Arc<AppState>>,
    manga_id: i64,
    chapter_url: String,
    chapter_name: String,
    scroll_ratio: f64,
    page_index: i64,
    reading_mode: String,
) -> Result<(), AppError> {
    state
        .db
        .upsert_reader_session(
            manga_id,
            &chapter_url,
            &chapter_name,
            scroll_ratio,
            page_index,
            &reading_mode,
        )
        .await
}

/// Clears the active reader session checkpoint on clean exit or dismiss.
#[tauri::command]
pub(crate) async fn clear_reader_session(state: State<'_, Arc<AppState>>) -> Result<(), AppError> {
    state.db.clear_reader_session().await
}

/// Returns any abnormally terminated reader session eligible for crash recovery.
#[tauri::command]
pub(crate) async fn get_crash_recovery_session(
    state: State<'_, Arc<AppState>>,
) -> Result<Option<CrashRecoverySession>, AppError> {
    state.db.get_crash_recovery_session().await
}

/// Records a sanitized frontend runtime/error log entry into SQLite diagnostics.
#[tauri::command]
pub(crate) async fn log_client_error(
    state: State<'_, Arc<AppState>>,
    entry: NewDiagnosticLog,
) -> Result<i64, AppError> {
    state.db.record_diagnostic_log(&entry).await
}

/// Lists recent sanitized crash and error diagnostic logs.
#[tauri::command]
pub(crate) async fn list_diagnostic_logs(
    state: State<'_, Arc<AppState>>,
    limit: Option<i64>,
) -> Result<Vec<DiagnosticLogEntry>, AppError> {
    state.db.list_diagnostic_logs(limit.unwrap_or(100)).await
}

/// Clears all stored diagnostic logs.
#[tauri::command]
pub(crate) async fn clear_diagnostic_logs(state: State<'_, Arc<AppState>>) -> Result<(), AppError> {
    state.db.clear_diagnostic_logs().await
}

/// Generates a structured, credential-free JSON diagnostic report ready for copy or file download.
#[tauri::command]
pub(crate) async fn export_diagnostic_logs(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
) -> Result<String, AppError> {
    let settings = state.current_settings().await;
    let logs = state.db.list_diagnostic_logs(500).await?;
    let cache = state.images.info().await.ok();
    let env = crate::android_env::get_device_env(&app).ok();

    let payload = serde_json::json!({
        "app": "RtMaga",
        "version": CURRENT_APP_VERSION,
        "exportedAtUnix": crate::repo::now_unix_secs(),
        "platform": get_platform(),
        "device": env.map(|e| serde_json::json!({
            "platform": e.platform,
            "sdkInt": e.sdk_int,
            "skin": e.skin,
            "orientation": e.orientation,
            "viewportDp": [e.viewport_width_dp, e.viewport_height_dp],
        })),
        "cache": cache,
        "settings": settings.redacted_for_diagnostics(),
        "logCount": logs.len(),
        "logs": logs,
    });

    serde_json::to_string_pretty(&payload).map_err(|e| AppError::Internal {
        message: format!("failed to serialize diagnostic bundle: {e}"),
    })
}

/// Lists disk cache usage grouped by manga source site.
#[tauri::command]
pub(crate) async fn list_cache_by_source(
    state: State<'_, Arc<AppState>>,
) -> Result<Vec<CacheSourceStats>, AppError> {
    state.images.list_by_source().await
}

/// Lists disk cache usage grouped by individual manga.
#[tauri::command]
pub(crate) async fn list_cache_by_manga(
    state: State<'_, Arc<AppState>>,
) -> Result<Vec<CacheMangaStats>, AppError> {
    state.images.list_by_manga().await
}

/// Clears cached images only for `site` (preserving library, history, bookmarks, and settings).
#[tauri::command]
pub(crate) async fn clear_cache_by_source(
    state: State<'_, Arc<AppState>>,
    site: String,
) -> Result<CacheInfo, AppError> {
    state.images.clear_by_source(&site).await
}

/// Clears cached images only for `manga_id` (preserving library, history, bookmarks, and settings).
#[tauri::command]
pub(crate) async fn clear_cache_by_manga(
    state: State<'_, Arc<AppState>>,
    manga_id: i64,
) -> Result<CacheInfo, AppError> {
    state.images.clear_by_manga(manga_id).await
}

/// Returns the built-in catalog of reputable DNS-over-HTTPS (`DoH`) providers and profiles.
#[tauri::command]
pub(crate) fn get_network_providers() -> Vec<NetworkProviderInfo> {
    network_provider_catalog()
}

/// Tests connectivity and round-trip latency for a DNS-over-HTTPS (`DoH`) provider configuration.
#[tauri::command]
pub(crate) async fn test_doh_provider(config: DohConfig) -> Result<DohTestResult, AppError> {
    test_doh_config(&config).await.map_err(AppError::from)
}

/// Returns live adaptive device environment info (safe areas, OEM skin, real device name, orientation).
#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "tauri::command extracts owned AppHandle"
)]
pub(crate) fn get_device_env(app: AppHandle) -> Result<DeviceEnvInfo, AppError> {
    crate::android_env::get_device_env(&app)
}

/// Updates mobile screen orientation lock (`"auto"`, `"portrait"`, or `"landscape"`).
#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "tauri::command extracts owned AppHandle"
)]
pub(crate) fn set_orientation_lock(
    app: AppHandle,
    mode: &str,
) -> Result<OrientationLockReply, AppError> {
    crate::android_env::set_orientation_lock(&app, mode)
}

/// Updates Android fullscreen immersive system bars mode.
#[tauri::command]
#[expect(
    clippy::fn_params_excessive_bools,
    clippy::needless_pass_by_value,
    reason = "IPC command toggles immersive fullscreen mode with owned AppHandle"
)]
pub(crate) fn set_immersive(app: AppHandle, enabled: bool) -> Result<(), AppError> {
    crate::android_env::set_immersive(&app, enabled)
}

/// Checks whether Android/OS notifications are currently permitted.
#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "tauri::command extracts owned AppHandle"
)]
pub(crate) fn check_notification_permission(
    app: AppHandle,
) -> Result<NotificationPermissionInfo, AppError> {
    crate::android_env::check_notification_permission(&app)
}

/// Requests Android 13+ `POST_NOTIFICATIONS` permission at runtime.
#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "tauri::command extracts owned AppHandle"
)]
pub(crate) fn request_notification_permission(
    app: AppHandle,
) -> Result<NotificationPermissionInfo, AppError> {
    crate::android_env::request_notification_permission(&app)
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

/// Returns the current serverless LAN sync status snapshot with the resolved local device name.
#[tauri::command]
pub(crate) async fn sync_get_status(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
) -> Result<SyncStatusView, AppError> {
    let mut status = state.sync.status().await;
    let custom = state.current_settings().await.device.custom_name;
    status.local_device_name =
        crate::android_env::resolve_display_device_name(Some(&app), custom.as_deref());
    Ok(status)
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
