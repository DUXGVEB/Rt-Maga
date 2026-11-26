//! Shared application state owned by the Tauri runtime.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64};

use mangacore::Registry;
use tokio::sync::{Mutex, RwLock, watch};
use tokio::task::JoinHandle;

use crate::cache::{CatalogCache, ChapterCache, ImageCache};
use crate::repo::Database;
use crate::services::app_release::AppReleaseInfo;
use crate::services::sync::SyncManager;
use crate::settings::Settings;

/// Central state injected into Tauri commands, background tasks, and the `img://` proxy.
#[derive(Debug)]
pub(crate) struct AppState {
    /// Local application data directory (`<app_local_data_dir>`), housing `rtmaga.db`, `cache/`, and `cbz/`.
    pub data_dir: PathBuf,
    /// Local SQLite database handle.
    pub db: Database,
    /// Multi-tier manga URL resolver and shared HTTP client.
    pub registry: Registry,
    /// Disk-backed LRU image cache.
    pub images: ImageCache,
    /// Short-lived in-memory chapter metadata cache.
    pub chapters: ChapterCache,
    /// Short-lived in-memory catalog & filter page cache.
    pub catalogs: CatalogCache,
    /// Live user settings synchronized with SQLite.
    pub settings: RwLock<Settings>,
    /// Persistent local HMAC key securing `img://` proxy tokens across app restarts.
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
    /// Monotonic counter of primary chapter load requests for cancelling superseded scrapes.
    pub chapter_load_seq: AtomicU64,
    /// Broadcast channel notifying in-flight chapter loads when a newer primary request starts.
    pub chapter_cancel_tx: watch::Sender<u64>,
}

impl AppState {
    /// Constructs [`AppState`] with a persistent local proxy HMAC key.
    pub(crate) fn new(
        data_dir: PathBuf,
        db: Database,
        registry: Registry,
        images: ImageCache,
        settings: Settings,
        proxy_key: [u8; 32],
    ) -> Arc<Self> {
        let (chapter_cancel_tx, _rx) = watch::channel(0_u64);

        Arc::new(Self {
            data_dir,
            db,
            registry,
            images,
            chapters: ChapterCache::new(),
            catalogs: CatalogCache::new(),
            settings: RwLock::new(settings),
            proxy_key,
            update_task: Mutex::new(None),
            update_check_running: AtomicBool::new(false),
            update_install_running: AtomicBool::new(false),
            latest_release: RwLock::new(None),
            sync: SyncManager::new(),
            chapter_load_seq: AtomicU64::new(0),
            chapter_cancel_tx,
        })
    }

    /// Returns a cloned snapshot of the current [`Settings`].
    pub(crate) async fn current_settings(&self) -> Settings {
        self.settings.read().await.clone()
    }
}
