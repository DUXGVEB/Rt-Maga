//! Shared application state owned by the Tauri runtime.

use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use mangacore::Registry;
use rand::RngCore;
use tokio::sync::{Mutex, RwLock};
use tokio::task::JoinHandle;

use crate::cache::{ChapterCache, ImageCache};
use crate::repo::Database;
use crate::services::app_release::AppReleaseInfo;
use crate::services::sync::SyncManager;
use crate::settings::Settings;

/// Central state injected into Tauri commands, background tasks, and the `img://` proxy.
#[derive(Debug)]
pub(crate) struct AppState {
    /// Local SQLite database handle.
    pub db: Database,
    /// Multi-tier manga URL resolver and shared HTTP client.
    pub registry: Registry,
    /// Disk-backed LRU image cache.
    pub images: ImageCache,
    /// Short-lived in-memory chapter metadata cache.
    pub chapters: ChapterCache,
    /// Live user settings synchronized with SQLite.
    pub settings: RwLock<Settings>,
    /// Per-session random HMAC key securing `img://` proxy tokens.
    pub proxy_key: [u8; 32],
    /// Handle to the background periodic update-checker task.
    pub update_task: Mutex<Option<JoinHandle<()>>>,
    /// Guards against overlapping manual and scheduled chapter-update runs.
    pub update_check_running: AtomicBool,
    /// Guards against concurrent in-app update downloads/installations.
    pub update_install_running: AtomicBool,
    /// Cached result of the latest application release check.
    pub latest_release: RwLock<Option<AppReleaseInfo>>,
    /// Serverless peer-to-peer LAN synchronization session coordinator.
    pub sync: SyncManager,
}

impl AppState {
    /// Constructs [`AppState`] with a fresh cryptographically random proxy HMAC key.
    pub(crate) fn new(
        db: Database,
        registry: Registry,
        images: ImageCache,
        settings: Settings,
    ) -> Arc<Self> {
        let mut proxy_key = [0_u8; 32];
        rand::thread_rng().fill_bytes(&mut proxy_key);

        Arc::new(Self {
            db,
            registry,
            images,
            chapters: ChapterCache::new(),
            settings: RwLock::new(settings),
            proxy_key,
            update_task: Mutex::new(None),
            update_check_running: AtomicBool::new(false),
            update_install_running: AtomicBool::new(false),
            latest_release: RwLock::new(None),
            sync: SyncManager::new(),
        })
    }

    /// Returns a cloned snapshot of the current [`Settings`].
    pub(crate) async fn current_settings(&self) -> Settings {
        self.settings.read().await.clone()
    }
}
