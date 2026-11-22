//! SQLite repository layer backed by `sqlx` in WAL mode.

pub(crate) mod history;
pub(crate) mod manga;
pub(crate) mod settings;

use std::path::Path;
use std::time::Duration;

use sqlx::SqlitePool;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};

use crate::error::AppError;

/// Shared SQLite database handle.
#[derive(Debug, Clone)]
pub(crate) struct Database {
    pool: SqlitePool,
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

        Self::migrate_and_wrap(pool).await
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

        Self::migrate_and_wrap(pool).await
    }

    async fn migrate_and_wrap(pool: SqlitePool) -> Result<Self, AppError> {
        sqlx::migrate!("./migrations")
            .run(&pool)
            .await
            .map_err(|e| AppError::Db {
                message: format!("migration failed: {e}"),
            })?;
        Ok(Self { pool })
    }

    /// Returns a reference to the underlying [`SqlitePool`].
    pub(crate) fn pool(&self) -> &SqlitePool {
        &self.pool
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
}
