//! Database operations for reading `history`.

use sqlx::FromRow;

use crate::error::AppError;
use crate::repo::{Database, now_unix_secs};

/// Joined reading history record returned to the UI.
#[derive(Debug, Clone, FromRow)]
pub(crate) struct HistoryEntryRow {
    pub manga_id: i64,
    pub manga_url: String,
    pub site: String,
    pub title: String,
    pub cover_url: Option<String>,
    pub chapter_url: String,
    pub chapter_name: String,
    pub scroll_ratio: f64,
    pub read_at: i64,
    pub latest_chapter_url: Option<String>,
    pub latest_chapter_name: Option<String>,
    pub has_new: i64,
    pub total_chapters: i64,
    pub current_chapter_idx: Option<i64>,
}

impl Database {
    /// Upserts reading progress for a manga into `history`.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Db`] on SQLite failure.
    pub(crate) async fn save_history(
        &self,
        manga_id: i64,
        chapter_url: &str,
        chapter_name: &str,
        scroll_ratio: f64,
    ) -> Result<(), AppError> {
        let now = now_unix_secs();
        let clamped = scroll_ratio.clamp(0.0, 1.0);

        sqlx::query(
            "INSERT INTO history (manga_id, chapter_url, chapter_name, scroll_ratio, read_at)
             VALUES (?, ?, ?, ?, ?)
             ON CONFLICT(manga_id) DO UPDATE SET
                chapter_url = excluded.chapter_url,
                chapter_name = excluded.chapter_name,
                scroll_ratio = excluded.scroll_ratio,
                read_at = excluded.read_at",
        )
        .bind(manga_id)
        .bind(chapter_url)
        .bind(chapter_name)
        .bind(clamped)
        .bind(now)
        .execute(self.pool())
        .await?;

        Ok(())
    }

    /// Loads reading history ordered by most recently read first.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Db`] on SQLite failure.
    pub(crate) async fn list_history(&self) -> Result<Vec<HistoryEntryRow>, AppError> {
        let rows = sqlx::query_as::<_, HistoryEntryRow>(
            "SELECT
                h.manga_id,
                m.url AS manga_url,
                m.site,
                m.title,
                m.cover_url,
                h.chapter_url,
                h.chapter_name,
                h.scroll_ratio,
                h.read_at,
                m.latest_chapter_url,
                m.latest_chapter_name,
                m.has_new,
                (SELECT COUNT(*) FROM chapter c WHERE c.manga_id = m.id) AS total_chapters,
                (SELECT c.idx FROM chapter c WHERE c.manga_id = m.id AND c.url = h.chapter_url LIMIT 1) AS current_chapter_idx
             FROM history h
             INNER JOIN manga m ON m.id = h.manga_id
             ORDER BY h.read_at DESC",
        )
        .fetch_all(self.pool())
        .await?;
        Ok(rows)
    }

    /// Loads the saved history entry for a single manga, if present.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Db`] on SQLite failure.
    pub(crate) async fn get_history_for_manga(
        &self,
        manga_id: i64,
    ) -> Result<Option<(String, String, f64, i64)>, AppError> {
        let row = sqlx::query_as::<_, (String, String, f64, i64)>(
            "SELECT chapter_url, chapter_name, scroll_ratio, read_at FROM history WHERE manga_id = ?",
        )
        .bind(manga_id)
        .fetch_optional(self.pool())
        .await?;
        Ok(row)
    }

    /// Removes a manga from reading history and clears its `has_new` flag so it stops being checked.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Db`] on SQLite failure.
    pub(crate) async fn remove_history(&self, manga_id: i64) -> Result<(), AppError> {
        sqlx::query("DELETE FROM history WHERE manga_id = ?")
            .bind(manga_id)
            .execute(self.pool())
            .await?;
        sqlx::query("UPDATE manga SET has_new = 0 WHERE id = ?")
            .bind(manga_id)
            .execute(self.pool())
            .await?;
        Ok(())
    }

    /// Clears the entire reading history and resets all `has_new` flags.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Db`] on SQLite failure.
    pub(crate) async fn clear_history(&self) -> Result<(), AppError> {
        sqlx::query("DELETE FROM history")
            .execute(self.pool())
            .await?;
        sqlx::query("UPDATE manga SET has_new = 0")
            .execute(self.pool())
            .await?;
        Ok(())
    }
}
