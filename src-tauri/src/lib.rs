//! `RtMaga` application library shared by desktop (Windows) and mobile (Android) entry points.

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
        .plugin(tauri_plugin_opener::init())
        .register_asynchronous_uri_scheme_protocol("img", |ctx, request, responder| {
            let app = ctx.app_handle().clone();
            let uri = request.uri().to_string();
            tauri::async_runtime::spawn(async move {
                let state = app.state::<Arc<AppState>>();
                let response = image_proxy::handle_protocol_request(&state, &uri).await;
                responder.respond(response);
            });
        })
        .setup(|app| {
            let data_dir = app
                .path()
                .app_local_data_dir()
                .expect("app local data directory must resolve");
            let db_path = data_dir.join("rtmaga.db");
            let cache_dir = data_dir.join("cache").join("img");

            let app_state = tauri::async_runtime::block_on(async {
                let db = Database::open(&db_path).await?;
                let settings = db.load_settings().await?;
                let policy = HttpPolicy {
                    max_conn_per_host: usize::from(settings.sources.max_conn_per_host),
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
            app.manage(app_state);

            tauri::async_runtime::spawn(async move {
                updater::restart_background_loop(handle, state_for_bg).await;
            });
            app_release::spawn_startup_check(handle_for_release, state_for_release);

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::resolve_url,
            commands::get_manga,
            commands::refresh_manga,
            commands::open_manual_verification,
            commands::get_chapter,
            commands::save_progress,
            commands::mark_read,
            commands::list_history,
            commands::remove_history,
            commands::clear_history,
            commands::check_updates_now,
            commands::get_settings,
            commands::set_settings,
            commands::get_app_version,
            commands::get_cached_app_release,
            commands::check_app_release,
            commands::open_release_url,
            commands::cache_info,
            commands::clear_image_cache,
        ]);

    builder
        .run(tauri::generate_context!())
        .expect("error while running RtMaga application");
}

fn init_logging() {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("rtmaga_lib=info,mangacore=info"));
    let _ = tracing_subscriber::fmt().with_env_filter(filter).try_init();
}
