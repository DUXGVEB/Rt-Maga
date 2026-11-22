//! SQLite repository layer backed by `sqlx` in WAL mode.

pub(crate) mod history;
pub(crate) mod manga;
pub(crate) mod settings;
pub(crate) mod sync;

use std::path::{Path, PathBuf};
use std::time::Duration;

use sqlx::SqlitePool;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};

use crate::error::AppError;

/// Shared SQLite database handle.
#[derive(Debug, Clone)]
pub(crate) struct Database {
    pool: SqlitePool,
    db_path: Option<PathBuf>,
}

impl Database {
    /// Opens or creates the SQLite database file at `path`, enables WAL mode, and runs migrations.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Db`] if the database cannot be opened or migrated.
    pub(crate) async fn open(path: &Path) -> Result<Self, AppError> {
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| AppError::Db {
                    message: format!("failed to create db directory: {e}"),
                })?;
        }

        let opts = SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true)
            .foreign_keys(true)
            .busy_timeout(Duration::from_secs(5))
            .journal_mode(SqliteJournalMode::Wal)
            .synchronous(SqliteSynchronous::Normal);

        let pool = SqlitePoolOptions::new()
            .max_connections(8)
            .connect_with(opts)
            .await?;

        Self::migrate_and_wrap(pool, Some(path.to_path_buf())).await
    }

    /// Opens an in-memory SQLite database for automated tests.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Db`] if migration fails.
    #[cfg(test)]
    pub(crate) async fn open_in_memory() -> Result<Self, AppError> {
        use std::str::FromStr;
        let opts = SqliteConnectOptions::from_str("sqlite::memory:")
            .expect("static sqlite memory url is valid")
            .foreign_keys(true);

        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(opts)
            .await?;

        Self::migrate_and_wrap(pool, None).await
    }

    async fn migrate_and_wrap(
        pool: SqlitePool,
        db_path: Option<PathBuf>,
    ) -> Result<Self, AppError> {
        sqlx::migrate!("./migrations")
            .run(&pool)
            .await
            .map_err(|e| AppError::Db {
                message: format!("migration failed: {e}"),
            })?;
        Ok(Self { pool, db_path })
    }

    /// Returns a reference to the underlying [`SqlitePool`].
    pub(crate) fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    /// Flushes the SQLite WAL journal into the primary database file and writes an atomic
    /// pre-update backup copy next to `rtmaga.db`.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Db`] if WAL checkpointing or creating the safety backup fails.
    pub(crate) async fn checkpoint_and_backup(&self) -> Result<Option<PathBuf>, AppError> {
        sqlx::query("PRAGMA wal_checkpoint(TRUNCATE)")
            .execute(&self.pool)
            .await?;

        let backup_path = if let Some(path) = self.db_path.as_ref() {
            if tokio::fs::try_exists(path).await.unwrap_or(false) {
                let bak = path.with_extension("db.pre-update.bak");
                let tmp = path.with_extension("db.pre-update.bak.tmp");
                tokio::fs::copy(path, &tmp)
                    .await
                    .map_err(|e| AppError::Db {
                        message: format!("failed to copy pre-update database backup: {e}"),
                    })?;
                if let Err(err) = tokio::fs::rename(&tmp, &bak).await {
                    let _ = tokio::fs::remove_file(&tmp).await;
                    return Err(AppError::Db {
                        message: format!("failed to commit pre-update database backup: {err}"),
                    });
                }
                Some(bak)
            } else {
                None
            }
        } else {
            None
        };

        Ok(backup_path)
    }

    /// Gracefully closes all pooled SQLite connections before application exit.
    #[cfg_attr(
        not(target_os = "windows"),
        expect(dead_code, reason = "used during Windows update shutdown")
    )]
    pub(crate) async fn close(&self) {
        self.pool.close().await;
    }
}

/// Returns the current Unix timestamp in seconds.
pub(crate) fn now_unix_secs() -> i64 {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    i64::try_from(secs).unwrap_or(i64::MAX)
}

#[cfg(test)]
mod tests {
    use mangacore::{ChapterRef, Manga, SiteId};
    use url::Url;

    use super::*;
    use crate::settings::{Settings, Theme};

    #[tokio::test]
    async fn round_trips_manga_history_and_settings() {
        let db = Database::open_in_memory().await.unwrap();
        let manga = Manga {
            url: Url::parse("https://example.com/series/demo-1").unwrap(),
            site: SiteId::generic("example.com"),
            title: "Demo Manga".to_owned(),
            cover: Some(Url::parse("https://cdn.example.com/demo.jpg").unwrap()),
            chapters: vec![
                ChapterRef {
                    url: Url::parse("https://example.com/series/demo-1-chap-1").unwrap(),
                    name: "Chapter 1".to_owned(),
                    number: Some(1.0),
                },
                ChapterRef {
                    url: Url::parse("https://example.com/series/demo-1-chap-2").unwrap(),
                    name: "Chapter 2".to_owned(),
                    number: Some(2.0),
                },
            ],
        };

        let id = db.upsert_manga(&manga).await.unwrap();
        let loaded = db.get_manga_by_id(id).await.unwrap().unwrap();
        assert_eq!(loaded.title, "Demo Manga");

        let chaps = db.list_chapters(id).await.unwrap();
        assert_eq!(chaps.len(), 2);

        db.save_history(
            id,
            "https://example.com/series/demo-1-chap-1",
            "Chapter 1",
            0.42,
        )
        .await
        .unwrap();

        let hist = db.list_history().await.unwrap();
        assert_eq!(hist.len(), 1);
        assert_eq!(hist[0].manga_id, id);

        let s = Settings {
            theme: Theme::Dark,
            ..Settings::default()
        };
        db.save_settings(&s).await.unwrap();
        assert_eq!(db.load_settings().await.unwrap().theme, Theme::Dark);

        // Mark chapter 1 read, then re-upsert with chapter 2's URL renamed; verify stale slug is pruned
        // while chapter 1's read flag is preserved.
        db.mark_chapter_read(id, "https://example.com/series/demo-1-chap-1")
            .await
            .unwrap();
        let mut updated = manga;
        updated.chapters[1].url =
            Url::parse("https://example.com/series/demo-1-chap-2-fixed").unwrap();
        db.upsert_manga(&updated).await.unwrap();
        let after = db.list_chapters(id).await.unwrap();
        assert_eq!(after.len(), 2, "stale chapter slug must be pruned");
        assert_eq!(after[0].read, 1, "existing read flag must be preserved");
        assert_eq!(
            after[1].url,
            "https://example.com/series/demo-1-chap-2-fixed"
        );

        // Non-finite scroll ratios must sanitize to 0.0 instead of violating NOT NULL.
        db.save_history(
            id,
            "https://example.com/series/demo-1-chap-1",
            "Chapter 1",
            f64::NAN,
        )
        .await
        .unwrap();
        let progress = db.get_history_for_manga(id).await.unwrap().unwrap();
        assert!((progress.2 - 0.0).abs() < f64::EPSILON);

        // Re-checking with an already-known chapter must NOT trigger a false new-chapter alert.
        let same = db.record_update_check(id, &updated).await.unwrap();
        assert!(same.is_none(), "known latest chapter must not flag has_new");

        // Adding a genuinely new chapter MUST trigger a new-chapter discovery.
        updated.chapters.push(ChapterRef {
            url: Url::parse("https://example.com/series/demo-1-chap-3").unwrap(),
            name: "Chapter 3".to_owned(),
            number: Some(3.0),
        });
        let discovered = db.record_update_check(id, &updated).await.unwrap();
        assert_eq!(discovered.as_deref(), Some("Chapter 3"));
    }

    #[tokio::test]
    async fn pre_update_checkpoint_and_backup_preserves_history() {
        let dir =
            std::env::temp_dir().join(format!("rtmaga_db_update_test_{}", rand::random::<u64>()));
        let db_path = dir.join("rtmaga.db");

        let db = Database::open(&db_path).await.unwrap();
        let manga = Manga {
            url: Url::parse("https://example.com/series/backup-demo").unwrap(),
            site: SiteId::generic("example.com"),
            title: "Backup Demo".to_owned(),
            cover: None,
            chapters: vec![ChapterRef {
                url: Url::parse("https://example.com/series/backup-demo-chap-1").unwrap(),
                name: "Chapter 1".to_owned(),
                number: Some(1.0),
            }],
        };
        let id = db.upsert_manga(&manga).await.unwrap();
        db.save_history(
            id,
            "https://example.com/series/backup-demo-chap-1",
            "Chapter 1",
            0.75,
        )
        .await
        .unwrap();

        let backup = db
            .checkpoint_and_backup()
            .await
            .unwrap()
            .expect("backup file must be created");
        db.close().await;
        assert!(backup.exists(), "backup file must exist on disk");

        // Re-open the backup database and verify history + manga survived WAL checkpoint intact.
        let reopened = Database::open(&backup).await.unwrap();
        let hist = reopened.list_history().await.unwrap();
        assert_eq!(hist.len(), 1);
        assert_eq!(hist[0].title, "Backup Demo");
        assert!((hist[0].scroll_ratio - 0.75).abs() < 1e-6);

        drop(reopened);
        let _ = tokio::fs::remove_dir_all(&dir).await;
    }

    fn sample_incoming_sync_bundle() -> crate::repo::sync::SyncBundle {
        use crate::repo::sync::{
            SyncBundle, SyncChapterPayload, SyncHistoryPayload, SyncMangaPayload,
        };
        SyncBundle {
            version: 1,
            device_name: "PeerPhone".to_owned(),
            exported_at: 5000,
            manga: vec![
                // Same chapter with lower scroll ratio -> skip, but chapter 2 read flag should union!
                SyncMangaPayload {
                    url: "https://example.com/series/same/".to_owned(),
                    site: "example.com".to_owned(),
                    title: "Same Chap".to_owned(),
                    cover_url: None,
                    latest_chapter_url: None,
                    latest_chapter_name: None,
                    chapters: vec![SyncChapterPayload {
                        url: "https://example.com/series/same-chap-2".to_owned(),
                        name: "Chapter 2".to_owned(),
                        number: Some(2.0),
                        idx: 1,
                        read: true,
                    }],
                    history: Some(SyncHistoryPayload {
                        chapter_url: "https://example.com/series/same-chap-2".to_owned(),
                        chapter_name: "Chapter 2".to_owned(),
                        scroll_ratio: 0.2,
                        read_at: 9999,
                    }),
                },
                // Incoming on Chapter 4 (further ahead than local Chapter 2) -> update!
                SyncMangaPayload {
                    url: "https://example.com/series/inc".to_owned(),
                    site: "example.com".to_owned(),
                    title: "Incoming Ahead".to_owned(),
                    cover_url: None,
                    latest_chapter_url: None,
                    latest_chapter_name: None,
                    chapters: Vec::new(),
                    history: Some(SyncHistoryPayload {
                        chapter_url: "https://example.com/series/inc-chap-4".to_owned(),
                        chapter_name: "Chapter 4".to_owned(),
                        scroll_ratio: 0.1,
                        read_at: 100,
                    }),
                },
                // Incoming on Chapter 3 (behind local Chapter 5) -> keep local!
                SyncMangaPayload {
                    url: "https://example.com/series/loc".to_owned(),
                    site: "example.com".to_owned(),
                    title: "Local Ahead".to_owned(),
                    cover_url: None,
                    latest_chapter_url: None,
                    latest_chapter_name: None,
                    chapters: Vec::new(),
                    history: Some(SyncHistoryPayload {
                        chapter_url: "https://example.com/series/loc-chap-3".to_owned(),
                        chapter_name: "Chapter 3".to_owned(),
                        scroll_ratio: 0.9,
                        read_at: 99_999,
                    }),
                },
                // Brand new manga -> add!
                SyncMangaPayload {
                    url: "https://example.com/series/brand-new".to_owned(),
                    site: "example.com".to_owned(),
                    title: "Brand New".to_owned(),
                    cover_url: None,
                    latest_chapter_url: Some(
                        "https://example.com/series/brand-new-chap-1".to_owned(),
                    ),
                    latest_chapter_name: Some("Chapter 1".to_owned()),
                    chapters: vec![SyncChapterPayload {
                        url: "https://example.com/series/brand-new-chap-1".to_owned(),
                        name: "Chapter 1".to_owned(),
                        number: Some(1.0),
                        idx: 0,
                        read: false,
                    }],
                    history: Some(SyncHistoryPayload {
                        chapter_url: "https://example.com/series/brand-new-chap-1".to_owned(),
                        chapter_name: "Chapter 1".to_owned(),
                        scroll_ratio: 0.6,
                        read_at: 2000,
                    }),
                },
            ],
        }
    }

    #[tokio::test]
    async fn sync_merge_resolves_duplicates_by_chapter_progress_and_unions_read_flags() {
        let db = Database::open_in_memory().await.unwrap();

        let mk_manga = |slug: &str, title: &str| Manga {
            url: Url::parse(&format!("https://example.com/series/{slug}")).unwrap(),
            site: SiteId::generic("example.com"),
            title: title.to_owned(),
            cover: None,
            chapters: (1..=5)
                .map(|n| ChapterRef {
                    url: Url::parse(&format!("https://example.com/series/{slug}-chap-{n}"))
                        .unwrap(),
                    name: format!("Chapter {n}"),
                    number: Some(f32::from(u8::try_from(n).unwrap())),
                })
                .collect(),
        };

        let id_same = db
            .upsert_manga(&mk_manga("same", "Same Chap"))
            .await
            .unwrap();
        db.save_history(
            id_same,
            "https://example.com/series/same-chap-2",
            "Chapter 2",
            0.5,
        )
        .await
        .unwrap();
        db.mark_chapter_read(id_same, "https://example.com/series/same-chap-1")
            .await
            .unwrap();

        let id_inc = db
            .upsert_manga(&mk_manga("inc", "Incoming Ahead"))
            .await
            .unwrap();
        db.save_history(
            id_inc,
            "https://example.com/series/inc-chap-2",
            "Chapter 2",
            0.3,
        )
        .await
        .unwrap();

        let id_loc = db
            .upsert_manga(&mk_manga("loc", "Local Ahead"))
            .await
            .unwrap();
        db.save_history(
            id_loc,
            "https://example.com/series/loc-chap-5",
            "Chapter 5",
            0.4,
        )
        .await
        .unwrap();

        let incoming = sample_incoming_sync_bundle();
        let report = db.merge_sync_bundle(&incoming).await.unwrap();
        assert_eq!(report.added_manga, 1);
        assert_eq!(report.updated_newer_chapter, 1);
        assert_eq!(report.skipped_same_chapter, 1);
        assert_eq!(report.kept_local_newer, 1);

        // Verify chapter read flags were unioned (both chap 1 and chap 2 read).
        let same_chaps = db.list_chapters(id_same).await.unwrap();
        assert_eq!(same_chaps[0].read, 1);
        assert_eq!(same_chaps[1].read, 1);

        // Verify Incoming Ahead moved to Chapter 4 while Local Ahead stayed on Chapter 5.
        let inc_h = db.get_history_for_manga(id_inc).await.unwrap().unwrap();
        assert_eq!(inc_h.0, "https://example.com/series/inc-chap-4");
        let loc_h = db.get_history_for_manga(id_loc).await.unwrap().unwrap();
        assert_eq!(loc_h.0, "https://example.com/series/loc-chap-5");

        // Verify export round-trip contains all 4 history manga.
        let exported = db.export_sync_bundle("TestPC").await.unwrap();
        assert_eq!(exported.manga.len(), 4);
    }
}
