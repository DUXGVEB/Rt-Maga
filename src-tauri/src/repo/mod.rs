//! SQLite repository layer backed by `sqlx` in WAL mode.

pub(crate) mod bookmark;
pub(crate) mod diagnostics;
pub(crate) mod history;
pub(crate) mod manga;
pub(crate) mod notification;
pub(crate) mod session;
pub(crate) mod settings;
pub(crate) mod sources;
pub(crate) mod sync;

use std::path::{Path, PathBuf};
use std::time::Duration;

use sha2::{Digest, Sha384};
use sqlx::SqlitePool;
use sqlx::migrate::Migrator;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};

use crate::error::AppError;

static MIGRATOR: Migrator = sqlx::migrate!("./migrations");

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
        Self::reconcile_line_ending_checksums(&pool).await?;
        MIGRATOR.run(&pool).await.map_err(|e| AppError::Db {
            message: format!("migration failed: {e}"),
        })?;
        Ok(Self { pool, db_path })
    }

    /// Normalizes `_sqlx_migrations` SHA-384 checksums when a previously applied migration differs
    /// strictly by `CRLF` (`\r\n`) vs `LF` (`\n`) line endings across OS/CI checkouts, while
    /// preserving `VersionMismatch` rejection for any genuine SQL content modification.
    async fn reconcile_line_ending_checksums(pool: &SqlitePool) -> Result<(), AppError> {
        let has_table: Option<(i64,)> = sqlx::query_as(
            "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = '_sqlx_migrations' LIMIT 1",
        )
        .fetch_optional(pool)
        .await?;
        if has_table.is_none() {
            return Ok(());
        }

        let applied: Vec<(i64, Vec<u8>)> = sqlx::query_as(
            "SELECT version, checksum FROM _sqlx_migrations WHERE success = 1 ORDER BY version",
        )
        .fetch_all(pool)
        .await?;

        for (version, stored_checksum) in applied {
            let Some(migration) = MIGRATOR.iter().find(|m| m.version == version) else {
                continue;
            };
            let expected = migration.checksum.as_ref();
            if stored_checksum.as_slice() == expected {
                continue;
            }

            let lf_sql = migration.sql.replace("\r\n", "\n");
            let crlf_sql = lf_sql.replace('\n', "\r\n");
            let lf_digest = Sha384::digest(lf_sql.as_bytes());
            let crlf_digest = Sha384::digest(crlf_sql.as_bytes());

            if stored_checksum.as_slice() == lf_digest.as_slice()
                || stored_checksum.as_slice() == crlf_digest.as_slice()
            {
                sqlx::query("UPDATE _sqlx_migrations SET checksum = ? WHERE version = ?")
                    .bind(expected)
                    .bind(version)
                    .execute(pool)
                    .await?;
            }
        }

        Ok(())
    }

    /// Returns a reference to the underlying [`SqlitePool`].
    pub(crate) fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    /// Truncates the SQLite WAL file back into the main database file to keep disk footprint lean.
    pub(crate) async fn checkpoint_wal_truncate(&self) {
        let _ = sqlx::query("PRAGMA wal_checkpoint(TRUNCATE)")
            .execute(&self.pool)
            .await;
        let _ = sqlx::query("PRAGMA optimize").execute(&self.pool).await;
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

    fn ch(url: &str, name: &str, number: Option<f32>) -> ChapterRef {
        ChapterRef::simple(Url::parse(url).unwrap(), name.to_owned(), number)
    }

    #[tokio::test]
    async fn round_trips_manga_history_and_settings() {
        let db = Database::open_in_memory().await.unwrap();
        let manga = Manga::minimal(
            Url::parse("https://example.com/series/demo-1").unwrap(),
            SiteId::generic("example.com"),
            "Demo Manga".to_owned(),
            Some(Url::parse("https://cdn.example.com/demo.jpg").unwrap()),
            vec![
                ch(
                    "https://example.com/series/demo-1-chap-1",
                    "Chapter 1",
                    Some(1.0),
                ),
                ch(
                    "https://example.com/series/demo-1-chap-2",
                    "Chapter 2",
                    Some(2.0),
                ),
            ],
        );

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
        updated.chapters.push(ch(
            "https://example.com/series/demo-1-chap-3",
            "Chapter 3",
            Some(3.0),
        ));
        updated.chapters.push(ch(
            "https://example.com/series/demo-1-chap-4",
            "Chapter 4",
            Some(4.0),
        ));
        let discovered = db.record_update_check(id, &updated).await.unwrap();
        assert_eq!(discovered.len(), 2);
        assert_eq!(discovered[0].name, "Chapter 3");
        assert_eq!(discovered[1].name, "Chapter 4");
    }

    #[tokio::test]
    async fn bookmarks_priority_and_notification_deduplication_work() {
        use crate::repo::bookmark::BookmarkSort;

        let db = Database::open_in_memory().await.unwrap();
        let hist_only = Manga::minimal(
            Url::parse("https://example.com/series/hist-only").unwrap(),
            SiteId::generic("example.com"),
            "Alpha History".to_owned(),
            None,
            vec![ch(
                "https://example.com/series/hist-only-1",
                "Chapter 1",
                Some(1.0),
            )],
        );
        let bookmarked = Manga::minimal(
            Url::parse("https://example.com/series/fav-manga").unwrap(),
            SiteId::generic("example.com"),
            "Beta Bookmark".to_owned(),
            None,
            vec![ch(
                "https://example.com/series/fav-119",
                "Chapter 119",
                Some(119.0),
            )],
        );

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
            .map(|n| {
                ChapterRef::simple(
                    Url::parse(&format!("https://example.com/series/fav-{n}")).unwrap(),
                    format!("Chapter {n}"),
                    Some(f32::from(u8::try_from(n).unwrap())),
                )
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
        let manga = Manga::minimal(
            Url::parse("https://example.com/series/backup-demo").unwrap(),
            SiteId::generic("example.com"),
            "Backup Demo".to_owned(),
            None,
            vec![ch(
                "https://example.com/series/backup-demo-chap-1",
                "Chapter 1",
                Some(1.0),
            )],
        );
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

        reopened.close().await;
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

        let mk_manga = |slug: &str, title: &str| {
            Manga::minimal(
                Url::parse(&format!("https://example.com/series/{slug}")).unwrap(),
                SiteId::generic("example.com"),
                title.to_owned(),
                None,
                (1..=5)
                    .map(|n| {
                        ChapterRef::simple(
                            Url::parse(&format!("https://example.com/series/{slug}-chap-{n}"))
                                .unwrap(),
                            format!("Chapter {n}"),
                            Some(f32::from(u8::try_from(n).unwrap())),
                        )
                    })
                    .collect(),
            )
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
        let manga = Manga::minimal(
            Url::parse("https://example.com/series/protect").unwrap(),
            SiteId::generic("example.com"),
            "Protected Series".to_owned(),
            None,
            (1..=5)
                .map(|n| {
                    ChapterRef::simple(
                        Url::parse(&format!("https://example.com/series/protect-chap-{n}"))
                            .unwrap(),
                        format!("Chapter {n}"),
                        Some(f32::from(u8::try_from(n).unwrap())),
                    )
                })
                .collect(),
        );
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
        let mut series = Manga::minimal(
            Url::parse("https://example.com/series/complete-demo").unwrap(),
            SiteId::generic("example.com"),
            "Complete Demo".to_owned(),
            None,
            vec![
                ch(
                    "https://example.com/series/complete-demo-1",
                    "Chapter 1",
                    Some(1.0),
                ),
                ch(
                    "https://example.com/series/complete-demo-2",
                    "Chapter 2",
                    Some(2.0),
                ),
            ],
        );

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

        series.chapters.push(ch(
            "https://example.com/series/complete-demo-3",
            "Chapter 3",
            Some(3.0),
        ));
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
            ..NewDiagnosticLog::default()
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
        let manga = Manga::minimal(
            Url::parse("https://example.com/series/cache-demo").unwrap(),
            SiteId::generic("example.com"),
            "Cache Demo".to_owned(),
            None,
            vec![ch(
                "https://example.com/series/cache-demo-1",
                "Chapter 1",
                Some(1.0),
            )],
        );
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
                    ..CacheEntryMeta::default()
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
                    ..CacheEntryMeta::default()
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
                    ..CacheEntryMeta::default()
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
        let multi = Manga::minimal(
            Url::parse("https://example.com/series/multi-guard").unwrap(),
            SiteId::generic("example.com"),
            "Original Series Title".to_owned(),
            None,
            vec![
                ch(
                    "https://example.com/series/multi-guard/chap-1",
                    "Chapter 1",
                    Some(1.0),
                ),
                ch(
                    "https://example.com/series/multi-guard/chap-2",
                    "Chapter 2",
                    Some(2.0),
                ),
            ],
        );
        let id = db.upsert_manga(&multi).await.unwrap();
        db.mark_chapter_read(id, "https://example.com/series/multi-guard/chap-1")
            .await
            .unwrap();

        let single_fallback = Manga::minimal(
            Url::parse("https://example.com/series/multi-guard").unwrap(),
            SiteId::generic("example.com"),
            "Original Series Title - Chapter 1".to_owned(),
            None,
            vec![ch(
                "https://example.com/series/multi-guard/chap-1/",
                "Chapter 1",
                Some(1.0),
            )],
        );
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
        let active_series = Manga::minimal(
            Url::parse("https://example.com/series/active-guard").unwrap(),
            SiteId::generic("example.com"),
            "Active Guard".to_owned(),
            None,
            vec![
                ch(
                    "https://example.com/series/active-guard/chap-1",
                    "Chapter 1",
                    Some(1.0),
                ),
                ch(
                    "https://example.com/series/active-guard/chap-2",
                    "Chapter 2",
                    Some(2.0),
                ),
            ],
        );
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

        let completed_series = Manga::minimal(
            Url::parse("https://example.com/series/done-series").unwrap(),
            SiteId::generic("example.com"),
            "Done Series".to_owned(),
            None,
            vec![ch(
                "https://example.com/series/done-series/chap-1",
                "Chapter 1",
                Some(1.0),
            )],
        );
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
        let manga = Manga::minimal(
            Url::parse("https://example.com/series/removed-hist").unwrap(),
            SiteId::generic("example.com"),
            "Removed History Series".to_owned(),
            None,
            vec![
                ch(
                    "https://example.com/series/removed-hist/chap-1",
                    "Chapter 1",
                    Some(1.0),
                ),
                ch(
                    "https://example.com/series/removed-hist/chap-2",
                    "Chapter 2",
                    Some(2.0),
                ),
            ],
        );
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

    #[tokio::test]
    async fn source_id_prevents_duplicates_without_merging_similar_titles() {
        let db = Database::open_in_memory().await.unwrap();
        let mut first = Manga::minimal(
            Url::parse("https://example.com/truyen-tranh/alpha-100").unwrap(),
            SiteId::known("truyenqq"),
            "Alpha Tale".to_owned(),
            None,
            vec![ch(
                "https://example.com/truyen-tranh/alpha-100-chap-1",
                "Ch 1",
                Some(1.0),
            )],
        );
        first.source_id = Some("truyenqq:100".to_owned());
        first.authors = vec!["Author A".to_owned()];

        let mut renamed_url = Manga::minimal(
            Url::parse("https://example.com/truyen-tranh/alpha-renamed-100").unwrap(),
            SiteId::known("truyenqq"),
            "Alpha Tale".to_owned(),
            None,
            vec![
                ch(
                    "https://example.com/truyen-tranh/alpha-100-chap-1",
                    "Ch 1",
                    Some(1.0),
                ),
                ch(
                    "https://example.com/truyen-tranh/alpha-100-chap-2",
                    "Ch 2",
                    Some(2.0),
                ),
            ],
        );
        renamed_url.source_id = Some("truyenqq:100".to_owned());

        let mut distinct_same_title = Manga::minimal(
            Url::parse("https://example.com/truyen-tranh/alpha-200").unwrap(),
            SiteId::known("truyenqq"),
            "Alpha Tale".to_owned(),
            None,
            vec![ch(
                "https://example.com/truyen-tranh/alpha-200-chap-1",
                "Ch 1",
                Some(1.0),
            )],
        );
        distinct_same_title.source_id = Some("truyenqq:200".to_owned());

        let id1 = db.upsert_manga(&first).await.unwrap();
        let id2 = db.upsert_manga(&renamed_url).await.unwrap();
        let id3 = db.upsert_manga(&distinct_same_title).await.unwrap();

        assert_eq!(
            id1, id2,
            "same source_id must deduplicate across URL changes"
        );
        assert_ne!(
            id1, id3,
            "different source_id must NOT be merged merely because titles match"
        );
        let stored = db.get_manga_by_id(id1).await.unwrap().unwrap();
        assert_eq!(stored.authors_json.as_deref(), Some("[\"Author A\"]"));
    }

    #[tokio::test]
    async fn proxy_secret_created_at_and_path_scrubbing_work() {
        use crate::repo::diagnostics::scrub_personal_paths;

        let db = Database::open_in_memory().await.unwrap();
        let key1 = db.load_or_init_proxy_secret().await.unwrap();
        let key2 = db.load_or_init_proxy_secret().await.unwrap();
        assert_eq!(
            key1, key2,
            "cover proxy secret must persist across restarts"
        );

        let mut adult_manga = Manga::minimal(
            Url::parse("https://adult-gallery.example/g/1").unwrap(),
            SiteId::generic("adult-gallery.example"),
            "Sample Work".to_owned(),
            None,
            vec![ch(
                "https://adult-gallery.example/g/1/ch-1",
                "Ch 1",
                Some(1.0),
            )],
        );
        adult_manga.tags = vec![
            "Sci-Fi".to_owned(),
            "sci-fi".to_owned(),
            "Action".to_owned(),
        ];
        let adult_id = db.upsert_manga(&adult_manga).await.unwrap();
        let adult_row = db.get_manga_by_id(adult_id).await.unwrap().unwrap();
        assert!(
            adult_row.created_at.is_some(),
            "created_at must populate on insert"
        );
        assert_eq!(
            adult_row.tags_json.as_deref(),
            Some("[\"Sci-Fi\",\"Action\"]"),
            "duplicate tags must compact case-insensitively"
        );

        let scrubbed =
            scrub_personal_paths(r"failed opening C:\Users\{user}\AppData\Local\RtMaga\log.txt");
        assert!(!scrubbed.contains("{user}"));
        assert!(scrubbed.contains("<USER_HOME>\\AppData"));
    }

    #[tokio::test]
    async fn saved_source_visibility_isolation_and_manual_pin_work() {
        use crate::repo::sources::{
            SourcePlacement, UpdateSavedSource, UpsertSavedSource, VisibilityFilter,
            VisibilityPatch,
        };

        let db = Database::open_in_memory().await.unwrap();
        let _main = db
            .upsert_discovered_source(&UpsertSavedSource {
                source_key: "host:main-comics.example",
                display_name: "Main Comics",
                base_url: "https://main-comics.example",
                source_kind: "madara",
                parser_ref: None,
                search_template: None,
                icon_url: None,
                detected_adult: false,
                placement: SourcePlacement::Auto,
                prefer_new_name: false,
            })
            .await
            .unwrap();

        let supp = db
            .upsert_discovered_source(&UpsertSavedSource {
                source_key: "host:adult-gallery.example",
                display_name: "Adult Gallery",
                base_url: "https://adult-gallery.example",
                source_kind: "flat_gallery",
                parser_ref: None,
                search_template: None,
                icon_url: None,
                detected_adult: true,
                placement: SourcePlacement::Auto,
                prefer_new_name: false,
            })
            .await
            .unwrap();

        assert_eq!(
            db.list_saved_sources(VisibilityFilter::MainOnly)
                .await
                .unwrap()
                .len(),
            1
        );
        let supp_list = db
            .list_saved_sources(VisibilityFilter::SupplementalOnly)
            .await
            .unwrap();
        assert_eq!(supp_list.len(), 1);
        assert_eq!(supp_list[0].id, supp.id);

        // User moves supplemental source into Main -> manual_visibility = 1 protects it from auto re-hiding.
        let moved = db
            .update_saved_source(
                supp.id,
                &UpdateSavedSource {
                    display_name: Some("Pinned Gallery"),
                    base_url: Some("https://mirror-gallery.example/sub"),
                    is_enabled: Some(true),
                    visibility: VisibilityPatch::MoveToMain,
                },
            )
            .await
            .unwrap();
        assert_eq!(moved.is_supplemental, 0);
        assert_eq!(moved.manual_visibility, 1);
        assert_eq!(moved.base_url, "https://mirror-gallery.example");

        let re_upserted = db
            .upsert_discovered_source(&UpsertSavedSource {
                source_key: "host:adult-gallery.example",
                display_name: "Other",
                base_url: "https://adult-gallery.example",
                source_kind: "flat_gallery",
                parser_ref: None,
                search_template: None,
                icon_url: None,
                detected_adult: true,
                placement: SourcePlacement::Auto,
                prefer_new_name: false,
            })
            .await
            .unwrap();
        assert_eq!(
            re_upserted.is_supplemental, 0,
            "manual visibility pin must hold"
        );
        assert_eq!(re_upserted.display_name, "Pinned Gallery");
    }

    #[tokio::test]
    async fn reconciles_crlf_and_lf_migration_checksums_without_data_loss() {
        let dir =
            std::env::temp_dir().join(format!("rtmaga_db_eol_test_{}", rand::random::<u64>()));
        let db_path = dir.join("rtmaga.db");

        let db = Database::open(&db_path).await.unwrap();
        let manga = Manga::minimal(
            Url::parse("https://example.com/series/eol-demo").unwrap(),
            SiteId::generic("example.com"),
            "EOL Upgrade Demo".to_owned(),
            None,
            vec![ch(
                "https://example.com/series/eol-demo-chap-1",
                "Chapter 1",
                Some(1.0),
            )],
        );
        let id = db.upsert_manga(&manga).await.unwrap();
        db.save_history(
            id,
            "https://example.com/series/eol-demo-chap-1",
            "Chapter 1",
            0.88,
            5,
        )
        .await
        .unwrap();

        // Rewrite all applied migration checksums to their CRLF SHA-384 equivalents (simulating
        // a database touched by a Windows `core.autocrlf=true` build).
        for migration in MIGRATOR.iter() {
            let crlf_sql = migration.sql.replace("\r\n", "\n").replace('\n', "\r\n");
            let crlf_digest = Sha384::digest(crlf_sql.as_bytes());
            sqlx::query("UPDATE _sqlx_migrations SET checksum = ? WHERE version = ?")
                .bind(crlf_digest.as_slice())
                .bind(migration.version)
                .execute(db.pool())
                .await
                .unwrap();
        }
        db.close().await;

        // Re-opening must succeed transparently and preserve all user reading history.
        let reopened_from_crlf = Database::open(&db_path).await.unwrap();
        let hist = reopened_from_crlf.list_history().await.unwrap();
        assert_eq!(hist.len(), 1);
        assert_eq!(hist[0].title, "EOL Upgrade Demo");
        assert_eq!(hist[0].page_index, 5);

        // Now rewrite checksums to LF SHA-384 equivalents and verify reopening also succeeds.
        for migration in MIGRATOR.iter() {
            let lf_sql = migration.sql.replace("\r\n", "\n");
            let lf_digest = Sha384::digest(lf_sql.as_bytes());
            sqlx::query("UPDATE _sqlx_migrations SET checksum = ? WHERE version = ?")
                .bind(lf_digest.as_slice())
                .bind(migration.version)
                .execute(reopened_from_crlf.pool())
                .await
                .unwrap();
        }
        reopened_from_crlf.close().await;

        let reopened_from_lf = Database::open(&db_path).await.unwrap();
        assert_eq!(reopened_from_lf.list_history().await.unwrap().len(), 1);
        reopened_from_lf.close().await;

        let _ = tokio::fs::remove_dir_all(&dir).await;
    }

    #[tokio::test]
    async fn rejects_genuinely_tampered_migration_checksum() {
        let dir =
            std::env::temp_dir().join(format!("rtmaga_db_tamper_test_{}", rand::random::<u64>()));
        let db_path = dir.join("rtmaga.db");

        let db = Database::open(&db_path).await.unwrap();
        let bogus_digest = Sha384::digest(b"DROP TABLE manga; -- tampered migration");
        sqlx::query("UPDATE _sqlx_migrations SET checksum = ? WHERE version = 1")
            .bind(bogus_digest.as_slice())
            .execute(db.pool())
            .await
            .unwrap();
        db.close().await;

        let err = Database::open(&db_path)
            .await
            .expect_err("genuinely modified migration checksum must still be rejected");
        assert!(
            err.to_string().contains("migration failed"),
            "unexpected error: {err}"
        );

        let _ = tokio::fs::remove_dir_all(&dir).await;
    }
}
