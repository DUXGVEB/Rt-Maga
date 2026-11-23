//! `RtMaga` application library shared by desktop (Windows) and mobile (Android) entry points.

mod android_env;
mod android_installer;
mod cache;
mod commands;
mod error;
mod image_proxy;
mod repo;
mod services;
mod settings;
mod state;

use std::sync::Arc;

use mangacore::{HttpClient, HttpPolicy, Registry};
use tauri::Manager;
use tracing::{info, warn};
use tracing_subscriber::EnvFilter;

use crate::cache::ImageCache;
use crate::repo::Database;
use crate::services::{app_release, updater};
use crate::state::AppState;

/// Number of days to keep non-history manga records before automatic cleanup.
const STALE_MANGA_RETENTION_DAYS: i64 = 30;

/// Launches the `RtMaga` Tauri application on desktop or Android.
///
/// # Panics
///
/// Panics if the local data directory, SQLite database, or Tauri runtime fails to initialize.
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    init_logging();

    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_opener::init());

    #[cfg(target_os = "android")]
    let builder = builder
        .plugin(android_installer::init_plugin())
        .plugin(android_env::init_plugin());

    let builder = builder
        .register_asynchronous_uri_scheme_protocol("img", |ctx, request, responder| {
            let app = ctx.app_handle().clone();
            let uri = request.uri().to_string();
            tauri::async_runtime::spawn(async move {
                let state = app.state::<Arc<AppState>>();
                let response = image_proxy::handle_protocol_request(&state, &uri).await;
                responder.respond(response);
            });
        })
        .setup(setup_app)
        .invoke_handler(tauri::generate_handler![
            commands::resolve_url,
            commands::get_manga,
            commands::refresh_manga,
            commands::open_manual_verification,
            commands::get_chapter,
            commands::save_progress,
            commands::get_last_reading_destination,
            commands::mark_read,
            commands::list_history,
            commands::remove_history,
            commands::clear_history,
            commands::toggle_bookmark,
            commands::list_bookmarks,
            commands::set_manga_hidden,
            commands::list_hidden_manga,
            commands::delete_manga_permanently,
            commands::set_manga_reader_override,
            commands::mark_manga_detected_webtoon,
            commands::list_notifications,
            commands::mark_notification_read,
            commands::mark_all_notifications_read,
            commands::delete_notification,
            commands::clear_notifications,
            commands::check_updates_now,
            commands::get_settings,
            commands::set_settings,
            commands::reset_settings_section,
            commands::reset_all_settings,
            commands::get_platform,
            commands::get_device_env,
            commands::set_orientation_lock,
            commands::set_immersive,
            commands::set_keep_screen_on,
            commands::check_notification_permission,
            commands::request_notification_permission,
            commands::upsert_reader_session,
            commands::clear_reader_session,
            commands::get_crash_recovery_session,
            commands::log_client_error,
            commands::list_diagnostic_logs,
            commands::clear_diagnostic_logs,
            commands::export_diagnostic_logs,
            commands::get_network_providers,
            commands::test_doh_provider,
            commands::test_proxy,
            commands::sync_get_status,
            commands::sync_start_sender,
            commands::sync_cancel,
            commands::sync_host_respond,
            commands::sync_connect_qr,
            commands::sync_connect_pin,
            commands::sync_receiver_confirm,
            commands::get_app_version,
            commands::get_cached_app_release,
            commands::check_app_release,
            commands::open_release_url,
            commands::download_and_install_update,
            commands::cache_info,
            commands::list_cache_by_source,
            commands::list_cache_by_manga,
            commands::clear_cache_by_source,
            commands::clear_cache_by_manga,
            commands::clear_image_cache,
        ]);

    builder
        .run(tauri::generate_context!())
        .expect("error while running RtMaga application");
}

#[expect(
    clippy::unnecessary_wraps,
    reason = "tauri::Builder::setup callback signature requires Result"
)]
fn setup_app(app: &mut tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    let data_dir = app
        .path()
        .app_local_data_dir()
        .expect("app local data directory must resolve");
    let db_path = data_dir.join("rtmaga.db");
    let cache_dir = data_dir.join("cache").join("img");
    let crash_file = data_dir.join("crash_last.txt");
    install_panic_hook(crash_file.clone());

    let app_state = tauri::async_runtime::block_on(async {
        let db = Database::open(&db_path).await?;
        import_pending_crash_file(&db, &crash_file).await;
        let settings = db.load_settings().await?;
        let policy = HttpPolicy {
            max_conn_per_host: usize::from(settings.sources.max_conn_per_host),
            doh: settings.doh.clone(),
            proxy: settings.proxy.clone(),
            speed_limit_kbps: settings.sources.speed_limit_kbps,
            ..HttpPolicy::default()
        };
        let http = HttpClient::with_policy(policy).map_err(error::AppError::from)?;
        let registry = Registry::with_http(http);
        let images = ImageCache::new(cache_dir, db.clone()).await?;

        match db.prune_stale_manga(STALE_MANGA_RETENTION_DAYS).await {
            Ok(removed) if removed > 0 => info!(removed, "pruned stale manga entries"),
            Err(err) => warn!(%err, "failed to prune stale manga entries"),
            _ => {}
        }

        Ok::<Arc<AppState>, error::AppError>(AppState::new(db, registry, images, settings))
    })
    .expect("failed to initialize RtMaga application state");

    let handle = app.handle().clone();
    let handle_for_release = app.handle().clone();
    let state_for_bg = Arc::clone(&app_state);
    let state_for_release = Arc::clone(&app_state);
    let cache_root_for_cleanup = app.path().app_cache_dir().ok();
    app.manage(app_state);

    if let Some(cache_root) = cache_root_for_cleanup {
        tauri::async_runtime::spawn(async move {
            app_release::cleanup_stale_installers(cache_root).await;
        });
    }

    tauri::async_runtime::spawn(async move {
        updater::restart_background_loop(handle, state_for_bg).await;
    });
    app_release::spawn_startup_check(handle_for_release, state_for_release);

    Ok(())
}

fn install_panic_hook(crash_file: std::path::PathBuf) {
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let payload = info
            .payload()
            .downcast_ref::<&str>()
            .map(|s| (*s).to_owned())
            .or_else(|| info.payload().downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "unexpected panic".to_owned());
        let location = info.location().map_or_else(
            || "unknown location".to_owned(),
            |loc| format!("{}:{}:{}", loc.file(), loc.line(), loc.column()),
        );
        let report = format!("panic at {location}: {payload}");
        let _ = std::fs::write(&crash_file, repo::diagnostics::redact_sensitive(&report));
        default_hook(info);
    }));
}

async fn import_pending_crash_file(db: &Database, crash_file: &std::path::Path) {
    if let Ok(contents) = tokio::fs::read_to_string(crash_file).await {
        let trimmed = contents.trim();
        if !trimmed.is_empty() {
            let _ = db
                .record_diagnostic_log(&repo::diagnostics::NewDiagnosticLog {
                    level: "crash".to_owned(),
                    category: "internal".to_owned(),
                    message: "Previous session ended with an unhandled crash".to_owned(),
                    detail: Some(trimmed.to_owned()),
                    source: None,
                    chapter_url: None,
                    stack_trace: Some(trimmed.to_owned()),
                })
                .await;
        }
        let _ = tokio::fs::remove_file(crash_file).await;
    }
}

fn init_logging() {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("rtmaga_lib=info,mangacore=info"));
    let _ = tracing_subscriber::fmt().with_env_filter(filter).try_init();
}
