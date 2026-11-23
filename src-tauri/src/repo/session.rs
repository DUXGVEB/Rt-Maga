//! Crash-recovery reader session checkpointing in SQLite.

use serde::{Deserialize, Serialize};
use sqlx::FromRow;

use crate::error::AppError;
use crate::repo::{Database, now_unix_secs};

/// Recoverable reader session returned on launch if the previous reading session did not close cleanly.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CrashRecoverySession {
    pub manga_id: i64,
    pub manga_title: String,
    pub chapter_url: String,
    pub chapter_name: String,
    pub scroll_ratio: f64,
    pub page_index: i64,
    pub reading_mode: String,
    pub updated_at: i64,
}

impl Database {
    /// Marks a reading session as active (`active = 1`) with exact chapter, page, and reading mode.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Db`] on SQLite failure.
    pub(crate) async fn upsert_reader_session(
        &self,
        manga_id: i64,
        chapter_url: &str,
        chapter_name: &str,
        scroll_ratio: f64,
        page_index: i64,
        reading_mode: &str,
    ) -> Result<(), AppError> {
        let now = now_unix_secs();
        let clamped_ratio = if scroll_ratio.is_finite() {
            scroll_ratio.clamp(0.0, 1.0)
        } else {
            0.0
        };
        let clamped_page = page_index.max(0);
        let clean_mode = match reading_mode.trim() {
            "horizontal" => "horizontal",
            _ => "vertical",
        };

        sqlx::query(
            "INSERT INTO reader_session
                (id, active, manga_id, chapter_url, chapter_name, scroll_ratio, page_index, reading_mode, updated_at)
             VALUES (1, 1, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(id) DO UPDATE SET
                active = 1,
                manga_id = excluded.manga_id,
                chapter_url = excluded.chapter_url,
                chapter_name = excluded.chapter_name,
                scroll_ratio = excluded.scroll_ratio,
                page_index = excluded.page_index,
                reading_mode = excluded.reading_mode,
                updated_at = excluded.updated_at",
        )
        .bind(manga_id)
        .bind(chapter_url)
        .bind(chapter_name)
        .bind(clamped_ratio)
        .bind(clamped_page)
        .bind(clean_mode)
        .bind(now)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    /// Marks the active reader session cleanly closed (`active = 0`).
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Db`] on SQLite failure.
    pub(crate) async fn clear_reader_session(&self) -> Result<(), AppError> {
        sqlx::query("UPDATE reader_session SET active = 0 WHERE id = 1")
            .execute(self.pool())
            .await?;
        Ok(())
    }

    /// Returns the interrupted reader session if `active == 1` and its manga still exists.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Db`] on SQLite failure.
    pub(crate) async fn get_crash_recovery_session(
        &self,
    ) -> Result<Option<CrashRecoverySession>, AppError> {
        let row = sqlx::query_as::<_, CrashRecoverySession>(
            "SELECT
                s.manga_id,
                m.title AS manga_title,
                s.chapter_url,
                s.chapter_name,
                s.scroll_ratio,
                s.page_index,
                s.reading_mode,
                s.updated_at
             FROM reader_session s
             INNER JOIN manga m ON m.id = s.manga_id
             WHERE s.id = 1 AND s.active = 1 AND m.hidden = 0
             LIMIT 1",
        )
        .fetch_optional(self.pool())
        .await?;
        Ok(row)
    }
}
