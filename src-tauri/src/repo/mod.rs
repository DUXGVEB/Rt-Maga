//! SQLite repository layer backed by `sqlx` in WAL mode.

pub(crate) mod history;
pub(crate) mod manga;
pub(crate) mod settings;

use std::path::Path;

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
            url: Url::parse("https://truyenqqko.com/truyen-tranh/demo-1").unwrap(),
            site: SiteId::known("truyenqq"),
            title: "Demo Manga".to_owned(),
            cover: Some(Url::parse("https://i.hinhhinh.com/demo.jpg").unwrap()),
            chapters: vec![
                ChapterRef {
                    url: Url::parse("https://truyenqqko.com/truyen-tranh/demo-1-chap-1").unwrap(),
                    name: "Chương 1".to_owned(),
                    number: Some(1.0),
                },
                ChapterRef {
                    url: Url::parse("https://truyenqqko.com/truyen-tranh/demo-1-chap-2").unwrap(),
                    name: "Chương 2".to_owned(),
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
            "https://truyenqqko.com/truyen-tranh/demo-1-chap-1",
            "Chương 1",
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
    }
}
