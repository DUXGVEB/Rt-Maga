//! Thin Tauri IPC command handlers delegating to [`crate::services`].

use std::path::PathBuf;
use std::sync::Arc;

use mangacore::{
    CatalogFilterQuery, DohConfig, DohTestResult, NetworkProviderInfo, ProxyEndpoint,
    ProxyTestResult, SiteFilterSchema, network_provider_catalog, test_doh_config,
};
use serde::Serialize;
use tauri::ipc::Channel;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_opener::OpenerExt;

use crate::android_env::{DeviceEnvInfo, NotificationPermissionInfo, OrientationLockReply};
use crate::cache::{CacheInfo, CacheMangaStats, CacheSourceStats};
use crate::error::AppError;
use crate::repo::bookmark::BookmarkSort;
use crate::repo::diagnostics::{
    DiagnosticLogEntry, NewDiagnosticLog, redact_sensitive, scrub_personal_paths,
};
use crate::repo::history::LastReadingDestination;
use crate::repo::manga::HideAction;
use crate::repo::session::CrashRecoverySession;
use crate::repo::sources::{SourcePlacement, VisibilityFilter, VisibilityPatch};
use crate::services::ai::{AiDownloadProgressEvent, AiPageMode, AiStatusView, PageAiResultView};
use crate::services::app_release::{AppReleaseInfo, CURRENT_APP_VERSION, InstallUpdateOutcome};
use crate::services::backup_service::{
    BackupExportResult, BackupImportMode, BackupImportReport, BackupPreviewView,
    BackupProgressEvent,
};
use crate::services::comics::{TachibkBackupPreview, TachibkImportReport, TachibkImportRequest};
use crate::services::layout_detect::LayoutDetectionResult;
use crate::services::library::{
    BookmarkItemView, CatalogPageView, FetchMode, HiddenMangaView, HistoryItemView, MangaView,
    NotificationItemView, ResolveResponse,
};
use crate::services::reader::{
    ChapterLoadPriority, ChapterView, SavedImageOutcome, SetCoverOutcome,
};
use crate::services::rtmaga_codec::BackupBlockSelection;
use crate::services::sources::{MultiSourceSearchResponse, SavedSourceView};
use crate::services::sync::{SyncDecision, SyncSenderMode, SyncStatusView};
use crate::services::updater::{CheckScope, UpdateSummary};
use crate::services::{
    app_release, backup_service, comics, layout_detect, library, reader, sources, sync, updater,
};
use crate::settings::{ReaderSettings, Settings, SettingsSection, TranslationModelTier};
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

/// Outcome returned after starting an interactive browser authentication flow.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BrowserAuthOutcome {
    /// `"integrated_session"` when Chromium synced cookies directly, or `"external_browser"`.
    pub mode: &'static str,
    /// `true` when domain session cookies were synchronized back into `HttpClient`.
    pub cookies_synced: bool,
}

/// Opens a visible browser window so the user can manually complete a site verification check.
#[tauri::command]
pub(crate) async fn open_manual_verification(
    state: State<'_, Arc<AppState>>,
    url: String,
) -> Result<(), AppError> {
    library::open_manual_verification(&state, &url).await
}

/// Opens an interactive browser sign-in flow for `url`, synchronizing domain cookies back into
/// the app on desktop when Chromium is available, or opening the system browser on mobile.
#[tauri::command]
pub(crate) async fn open_browser_login_session(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    url: String,
) -> Result<BrowserAuthOutcome, AppError> {
    let normalized = mangacore::normalize_url(&url).map_err(AppError::from)?;
    mangacore::http::validate_public_http_url(&normalized).map_err(AppError::from)?;

    match state.registry.open_user_login_session(&normalized).await {
        Ok(()) => Ok(BrowserAuthOutcome {
            mode: "integrated_session",
            cookies_synced: true,
        }),
        Err(mangacore::ScrapeError::Headless(msg))
            if {
                let lower = msg.to_ascii_lowercase();
                lower.contains("unavailable")
                    || lower.contains("executable")
                    || lower.contains("disabled")
            } =>
        {
            app.opener()
                .open_url(normalized.as_str(), None::<&str>)
                .map_err(|e| AppError::Internal {
                    message: format!("failed to open external login URL: {e}"),
                })?;
            Ok(BrowserAuthOutcome {
                mode: "external_browser",
                cookies_synced: false,
            })
        }
        Err(err) => Err(AppError::from(err)),
    }
}

/// Fetches ordered image tokens and neighbor chapter links for a chapter URL.
#[tauri::command]
pub(crate) async fn get_chapter(
    state: State<'_, Arc<AppState>>,
    url: String,
    manga_id: Option<i64>,
    priority: Option<ChapterLoadPriority>,
) -> Result<ChapterView, AppError> {
    reader::get_chapter(&state, &url, manga_id, priority.unwrap_or_default()).await
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

/// Persists or clears a manga's full per-title reader settings configuration.
#[tauri::command]
pub(crate) async fn set_manga_reader_settings(
    state: State<'_, Arc<AppState>>,
    manga_id: i64,
    reader_settings: Option<ReaderSettings>,
) -> Result<Option<ReaderSettings>, AppError> {
    library::set_manga_reader_settings(&state, manga_id, reader_settings).await
}

/// Reads plain text from the native OS clipboard (Android `ClipboardManager` / Windows Win32).
#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "tauri::command extracts owned AppHandle"
)]
pub(crate) fn read_clipboard_text(app: AppHandle) -> Result<Option<String>, AppError> {
    crate::android_env::read_clipboard_text(&app)
}

/// Opens a validated public `http://` or `https://` manga/chapter URL in the external browser.
#[tauri::command]
pub(crate) async fn open_external_url(app: AppHandle, url: String) -> Result<(), AppError> {
    let normalized = mangacore::normalize_url(&url).map_err(AppError::from)?;
    mangacore::http::validate_public_http_url(&normalized).map_err(AppError::from)?;

    #[cfg(target_os = "android")]
    {
        if crate::android_env::open_external_url_android(&app, normalized.as_str()).is_ok() {
            return Ok(());
        }
    }

    app.opener()
        .open_url(normalized.as_str(), None::<&str>)
        .map_err(|e| AppError::Internal {
            message: format!("failed to open external URL: {e}"),
        })
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
    let (
        schedule_changed,
        doh_changed,
        proxy_changed,
        conn_changed,
        speed_changed,
        ai_tier_changed,
        ai_disabled_now,
    ) = {
        let mut guard = state.settings.write().await;
        let sched = guard.updates.enabled != saved.updates.enabled
            || guard.updates.interval_hours != saved.updates.interval_hours;
        let doh = guard.doh != saved.doh;
        let prox = guard.proxy != saved.proxy;
        let conn = guard.sources.max_conn_per_host != saved.sources.max_conn_per_host;
        let speed = guard.sources.speed_limit_kbps != saved.sources.speed_limit_kbps;
        let tier_ch = guard.accessibility.translation_model_tier
            != saved.accessibility.translation_model_tier;
        let was_ai_active =
            guard.accessibility.bubble_auto_zoom || guard.accessibility.ai_translation_overlay;
        let is_ai_active =
            saved.accessibility.bubble_auto_zoom || saved.accessibility.ai_translation_overlay;
        *guard = saved.clone();
        (
            sched,
            doh,
            prox,
            conn,
            speed,
            tier_ch,
            was_ai_active && !is_ai_active,
        )
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
    if ai_tier_changed {
        state
            .ai
            .sync_active_tier(saved.accessibility.translation_model_tier)
            .await;
    }
    if ai_disabled_now {
        let _ = state.ai.hard_unload_and_purge(Some(app)).await;
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

/// Result returned when exporting diagnostic logs directly to a local file on disk.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SavedDiagnosticFile {
    /// Complete filesystem path where the diagnostic report was written.
    pub path: String,
    /// Exact byte size written to disk.
    pub bytes: u64,
    /// Number of diagnostic log entries included in the exported file.
    pub log_count: usize,
}

async fn build_diagnostic_export_json(
    app: &AppHandle,
    state: &AppState,
) -> Result<(String, usize), AppError> {
    let settings = state.current_settings().await;
    let logs = state.db.list_diagnostic_logs(500).await?;
    let log_count = logs.len();
    let cache = state.images.info().await.ok();
    let env = crate::android_env::get_device_env(app).ok();

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
        "logCount": log_count,
        "logs": logs,
    });

    let pretty = serde_json::to_string_pretty(&payload).map_err(|e| AppError::Internal {
        message: format!("failed to serialize diagnostic bundle: {e}"),
    })?;
    Ok((pretty, log_count))
}

/// Generates a structured, credential-free JSON diagnostic report ready for copy or inspection.
#[tauri::command]
pub(crate) async fn export_diagnostic_logs(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
) -> Result<String, AppError> {
    let (json, _) = build_diagnostic_export_json(&app, &state).await?;
    Ok(json)
}

/// Writes a structured, credential-free JSON diagnostic log file atomically to disk and returns
/// the exact saved file path.
#[tauri::command]
pub(crate) async fn save_diagnostic_logs_file(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
) -> Result<SavedDiagnosticFile, AppError> {
    let (json, log_count) = build_diagnostic_export_json(&app, &state).await?;
    let ts = crate::repo::now_unix_secs();
    write_export_file(
        &app,
        &format!("rtmaga-diagnostics-{ts}.json"),
        &json,
        log_count,
    )
    .await
}

async fn write_export_file(
    app: &AppHandle,
    filename: &str,
    contents: &str,
    log_count: usize,
) -> Result<SavedDiagnosticFile, AppError> {
    let target_dir = app
        .path()
        .download_dir()
        .or_else(|_| app.path().document_dir())
        .or_else(|_| app.path().app_local_data_dir().map(|d| d.join("logs")))
        .map_err(|e| AppError::Internal {
            message: format!("failed to resolve export directory: {e}"),
        })?;

    tokio::fs::create_dir_all(&target_dir)
        .await
        .map_err(|e| AppError::Internal {
            message: format!(
                "failed to prepare export directory '{}': {e}",
                target_dir.display()
            ),
        })?;

    let dest_path = target_dir.join(filename);
    let tmp_path = target_dir.join(format!(".{filename}.tmp"));
    let data = contents.as_bytes();

    if let Err(err) = tokio::fs::write(&tmp_path, data).await {
        return Err(AppError::Internal {
            message: format!("failed to write export file: {err}"),
        });
    }
    if let Err(err) = tokio::fs::rename(&tmp_path, &dest_path).await {
        let _ = tokio::fs::remove_file(&tmp_path).await;
        return Err(AppError::Internal {
            message: format!(
                "failed to finalize export file '{}': {err}",
                dest_path.display()
            ),
        });
    }

    let final_path = tokio::fs::canonicalize(&dest_path)
        .await
        .unwrap_or(dest_path);
    let display_path = final_path
        .to_string_lossy()
        .trim_start_matches(r"\\?\")
        .to_owned();
    let bytes = u64::try_from(data.len()).unwrap_or(0);

    Ok(SavedDiagnosticFile {
        path: display_path,
        bytes,
        log_count,
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

/// Purges cached chapter page images for a single finished chapter while preserving covers and settings.
#[tauri::command]
pub(crate) async fn purge_chapter_page_cache(
    state: State<'_, Arc<AppState>>,
    manga_id: i64,
    chapter_url: String,
) -> Result<CacheInfo, AppError> {
    state
        .images
        .clear_chapter_pages(manga_id, &chapter_url)
        .await
}

/// Purges all transient chapter page images for a manga (on exiting reader / finishing manga),
/// while keeping its cover thumbnail and per-manga reader settings intact.
#[tauri::command]
pub(crate) async fn purge_manga_page_cache(
    state: State<'_, Arc<AppState>>,
    manga_id: i64,
) -> Result<CacheInfo, AppError> {
    state.chapters.invalidate_all();
    state.images.clear_manga_pages(manga_id).await
}

/// Purges all transient chapter page images across the entire app while preserving covers and settings.
#[tauri::command]
pub(crate) async fn purge_all_page_cache(
    state: State<'_, Arc<AppState>>,
) -> Result<CacheInfo, AppError> {
    state.chapters.invalidate_all();
    state.images.clear_all_chapter_pages().await
}

/// Clears both the local disk image cache and the in-memory chapter cache (never touches manga settings).
#[tauri::command]
pub(crate) async fn clear_image_cache(
    state: State<'_, Arc<AppState>>,
) -> Result<CacheInfo, AppError> {
    state.images.clear().await?;
    state.chapters.invalidate_all();
    state.images.info().await
}

/// Lists saved sources filtered by visibility bucket (`main_only`, `supplemental_only`, `all`).
#[tauri::command]
pub(crate) async fn list_saved_sources(
    state: State<'_, Arc<AppState>>,
    filter: Option<VisibilityFilter>,
) -> Result<Vec<SavedSourceView>, AppError> {
    sources::list_saved_sources(&state, filter.unwrap_or_default()).await
}

/// Inspects a website URL, caches its icon, classifies its CMS structure, and saves it as a source.
#[tauri::command]
pub(crate) async fn add_saved_source(
    state: State<'_, Arc<AppState>>,
    url: String,
    display_name: Option<String>,
    placement: Option<SourcePlacement>,
) -> Result<SavedSourceView, AppError> {
    sources::add_saved_source(
        &state,
        &url,
        display_name.as_deref(),
        placement.unwrap_or_default(),
    )
    .await
}

/// Updates user-editable properties of a saved source (display name, mirror URL, enabled, or placement).
#[tauri::command]
pub(crate) async fn update_saved_source(
    state: State<'_, Arc<AppState>>,
    id: i64,
    display_name: Option<String>,
    base_url: Option<String>,
    is_enabled: Option<bool>,
    visibility: Option<VisibilityPatch>,
) -> Result<SavedSourceView, AppError> {
    sources::update_saved_source(
        &state,
        id,
        display_name.as_deref(),
        base_url.as_deref(),
        is_enabled,
        visibility.unwrap_or_default(),
    )
    .await
}

/// Deletes a saved source record by ID.
#[tauri::command]
pub(crate) async fn delete_saved_source(
    state: State<'_, Arc<AppState>>,
    id: i64,
) -> Result<(), AppError> {
    sources::delete_saved_source(&state, id).await
}

/// Browses, searches, or filters a single saved source using its mirror URL, CMS classification,
/// and per-website multi-tag/chapter/recency filter schema.
#[tauri::command]
pub(crate) async fn browse_saved_source(
    state: State<'_, Arc<AppState>>,
    source_id: i64,
    query: Option<String>,
    filters: Option<CatalogFilterQuery>,
    page: Option<u32>,
    force_refresh: Option<bool>,
) -> Result<CatalogPageView, AppError> {
    sources::browse_saved_source(
        &state,
        source_id,
        query.as_deref(),
        filters,
        page.unwrap_or(1),
        force_refresh.unwrap_or(false),
    )
    .await
}

/// Browses, paginates, or applies multi-tag/chapter/recency filters directly against an arbitrary
/// catalog/category/homepage URL.
#[tauri::command]
pub(crate) async fn browse_catalog_url(
    state: State<'_, Arc<AppState>>,
    url: String,
    filters: Option<CatalogFilterQuery>,
    page: Option<u32>,
    force_refresh: Option<bool>,
) -> Result<CatalogPageView, AppError> {
    library::browse_catalog_url(
        &state,
        &url,
        filters,
        page.unwrap_or(1),
        force_refresh.unwrap_or(false),
    )
    .await
}

/// Returns (or lazily discovers) the dynamic per-website [`SiteFilterSchema`] for a `source_id` or `url`.
#[tauri::command]
pub(crate) async fn get_web_filter_schema(
    state: State<'_, Arc<AppState>>,
    source_id: Option<i64>,
    url: Option<String>,
) -> Result<SiteFilterSchema, AppError> {
    sources::get_web_filter_schema(&state, source_id, url.as_deref()).await
}

/// Searches in parallel across all enabled saved sources in the requested visibility scope.
#[tauri::command]
pub(crate) async fn search_saved_sources(
    state: State<'_, Arc<AppState>>,
    query: String,
    filter: Option<VisibilityFilter>,
) -> Result<MultiSourceSearchResponse, AppError> {
    sources::search_across_sources(&state, &query, filter.unwrap_or_default()).await
}

/// Opens the native Windows multi-format comic picker when clicked by the user and imports selected files.
#[tauri::command]
pub(crate) async fn pick_and_import_cbz(
    state: State<'_, Arc<AppState>>,
    target_manga_id: Option<i64>,
) -> Result<Option<MangaView>, AppError> {
    comics::pick_and_import_comics_windows(state.data_dir.clone(), &state, target_manga_id).await
}

/// Imports user-selected or dropped comic files/folders (`.cbz`, `.cbr`, `.cb7`, `.cbt`, `.epub`,
/// `.pdf`, image folders, or loose images) into internal app storage.
#[tauri::command]
pub(crate) async fn import_cbz_paths(
    state: State<'_, Arc<AppState>>,
    paths: Vec<String>,
    target_manga_id: Option<i64>,
) -> Result<MangaView, AppError> {
    let buf_paths: Vec<PathBuf> = paths.into_iter().map(PathBuf::from).collect();
    comics::import_comic_paths(&state.data_dir, &state, &buf_paths, target_manga_id).await
}

/// Starts a chunked multi-format comic staging upload (for mobile `WebView` `<input type="file">`).
#[tauri::command]
pub(crate) async fn begin_cbz_upload(
    state: State<'_, Arc<AppState>>,
    filename: String,
) -> Result<String, AppError> {
    comics::begin_comic_staging(&state.data_dir, &filename).await
}

/// Appends a base64 chunk to a staged comic upload.
#[tauri::command]
pub(crate) async fn append_cbz_chunk(
    state: State<'_, Arc<AppState>>,
    upload_id: String,
    chunk_base64: String,
) -> Result<(), AppError> {
    comics::append_comic_staging_chunk(&state.data_dir, &upload_id, &chunk_base64).await
}

/// Finalizes one or more staged comic uploads and imports them as a manga or new chapters.
#[tauri::command]
pub(crate) async fn finish_cbz_upload(
    state: State<'_, Arc<AppState>>,
    upload_ids: Vec<String>,
    target_manga_id: Option<i64>,
) -> Result<MangaView, AppError> {
    comics::finish_comic_staging_import(&state.data_dir, &state, &upload_ids, target_manga_id).await
}

/// Inspects dropped or selected paths via binary magic-byte + protobuf probing and returns
/// only those that are valid Mihon/Tachiyomi `.tachibk` / `.proto.gz` backups.
#[tauri::command]
pub(crate) async fn detect_tachibk_paths(paths: Vec<String>) -> Result<Vec<String>, AppError> {
    let buf_paths: Vec<PathBuf> = paths.into_iter().map(PathBuf::from).collect();
    Ok(comics::detect_tachibk_paths(&buf_paths).await)
}

/// Decodes a `.tachibk` / `.proto.gz` backup file on disk and returns a structured preview
/// reconciled against existing `RtMaga` library entries.
#[tauri::command]
pub(crate) async fn preview_tachibk_backup(
    state: State<'_, Arc<AppState>>,
    path: String,
) -> Result<TachibkBackupPreview, AppError> {
    comics::preview_tachibk_from_path(&state, &PathBuf::from(path)).await
}

/// Decodes a staged `WebView` `.tachibk` / `.proto.gz` upload and returns a structured preview.
#[tauri::command]
pub(crate) async fn preview_tachibk_upload(
    state: State<'_, Arc<AppState>>,
    upload_id: String,
) -> Result<TachibkBackupPreview, AppError> {
    comics::preview_tachibk_from_upload(&state.data_dir, &state, &upload_id).await
}

/// Removes a staged `WebView` `.tachibk` upload file if the user cancels the import preview modal.
#[tauri::command]
pub(crate) async fn discard_tachibk_upload(
    state: State<'_, Arc<AppState>>,
    upload_id: String,
) -> Result<(), AppError> {
    comics::discard_tachibk_upload(&state.data_dir, &upload_id).await
}

/// Imports user-selected entries from a `.tachibk` / `.proto.gz` backup into `RtMaga`'s library,
/// auto-linking local matches and optionally creating metadata-only catalog entries.
#[tauri::command]
pub(crate) async fn import_tachibk_backup(
    state: State<'_, Arc<AppState>>,
    request: TachibkImportRequest,
) -> Result<TachibkImportReport, AppError> {
    comics::import_tachibk_backup(&state.data_dir, &state, &request).await
}

/// Category of user feedback / issue report.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub(crate) enum FeedbackReportKind {
    Bug,
    Suggestion,
}

/// Request payload from the UI bug report / feedback composer.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct FeedbackReportRequest {
    pub kind: FeedbackReportKind,
    pub title: String,
    pub description: String,
    pub attach_logs: bool,
    #[serde(default)]
    pub log_ids: Vec<i64>,
}

/// Formatted feedback Markdown report plus pre-filled GitHub Issue link.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct FormattedFeedbackReport {
    pub markdown: String,
    pub github_issue_url: String,
    pub url_truncated: bool,
    pub included_logs: usize,
}

async fn compose_feedback_report_inner(
    app: &AppHandle,
    state: &AppState,
    request: &FeedbackReportRequest,
) -> Result<FormattedFeedbackReport, AppError> {
    let clean_title = scrub_personal_paths(&redact_sensitive(request.title.trim()));
    if clean_title.is_empty() {
        return Err(AppError::InvalidUrl {
            message: "report title must not be empty".to_owned(),
        });
    }
    let clean_desc = scrub_personal_paths(&redact_sensitive(request.description.trim()));
    let logs = if request.attach_logs && !request.log_ids.is_empty() {
        state
            .db
            .list_diagnostic_logs_by_ids(&request.log_ids)
            .await?
    } else {
        Vec::new()
    };

    let env = crate::android_env::get_device_env(app).ok();
    let kind_label = match request.kind {
        FeedbackReportKind::Bug => "Bug Report",
        FeedbackReportKind::Suggestion => "Feature Suggestion",
    };
    let issue_prefix = match request.kind {
        FeedbackReportKind::Bug => "[Bug]",
        FeedbackReportKind::Suggestion => "[Feedback]",
    };
    let github_label = match request.kind {
        FeedbackReportKind::Bug => "bug",
        FeedbackReportKind::Suggestion => "enhancement",
    };

    let markdown =
        render_feedback_markdown(kind_label, &clean_title, &clean_desc, env.as_ref(), &logs);
    let issue_title = format!("{issue_prefix} {clean_title}");
    let full_url = build_github_issue_url(&issue_title, github_label, &markdown);

    let (github_issue_url, url_truncated) = if full_url.len() <= 5500 {
        (full_url, false)
    } else {
        let short_body = render_short_issue_body(kind_label, &clean_desc, env.as_ref(), logs.len());
        (
            build_github_issue_url(&issue_title, github_label, &short_body),
            true,
        )
    };

    Ok(FormattedFeedbackReport {
        markdown,
        github_issue_url,
        url_truncated,
        included_logs: logs.len(),
    })
}

fn render_feedback_markdown(
    kind_label: &str,
    title: &str,
    description: &str,
    env: Option<&DeviceEnvInfo>,
    logs: &[DiagnosticLogEntry],
) -> String {
    use std::fmt::Write as _;
    let mut out = String::with_capacity(1024);
    let _ = writeln!(out, "## [{kind_label}] {title}\n");
    let _ = writeln!(out, "### Description\n{description}\n");
    let _ = writeln!(out, "### Environment");
    let _ = writeln!(out, "- **App**: RtMaga v{CURRENT_APP_VERSION}");
    let _ = writeln!(out, "- **Platform**: {}", get_platform());
    if let Some(e) = env {
        let _ = writeln!(
            out,
            "- **Device**: {} {} ({}, SDK {}, {}x{}dp)",
            e.platform, e.skin, e.orientation, e.sdk_int, e.viewport_width_dp, e.viewport_height_dp
        );
    }
    if !logs.is_empty() {
        let _ = writeln!(out, "\n### Attached Diagnostic Logs ({})", logs.len());
        for entry in logs {
            let msg = scrub_personal_paths(&redact_sensitive(&entry.message));
            let src = entry.source.as_deref().unwrap_or("-");
            let _ = writeln!(
                out,
                "\n#### #{} [{}] `{}` (`{src}`)",
                entry.id, entry.level, entry.category
            );
            let _ = writeln!(out, "- **Message**: {msg}");
            if let Some(code) = entry.http_status {
                let _ = writeln!(out, "- **HTTP Status**: {code}");
            }
            if let Some(ref st) = entry.parsing_stage {
                let _ = writeln!(out, "- **Stage**: `{st}`");
            }
            if let Some(ref det) = entry.detail {
                let safe_det = scrub_personal_paths(&redact_sensitive(det));
                let _ = writeln!(
                    out,
                    "<details><summary>Detail</summary>\n\n```text\n{safe_det}\n```\n</details>"
                );
            }
        }
    }
    out
}

fn render_short_issue_body(
    kind_label: &str,
    description: &str,
    env: Option<&DeviceEnvInfo>,
    log_count: usize,
) -> String {
    use std::fmt::Write as _;
    let mut out = String::with_capacity(512);
    let _ = writeln!(out, "**Type**: {kind_label}");
    let _ = writeln!(
        out,
        "**App**: RtMaga v{CURRENT_APP_VERSION} ({})",
        get_platform()
    );
    if let Some(e) = env {
        let _ = writeln!(
            out,
            "**Orientation**: {} ({}x{}dp)",
            e.orientation, e.viewport_width_dp, e.viewport_height_dp
        );
    }
    let trimmed_desc: String = description.chars().take(1200).collect();
    let _ = writeln!(out, "\n### Description\n{trimmed_desc}\n");
    if log_count > 0 {
        let _ = writeln!(
            out,
            "\n*(Full Markdown report with {log_count} sanitized log entries was copied to your clipboard — please paste below.)*"
        );
    }
    out
}

fn build_github_issue_url(title: &str, label: &str, body: &str) -> String {
    let mut ser = url::form_urlencoded::Serializer::new(String::new());
    ser.append_pair("title", title);
    ser.append_pair("labels", label);
    ser.append_pair("body", body);
    format!(
        "https://github.com/DUXGVEB/Rt-Maga/issues/new?{}",
        ser.finish()
    )
}

/// Builds a redacted Markdown feedback/bug report and pre-filled GitHub Issue URL.
#[tauri::command]
pub(crate) async fn build_feedback_report(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    request: FeedbackReportRequest,
) -> Result<FormattedFeedbackReport, AppError> {
    compose_feedback_report_inner(&app, &state, &request).await
}

/// Saves a formatted, redacted Markdown feedback/bug report to disk and returns the saved file path.
#[tauri::command]
pub(crate) async fn save_feedback_report_file(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    request: FeedbackReportRequest,
) -> Result<SavedDiagnosticFile, AppError> {
    let report = compose_feedback_report_inner(&app, &state, &request).await?;
    let ts = crate::repo::now_unix_secs();
    write_export_file(
        &app,
        &format!("rtmaga-feedback-{ts}.md"),
        &report.markdown,
        report.included_logs,
    )
    .await
}

/// Exports selected library/source/settings blocks into a compressed, checksummed `.rtmaga`
/// binary backup using native OS file dialogs (Windows) or Android SAF, streaming progress via `Channel`.
#[tauri::command]
pub(crate) async fn rtmaga_backup_export(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    selection: BackupBlockSelection,
    on_progress: Channel<BackupProgressEvent>,
) -> Result<BackupExportResult, AppError> {
    backup_service::export_rtmaga_interactive(&app, &state, selection, on_progress).await
}

/// Opens the native OS picker (Windows) or Android SAF picker (`*/*`) for a `.rtmaga` backup,
/// runs 3-stage pre-flight validation off the main thread, and returns its metadata preview.
#[tauri::command]
pub(crate) async fn rtmaga_backup_pick_preview(
    app: AppHandle,
) -> Result<Option<BackupPreviewView>, AppError> {
    backup_service::pick_and_preview_rtmaga(&app).await
}

/// Validates and previews a `.rtmaga` backup file at `file_path` without mutating SQLite.
#[tauri::command]
pub(crate) async fn rtmaga_backup_preview_path(
    file_path: String,
) -> Result<BackupPreviewView, AppError> {
    backup_service::preview_rtmaga_file(
        PathBuf::from(file_path),
        None,
        backup_service::BackupFileOrigin::UserFile,
    )
    .await
}

/// Removes a temporary Android SAF `.rtmaga` staging file if the user cancels the import modal.
#[tauri::command]
pub(crate) async fn rtmaga_backup_discard_temp(app: AppHandle, file_path: String) {
    backup_service::discard_temp_backup_file(&app, &file_path).await;
}

/// Imports a pre-validated `.rtmaga` backup atomically inside a single `BEGIN IMMEDIATE`
/// SQLite transaction using Smart Merge (default) or Overwrite, streaming progress via `Channel`.
#[tauri::command]
#[expect(
    clippy::fn_params_excessive_bools,
    reason = "Tauri IPC command boundary maps directly to frontend boolean flag"
)]
pub(crate) async fn rtmaga_backup_import(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    file_path: String,
    mode: BackupImportMode,
    is_temp_file: bool,
    on_progress: Channel<BackupProgressEvent>,
) -> Result<BackupImportReport, AppError> {
    let origin = if is_temp_file {
        backup_service::BackupFileOrigin::TempStaging
    } else {
        backup_service::BackupFileOrigin::UserFile
    };
    backup_service::import_rtmaga_file(
        &app,
        &state,
        &PathBuf::from(file_path),
        mode,
        origin,
        on_progress,
    )
    .await
}

/// Saves a reader page image directly to Android `MediaStore` Gallery or Windows Pictures/RtMaga,
/// reusing any locally cached bytes without re-fetching.
#[tauri::command]
pub(crate) async fn download_reader_image(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    token: String,
    manga_title: Option<String>,
    page_index: Option<u32>,
) -> Result<SavedImageOutcome, AppError> {
    reader::download_reader_image(&app, &state, &token, manga_title.as_deref(), page_index).await
}

/// Persists a reader page image as the manga's cover in SQLite and persistent cover cache,
/// returning a fresh signed cover token for immediate reactive UI refresh.
#[tauri::command]
pub(crate) async fn set_manga_cover_from_page(
    state: State<'_, Arc<AppState>>,
    manga_id: i64,
    token: String,
) -> Result<SetCoverOutcome, AppError> {
    reader::set_manga_cover_from_page(&state, manga_id, &token).await
}

/// Runs non-blocking Rust-assisted Webtoon vs. Manga classification using manual override priority,
/// metadata hints, and header-only `imagesize` dimension sniffing on the first 2–4 chapter pages.
#[tauri::command]
pub(crate) async fn detect_chapter_layout(
    state: State<'_, Arc<AppState>>,
    manga_id: i64,
    page_tokens: Vec<String>,
) -> Result<LayoutDetectionResult, AppError> {
    layout_detect::detect_chapter_layout(&state, manga_id, &page_tokens).await
}

/// Returns the live status of the 2-tier on-device AI engine, model availability, and storage path.
#[tauri::command]
pub(crate) async fn ai_get_status(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
) -> Result<AiStatusView, AppError> {
    Ok(state.ai.status(Some(&app)).await)
}

/// Hot-switches the active AI translation model tier (`lite` or `qwen3_vl_2b`) without restart and
/// persists the selection in user settings.
#[tauri::command]
pub(crate) async fn ai_set_tier(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    tier: TranslationModelTier,
) -> Result<AiStatusView, AppError> {
    let status = state.ai.set_tier(Some(&app), tier).await?;
    persist_ai_tier_setting(&app, &state, status.active_tier).await?;
    Ok(status)
}

/// Downloads and verifies the Tier 2 `Qwen3-VL-2B` GGUF + `mmproj` weights (plus Windows `llama.cpp`
/// sidecar on Windows), streaming progress via `on_progress` and auto-switching upon completion.
#[tauri::command]
pub(crate) async fn ai_download_tier2(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    on_progress: Channel<AiDownloadProgressEvent>,
) -> Result<AiStatusView, AppError> {
    let status = state.ai.download_tier2(&app, on_progress).await?;
    persist_ai_tier_setting(&app, &state, status.active_tier).await?;
    Ok(status)
}

/// Cancels any active Tier 2 AI model download immediately and cleans up partial temp files.
#[tauri::command]
pub(crate) async fn ai_cancel_download(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
) -> Result<AiStatusView, AppError> {
    Ok(state.ai.cancel_download(Some(&app)).await)
}

/// Hard-unloads Tier 2, deletes downloaded Tier 2 model weights from disk, and reverts to `lite`.
#[tauri::command]
pub(crate) async fn ai_delete_tier2(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
) -> Result<AiStatusView, AppError> {
    let status = state.ai.delete_tier2(Some(&app)).await?;
    persist_ai_tier_setting(&app, &state, status.active_tier).await?;
    Ok(status)
}

/// Runs speech-bubble detection (`detect_only = true`) or full on-device manga page translation.
#[tauri::command]
pub(crate) async fn ai_translate_page(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    page_token: String,
    target_lang: String,
    detect_only: Option<bool>,
    generation: Option<u64>,
) -> Result<PageAiResultView, AppError> {
    let mode = if detect_only.unwrap_or(false) {
        AiPageMode::DetectOnly
    } else {
        AiPageMode::Translate
    };
    state
        .ai
        .translate_page(
            Some(&app),
            &state,
            &page_token,
            &target_lang,
            mode,
            generation,
        )
        .await
}

/// Immediately cancels all queued or in-flight page AI jobs and returns the new job generation.
#[tauri::command]
pub(crate) async fn ai_cancel_jobs(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
) -> Result<u64, AppError> {
    Ok(state.ai.cancel_jobs(Some(&app)).await)
}

/// Cancels active AI jobs, hard-unloads all native runtimes/sidecars, clears RAM caches, and purges
/// all transient AI disk cache files under `<data_dir>/cache/ai/`.
#[tauri::command]
pub(crate) async fn ai_hard_unload_and_purge(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
) -> Result<AiStatusView, AppError> {
    state.ai.hard_unload_and_purge(Some(&app)).await
}

async fn persist_ai_tier_setting(
    app: &AppHandle,
    state: &Arc<AppState>,
    tier: TranslationModelTier,
) -> Result<(), AppError> {
    let mut current = state.current_settings().await;
    if current.accessibility.translation_model_tier == tier {
        return Ok(());
    }
    current.accessibility.translation_model_tier = tier;
    let saved = state.db.save_settings(&current).await?;
    {
        let mut guard = state.settings.write().await;
        *guard = saved.clone();
    }
    let _ = app.emit("settings://changed", &saved);
    Ok(())
}
