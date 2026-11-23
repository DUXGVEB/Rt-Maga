//! SQLite repository layer backed by `sqlx` in WAL mode.

pub(crate) mod bookmark;
pub(crate) mod diagnostics;
pub(crate) mod history;
pub(crate) mod manga;
pub(crate) mod notification;
pub(crate) mod session;
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
            3,
        )
        .await
        .unwrap();

        let hist = db.list_history().await.unwrap();
        assert_eq!(hist.len(), 1);
        assert_eq!(hist[0].manga_id, id);
        assert_eq!(hist[0].page_index, 3);

        let last_dest = db.get_last_reading_destination().await.unwrap().unwrap();
        assert_eq!(last_dest.manga_id, id);
        assert_eq!(last_dest.page_index, 3);

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
            -5,
        )
        .await
        .unwrap();
        let progress = db.get_history_for_manga(id).await.unwrap().unwrap();
        assert!((progress.scroll_ratio - 0.0).abs() < f64::EPSILON);
        assert_eq!(progress.page_index, 0);

        // Re-checking with an already-known chapter must NOT trigger a false new-chapter alert.
        let same = db.record_update_check(id, &updated).await.unwrap();
        assert!(
            same.is_empty(),
            "known latest chapter must not flag has_new"
        );

        // Adding multiple genuinely new chapters MUST return all of them in order.
        updated.chapters.push(ChapterRef {
            url: Url::parse("https://example.com/series/demo-1-chap-3").unwrap(),
            name: "Chapter 3".to_owned(),
            number: Some(3.0),
        });
        updated.chapters.push(ChapterRef {
            url: Url::parse("https://example.com/series/demo-1-chap-4").unwrap(),
            name: "Chapter 4".to_owned(),
            number: Some(4.0),
        });
        let discovered = db.record_update_check(id, &updated).await.unwrap();
        assert_eq!(discovered.len(), 2);
        assert_eq!(discovered[0].name, "Chapter 3");
        assert_eq!(discovered[1].name, "Chapter 4");
    }

    #[tokio::test]
    async fn bookmarks_priority_and_notification_deduplication_work() {
        use crate::repo::bookmark::BookmarkSort;

        let db = Database::open_in_memory().await.unwrap();
        let hist_only = Manga {
            url: Url::parse("https://example.com/series/hist-only").unwrap(),
            site: SiteId::generic("example.com"),
            title: "Alpha History".to_owned(),
            cover: None,
            chapters: vec![ChapterRef {
                url: Url::parse("https://example.com/series/hist-only-1").unwrap(),
                name: "Chapter 1".to_owned(),
                number: Some(1.0),
            }],
        };
        let bookmarked = Manga {
            url: Url::parse("https://example.com/series/fav-manga").unwrap(),
            site: SiteId::generic("example.com"),
            title: "Beta Bookmark".to_owned(),
            cover: None,
            chapters: vec![ChapterRef {
                url: Url::parse("https://example.com/series/fav-119").unwrap(),
                name: "Chapter 119".to_owned(),
                number: Some(119.0),
            }],
        };

        let id_hist = db.upsert_manga(&hist_only).await.unwrap();
        let id_fav = db.upsert_manga(&bookmarked).await.unwrap();
        db.save_history(
            id_hist,
            "https://example.com/series/hist-only-1",
            "Chapter 1",
            0.2,
            1,
        )
        .await
        .unwrap();

        assert!(db.toggle_bookmark(id_fav).await.unwrap());
        assert!(db.is_bookmarked(id_fav).await.unwrap());
        assert_eq!(
            db.list_bookmarks(BookmarkSort::Newest).await.unwrap().len(),
            1
        );

        // Bookmarked manga must appear BEFORE non-bookmarked history manga in update targets.
        let targets = db.list_targets_for_updates().await.unwrap();
        assert_eq!(targets.len(), 2);
        assert_eq!(targets[0].id, id_fav);
        assert_eq!(targets[1].id, id_hist);

        // Multi-chapter discovery must create a concise range ("120–125") and deduplicate repeats.
        let fresh_chaps: Vec<ChapterRef> = (120..=125)
            .map(|n| ChapterRef {
                url: Url::parse(&format!("https://example.com/series/fav-{n}")).unwrap(),
                name: format!("Chapter {n}"),
                number: Some(f32::from(u8::try_from(n).unwrap())),
            })
            .collect();
        let row = db.get_manga_by_id(id_fav).await.unwrap().unwrap();
        let notif = db
            .record_chapter_notification(&row, &fresh_chaps)
            .await
            .unwrap()
            .expect("first discovery must create notification");
        assert_eq!(notif.chapter_range, "120–125");
        assert_eq!(notif.chapter_count, 6);

        // Repeating the exact same chapters must be deduplicated cleanly.
        let dup = db
            .record_chapter_notification(&row, &fresh_chaps)
            .await
            .unwrap();
        assert!(
            dup.is_none(),
            "already notified chapters must be deduplicated"
        );
        assert_eq!(db.list_notifications().await.unwrap().len(), 1);

        db.mark_notification_read(notif.id).await.unwrap();
        assert_eq!(db.list_notifications().await.unwrap()[0].read, 1);

        // Per-manga reader override & detected_webtoon round-trip.
        db.set_manga_reader_override(id_fav, Some("horizontal"))
            .await
            .unwrap();
        db.mark_manga_detected_webtoon(id_fav).await.unwrap();
        let updated_row = db.get_manga_by_id(id_fav).await.unwrap().unwrap();
        assert_eq!(
            updated_row.reader_mode_override.as_deref(),
            Some("horizontal")
        );
        assert_eq!(updated_row.detected_webtoon, 1);
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
            4,
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
                // Same chapter with lower scroll ratio -> skip, but chapter 2 read flag & bookmark should union!
                SyncMangaPayload {
                    url: "https://example.com/series/same/".to_owned(),
                    site: "example.com".to_owned(),
                    title: "Same Chap".to_owned(),
                    cover_url: None,
                    latest_chapter_url: None,
                    latest_chapter_name: None,
                    bookmarked: true,
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
                        page_index: 2,
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
                    bookmarked: false,
                    chapters: Vec::new(),
                    history: Some(SyncHistoryPayload {
                        chapter_url: "https://example.com/series/inc-chap-4".to_owned(),
                        chapter_name: "Chapter 4".to_owned(),
                        scroll_ratio: 0.1,
                        page_index: 1,
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
                    bookmarked: false,
                    chapters: Vec::new(),
                    history: Some(SyncHistoryPayload {
                        chapter_url: "https://example.com/series/loc-chap-3".to_owned(),
                        chapter_name: "Chapter 3".to_owned(),
                        scroll_ratio: 0.9,
                        page_index: 9,
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
                    bookmarked: true,
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
                        page_index: 6,
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
            2,
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
            1,
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
            3,
        )
        .await
        .unwrap();

        let incoming = sample_incoming_sync_bundle();
        let report = db.merge_sync_bundle(&incoming).await.unwrap();
        assert_eq!(report.added_manga, 1);
        assert_eq!(report.updated_newer_chapter, 1);
        assert_eq!(report.skipped_same_chapter, 1);
        assert_eq!(report.kept_local_newer, 1);
        assert_eq!(report.synced_bookmarks, 2);
        assert!(db.is_bookmarked(id_same).await.unwrap());

        // Verify chapter read flags were unioned (both chap 1 and chap 2 read).
        let same_chaps = db.list_chapters(id_same).await.unwrap();
        assert_eq!(same_chaps[0].read, 1);
        assert_eq!(same_chaps[1].read, 1);

        // Verify Incoming Ahead moved to Chapter 4 while Local Ahead stayed on Chapter 5.
        let inc_h = db.get_history_for_manga(id_inc).await.unwrap().unwrap();
        assert_eq!(inc_h.chapter_url, "https://example.com/series/inc-chap-4");
        let loc_h = db.get_history_for_manga(id_loc).await.unwrap().unwrap();
        assert_eq!(loc_h.chapter_url, "https://example.com/series/loc-chap-5");

        // Verify export round-trip contains all 4 history/bookmarked manga.
        let exported = db.export_sync_bundle("TestPC").await.unwrap();
        assert_eq!(exported.manga.len(), 4);
    }

    #[tokio::test]
    async fn sync_merge_preserves_newer_latest_chapter_and_rejects_ssrf_urls() {
        use crate::repo::sync::{SyncBundle, SyncHistoryPayload, SyncMangaPayload};

        let db = Database::open_in_memory().await.unwrap();
        let manga = Manga {
            url: Url::parse("https://example.com/series/protect").unwrap(),
            site: SiteId::generic("example.com"),
            title: "Protected Series".to_owned(),
            cover: None,
            chapters: (1..=5)
                .map(|n| ChapterRef {
                    url: Url::parse(&format!("https://example.com/series/protect-chap-{n}"))
                        .unwrap(),
                    name: format!("Chapter {n}"),
                    number: Some(f32::from(u8::try_from(n).unwrap())),
                })
                .collect(),
        };
        let id = db.upsert_manga(&manga).await.unwrap();
        db.save_history(
            id,
            "https://example.com/series/protect-chap-5",
            "Chapter 5",
            0.3,
            2,
        )
        .await
        .unwrap();

        let incoming = SyncBundle {
            version: 2,
            device_name: "OldPhone".to_owned(),
            exported_at: 1000,
            manga: vec![
                // Older snapshot claiming latest_chapter_url is chap 2, but on same chap 5 with higher ratio 0.85
                SyncMangaPayload {
                    url: "https://example.com/series/protect/".to_owned(),
                    site: "example.com".to_owned(),
                    title: "Protected Series".to_owned(),
                    cover_url: None,
                    latest_chapter_url: Some(
                        "https://example.com/series/protect-chap-2".to_owned(),
                    ),
                    latest_chapter_name: Some("Chapter 2".to_owned()),
                    bookmarked: false,
                    chapters: Vec::new(),
                    history: Some(SyncHistoryPayload {
                        chapter_url: "https://example.com/series/protect-chap-5/".to_owned(),
                        chapter_name: "Chapter 5".to_owned(),
                        scroll_ratio: 0.85,
                        page_index: 7,
                        read_at: 2000,
                    }),
                },
                // Disallowed SSRF loopback URL must be rejected
                SyncMangaPayload {
                    url: "http://127.0.0.1:8080/internal".to_owned(),
                    site: "127.0.0.1".to_owned(),
                    title: "SSRF Attempt".to_owned(),
                    cover_url: None,
                    latest_chapter_url: None,
                    latest_chapter_name: None,
                    bookmarked: false,
                    chapters: Vec::new(),
                    history: None,
                },
            ],
        };

        let report = db.merge_sync_bundle(&incoming).await.unwrap();
        assert_eq!(report.added_manga, 0, "SSRF URL must be rejected");
        assert_eq!(
            report.skipped_same_chapter, 1,
            "same chapter with higher ratio must still count as skipped_same_chapter"
        );
        assert_eq!(report.updated_newer_chapter, 0);

        let row = db.get_manga_by_id(id).await.unwrap().unwrap();
        assert_eq!(
            row.latest_chapter_name.as_deref(),
            Some("Chapter 5"),
            "older incoming latest_chapter_name must not overwrite newer local Chapter 5"
        );
        let hist = db.get_history_for_manga(id).await.unwrap().unwrap();
        assert!((hist.scroll_ratio - 0.85).abs() < 1e-6);
        assert_eq!(hist.page_index, 7);
    }

    #[tokio::test]
    async fn completed_hide_delete_and_crash_recovery_work() {
        use crate::repo::manga::HideAction;

        let db = Database::open_in_memory().await.unwrap();
        let mut series = Manga {
            url: Url::parse("https://example.com/series/complete-demo").unwrap(),
            site: SiteId::generic("example.com"),
            title: "Complete Demo".to_owned(),
            cover: None,
            chapters: vec![
                ChapterRef {
                    url: Url::parse("https://example.com/series/complete-demo-1").unwrap(),
                    name: "Chapter 1".to_owned(),
                    number: Some(1.0),
                },
                ChapterRef {
                    url: Url::parse("https://example.com/series/complete-demo-2").unwrap(),
                    name: "Chapter 2".to_owned(),
                    number: Some(2.0),
                },
            ],
        };

        let id = db.upsert_manga(&series).await.unwrap();
        db.save_history(
            id,
            "https://example.com/series/complete-demo-2",
            "Chapter 2",
            0.9,
            10,
        )
        .await
        .unwrap();
        db.mark_chapter_read(id, "https://example.com/series/complete-demo-2")
            .await
            .unwrap();

        let hist = db.list_history().await.unwrap();
        assert_eq!(hist.len(), 1);
        assert_eq!(hist[0].is_completed, 1);
        assert_eq!(hist[0].unread_count, 1);

        series.chapters.push(ChapterRef {
            url: Url::parse("https://example.com/series/complete-demo-3").unwrap(),
            name: "Chapter 3".to_owned(),
            number: Some(3.0),
        });
        let fresh = db.record_update_check(id, &series).await.unwrap();
        assert_eq!(fresh.len(), 1);
        let hist_after = db.list_history().await.unwrap();
        assert_eq!(
            hist_after[0].is_completed, 0,
            "new chapter must move completed manga back to active"
        );

        db.upsert_reader_session(
            id,
            "https://example.com/series/complete-demo-2",
            "Chapter 2",
            0.55,
            6,
            "horizontal",
        )
        .await
        .unwrap();
        let rec = db.get_crash_recovery_session().await.unwrap().unwrap();
        assert_eq!(rec.manga_id, id);
        assert_eq!(rec.page_index, 6);
        assert_eq!(rec.reading_mode, "horizontal");

        db.set_manga_hidden(id, HideAction::Hide).await.unwrap();
        assert!(db.list_history().await.unwrap().is_empty());
        assert_eq!(db.list_hidden_manga().await.unwrap().len(), 1);
        db.set_manga_hidden(id, HideAction::Unhide).await.unwrap();
        assert_eq!(db.list_history().await.unwrap().len(), 1);

        db.delete_manga_permanently(id).await.unwrap();
        assert!(db.get_manga_by_id(id).await.unwrap().is_none());
        assert!(db.get_crash_recovery_session().await.unwrap().is_none());
    }

    #[tokio::test]
    async fn diagnostic_redaction_strips_secrets() {
        use crate::repo::diagnostics::{NewDiagnosticLog, redact_sensitive};

        let db = Database::open_in_memory().await.unwrap();
        let redacted = redact_sensitive(
            "failed via socks5://alice:s3cr3t@proxy.example.com:1080?token=abc123&password=xyz",
        );
        assert!(
            !redacted.contains("s3cr3t"),
            "password in URL must be redacted"
        );
        assert!(!redacted.contains("abc123"), "token must be redacted");
        assert!(
            !redacted.contains("xyz"),
            "password query param must be redacted"
        );

        db.record_diagnostic_log(&NewDiagnosticLog {
            level: "error".to_owned(),
            category: "timeout".to_owned(),
            message: "Timed out".to_owned(),
            detail: Some("token=secret999".to_owned()),
            source: Some("example.com".to_owned()),
            chapter_url: None,
            stack_trace: None,
        })
        .await
        .unwrap();
        let logs = db.list_diagnostic_logs(10).await.unwrap();
        assert_eq!(logs.len(), 1);
        assert!(
            !logs[0]
                .detail
                .as_deref()
                .unwrap_or("")
                .contains("secret999")
        );
    }

    #[tokio::test]
    async fn scoped_cache_clear_and_settings_reset_preserve_user_data() {
        use crate::cache::ImageCache;
        use crate::cache::image::CacheEntryMeta;
        use crate::settings::SettingsSection;

        let db = Database::open_in_memory().await.unwrap();
        let manga = Manga {
            url: Url::parse("https://example.com/series/cache-demo").unwrap(),
            site: SiteId::generic("example.com"),
            title: "Cache Demo".to_owned(),
            cover: None,
            chapters: vec![ChapterRef {
                url: Url::parse("https://example.com/series/cache-demo-1").unwrap(),
                name: "Chapter 1".to_owned(),
                number: Some(1.0),
            }],
        };
        let id = db.upsert_manga(&manga).await.unwrap();
        db.save_history(
            id,
            "https://example.com/series/cache-demo-1",
            "Chapter 1",
            0.2,
            1,
        )
        .await
        .unwrap();

        let dir =
            std::env::temp_dir().join(format!("rtmaga_scoped_cache_{}", rand::random::<u64>()));
        let cache = ImageCache::new(dir.clone(), db.clone()).await.unwrap();
        let payload = bytes::Bytes::from_static(&[0x89, 0x50, 0x4E, 0x47, 0x00, 0x01]);
        cache
            .put(
                &ImageCache::key_for_url("https://cdn.example.com/1.png"),
                &payload,
                "image/png",
                512,
                CacheEntryMeta {
                    site: Some("example.com"),
                    manga_id: Some(id),
                },
            )
            .await;
        cache
            .put(
                &ImageCache::key_for_url("https://cdn.other.org/2.png"),
                &payload,
                "image/png",
                512,
                CacheEntryMeta {
                    site: Some("other.org"),
                    manga_id: None,
                },
            )
            .await;
        assert_eq!(cache.list_by_source().await.unwrap().len(), 2);
        assert_eq!(cache.list_by_manga().await.unwrap().len(), 1);
        let after_manga_clear = cache.clear_by_manga(id).await.unwrap();
        assert_eq!(after_manga_clear.files, 1);
        assert_eq!(
            db.list_history().await.unwrap().len(),
            1,
            "history must survive cache clear"
        );
        let after_src_clear = cache.clear_by_source("other.org").await.unwrap();
        assert_eq!(after_src_clear.files, 0);

        let mut s = Settings {
            theme: Theme::Dark,
            ..Settings::default()
        };
        s.sources.speed_limit_kbps = 2048;
        s.reset_section(SettingsSection::Sources);
        assert_eq!(s.sources.speed_limit_kbps, 0);
        assert_eq!(s.theme, Theme::Dark);

        // Pre-existing cache row with NULL site/manga_id must be backfilled on cache hit.
        let unattributed_key = ImageCache::key_for_url("https://cdn.example.com/unattributed.png");
        cache
            .put(
                &unattributed_key,
                &payload,
                "image/png",
                512,
                CacheEntryMeta::default(),
            )
            .await;
        let hit = cache
            .get(
                &unattributed_key,
                CacheEntryMeta {
                    site: Some("example.com"),
                    manga_id: Some(id),
                },
            )
            .await;
        assert!(hit.is_some());
        assert_eq!(cache.list_by_manga().await.unwrap().len(), 1);

        let _ = tokio::fs::remove_dir_all(&dir).await;
    }

    #[tokio::test]
    async fn single_chapter_fallback_preserves_multi_chapter_metadata_and_read_flags() {
        let db = Database::open_in_memory().await.unwrap();
        let multi = Manga {
            url: Url::parse("https://example.com/series/multi-guard").unwrap(),
            site: SiteId::generic("example.com"),
            title: "Original Series Title".to_owned(),
            cover: None,
            chapters: vec![
                ChapterRef {
                    url: Url::parse("https://example.com/series/multi-guard/chap-1").unwrap(),
                    name: "Chapter 1".to_owned(),
                    number: Some(1.0),
                },
                ChapterRef {
                    url: Url::parse("https://example.com/series/multi-guard/chap-2").unwrap(),
                    name: "Chapter 2".to_owned(),
                    number: Some(2.0),
                },
            ],
        };
        let id = db.upsert_manga(&multi).await.unwrap();
        db.mark_chapter_read(id, "https://example.com/series/multi-guard/chap-1")
            .await
            .unwrap();

        let single_fallback = Manga {
            url: Url::parse("https://example.com/series/multi-guard").unwrap(),
            site: SiteId::generic("example.com"),
            title: "Original Series Title - Chapter 1".to_owned(),
            cover: None,
            chapters: vec![ChapterRef {
                url: Url::parse("https://example.com/series/multi-guard/chap-1/").unwrap(),
                name: "Chapter 1".to_owned(),
                number: Some(1.0),
            }],
        };
        db.upsert_manga_with_id(Some(id), &single_fallback)
            .await
            .unwrap();
        let row = db.get_manga_by_id(id).await.unwrap().unwrap();
        assert_eq!(row.title, "Original Series Title");
        assert_eq!(row.latest_chapter_name.as_deref(), Some("Chapter 2"));
        let chaps = db.list_chapters(id).await.unwrap();
        assert_eq!(chaps.len(), 2);
        assert_eq!(
            chaps[0].read, 1,
            "trailing-slash variant must preserve read=1"
        );
    }

    #[tokio::test]
    async fn hidden_isolation_and_last_reading_skip_completed() {
        use crate::repo::manga::HideAction;

        let db = Database::open_in_memory().await.unwrap();
        let active_series = Manga {
            url: Url::parse("https://example.com/series/active-guard").unwrap(),
            site: SiteId::generic("example.com"),
            title: "Active Guard".to_owned(),
            cover: None,
            chapters: vec![
                ChapterRef {
                    url: Url::parse("https://example.com/series/active-guard/chap-1").unwrap(),
                    name: "Chapter 1".to_owned(),
                    number: Some(1.0),
                },
                ChapterRef {
                    url: Url::parse("https://example.com/series/active-guard/chap-2").unwrap(),
                    name: "Chapter 2".to_owned(),
                    number: Some(2.0),
                },
            ],
        };
        let id = db.upsert_manga(&active_series).await.unwrap();
        db.save_history(
            id,
            "https://example.com/series/active-guard/chap-1",
            "Chapter 1",
            0.4,
            2,
        )
        .await
        .unwrap();

        let completed_series = Manga {
            url: Url::parse("https://example.com/series/done-series").unwrap(),
            site: SiteId::generic("example.com"),
            title: "Done Series".to_owned(),
            cover: None,
            chapters: vec![ChapterRef {
                url: Url::parse("https://example.com/series/done-series/chap-1").unwrap(),
                name: "Chapter 1".to_owned(),
                number: Some(1.0),
            }],
        };
        let done_id = db.upsert_manga(&completed_series).await.unwrap();
        db.save_history(
            done_id,
            "https://example.com/series/done-series/chap-1",
            "Chapter 1",
            1.0,
            10,
        )
        .await
        .unwrap();
        db.mark_chapter_read(done_id, "https://example.com/series/done-series/chap-1")
            .await
            .unwrap();

        let dest = db.get_last_reading_destination().await.unwrap().unwrap();
        assert_eq!(dest.manga_id, id);

        let row = db.get_manga_by_id(id).await.unwrap().unwrap();
        db.record_chapter_notification(&row, &active_series.chapters[1..])
            .await
            .unwrap();
        db.upsert_reader_session(
            id,
            "https://example.com/series/active-guard/chap-1",
            "Chapter 1",
            0.4,
            2,
            "vertical",
        )
        .await
        .unwrap();
        db.set_manga_hidden(id, HideAction::Hide).await.unwrap();
        assert!(db.get_crash_recovery_session().await.unwrap().is_none());
        assert!(
            db.list_targets_for_updates()
                .await
                .unwrap()
                .iter()
                .all(|m| m.id != id)
        );
        assert!(db.list_notifications().await.unwrap().is_empty());
        assert!(
            db.export_sync_bundle("PC")
                .await
                .unwrap()
                .manga
                .iter()
                .all(|m| m.title != "Active Guard")
        );
    }

    #[tokio::test]
    async fn get_last_reading_destination_ignores_removed_history() {
        let db = Database::open_in_memory().await.unwrap();
        let manga = Manga {
            url: Url::parse("https://example.com/series/removed-hist").unwrap(),
            site: SiteId::generic("example.com"),
            title: "Removed History Series".to_owned(),
            cover: None,
            chapters: vec![
                ChapterRef {
                    url: Url::parse("https://example.com/series/removed-hist/chap-1").unwrap(),
                    name: "Chapter 1".to_owned(),
                    number: Some(1.0),
                },
                ChapterRef {
                    url: Url::parse("https://example.com/series/removed-hist/chap-2").unwrap(),
                    name: "Chapter 2".to_owned(),
                    number: Some(2.0),
                },
            ],
        };
        let id = db.upsert_manga(&manga).await.unwrap();
        db.save_history(
            id,
            "https://example.com/series/removed-hist/chap-1",
            "Chapter 1",
            0.3,
            2,
        )
        .await
        .unwrap();
        assert_eq!(
            db.get_last_reading_destination()
                .await
                .unwrap()
                .map(|d| d.manga_id),
            Some(id)
        );

        db.remove_history(id).await.unwrap();
        assert!(
            db.get_last_reading_destination().await.unwrap().is_none(),
            "removed history item must not be resurrected by LAST_READING_KEY"
        );
    }
}
