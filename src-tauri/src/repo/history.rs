//! Database operations for reading `history` and exact per-manga progress.

use serde::{Deserialize, Serialize};
use sqlx::FromRow;

use crate::error::AppError;
use crate::repo::{Database, now_unix_secs};

/// Persisted last-reading destination used by the History double-tap resume toggle.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LastReadingDestination {
    pub manga_id: i64,
    pub chapter_url: String,
    pub chapter_name: String,
    pub scroll_ratio: f64,
    pub page_index: i64,
    pub read_at: i64,
}

/// Single manga reading progress record stored in `history`.
#[derive(Debug, Clone, FromRow)]
pub(crate) struct HistoryProgressRow {
    pub chapter_url: String,
    pub chapter_name: String,
    pub scroll_ratio: f64,
    pub page_index: i64,
    pub read_at: i64,
}

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
    pub page_index: i64,
    pub read_at: i64,
    pub latest_chapter_url: Option<String>,
    pub latest_chapter_name: Option<String>,
    pub has_new: i64,
    pub bookmarked: i64,
    pub total_chapters: i64,
    pub unread_count: i64,
    pub is_completed: i64,
    pub current_chapter_idx: Option<i64>,
}

const LAST_READING_KEY: &str = "last_reading_destination_v1";

impl Database {
    /// Upserts exact reading progress (`chapter_url`, `scroll_ratio`, `page_index`) into `history`
    /// and persists the most recently read destination for quick History-button resume.
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
        page_index: i64,
    ) -> Result<(), AppError> {
        let now = now_unix_secs();
        let clamped_ratio = if scroll_ratio.is_finite() {
            scroll_ratio.clamp(0.0, 1.0)
        } else {
            0.0
        };
        let clamped_page = page_index.max(0);

        sqlx::query(
            "INSERT INTO history (manga_id, chapter_url, chapter_name, scroll_ratio, page_index, read_at)
             VALUES (?, ?, ?, ?, ?, ?)
             ON CONFLICT(manga_id) DO UPDATE SET
                chapter_url = excluded.chapter_url,
                chapter_name = excluded.chapter_name,
                scroll_ratio = excluded.scroll_ratio,
                page_index = excluded.page_index,
                read_at = excluded.read_at",
        )
        .bind(manga_id)
        .bind(chapter_url)
        .bind(chapter_name)
        .bind(clamped_ratio)
        .bind(clamped_page)
        .bind(now)
        .execute(self.pool())
        .await?;

        let dest = LastReadingDestination {
            manga_id,
            chapter_url: chapter_url.to_owned(),
            chapter_name: chapter_name.to_owned(),
            scroll_ratio: clamped_ratio,
            page_index: clamped_page,
            read_at: now,
        };
        if let Ok(json) = serde_json::to_string(&dest) {
            let _ = sqlx::query(
                "INSERT INTO settings (key, value) VALUES (?, ?)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            )
            .bind(LAST_READING_KEY)
            .bind(json)
            .execute(self.pool())
            .await;
        }

        Ok(())
    }

    /// Loads the persisted last reading destination (validating that the manga still exists),
    /// falling back to the most recent row in `history`.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Db`] on SQLite failure.
    pub(crate) async fn get_last_reading_destination(
        &self,
    ) -> Result<Option<LastReadingDestination>, AppError> {
        let saved: Option<(String,)> =
            sqlx::query_as("SELECT value FROM settings WHERE key = ? LIMIT 1")
                .bind(LAST_READING_KEY)
                .fetch_optional(self.pool())
                .await?;

        if let Some((json,)) = saved
            && let Ok(dest) = serde_json::from_str::<LastReadingDestination>(&json)
            && self.get_history_for_manga(dest.manga_id).await?.is_some()
            && self
                .get_manga_by_id(dest.manga_id)
                .await?
                .is_some_and(|m| m.hidden == 0)
            && let Ok((_, is_completed)) = self.get_manga_read_summary(dest.manga_id).await
            && !is_completed
        {
            return Ok(Some(dest));
        }

        let fallback: Option<(i64, String, String, f64, i64, i64)> = sqlx::query_as(
            "SELECT h.manga_id, h.chapter_url, h.chapter_name, h.scroll_ratio, h.page_index, h.read_at
             FROM history h
             INNER JOIN manga m ON m.id = h.manga_id
             WHERE m.hidden = 0
               AND NOT (
                   m.has_new = 0
                   AND m.latest_chapter_url IS NOT NULL
                   AND EXISTS (
                       SELECT 1 FROM chapter c
                       WHERE c.manga_id = m.id
                         AND RTRIM(c.url, '/') = RTRIM(m.latest_chapter_url, '/')
                         AND c.read = 1
                   )
               )
             ORDER BY h.read_at DESC LIMIT 1",
        )
        .fetch_optional(self.pool())
        .await?;

        Ok(fallback.map(
            |(manga_id, chapter_url, chapter_name, scroll_ratio, page_index, read_at)| {
                LastReadingDestination {
                    manga_id,
                    chapter_url,
                    chapter_name,
                    scroll_ratio,
                    page_index,
                    read_at,
                }
            },
        ))
    }

    /// Loads visible (non-hidden) reading history ordered by most recently read first.
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
                h.page_index,
                h.read_at,
                m.latest_chapter_url,
                m.latest_chapter_name,
                m.has_new,
                CASE WHEN b.manga_id IS NOT NULL THEN 1 ELSE 0 END AS bookmarked,
                (SELECT COUNT(*) FROM chapter c WHERE c.manga_id = m.id) AS total_chapters,
                (SELECT COUNT(*) FROM chapter c WHERE c.manga_id = m.id AND c.read = 0) AS unread_count,
                CASE WHEN m.has_new = 0
                     AND m.latest_chapter_url IS NOT NULL
                     AND EXISTS (
                         SELECT 1 FROM chapter c
                         WHERE c.manga_id = m.id
                           AND RTRIM(c.url, '/') = RTRIM(m.latest_chapter_url, '/')
                           AND c.read = 1
                     ) THEN 1 ELSE 0 END AS is_completed,
                (SELECT c.idx FROM chapter c WHERE c.manga_id = m.id AND RTRIM(c.url, '/') = RTRIM(h.chapter_url, '/') LIMIT 1) AS current_chapter_idx
             FROM history h
             INNER JOIN manga m ON m.id = h.manga_id
             LEFT JOIN bookmark b ON b.manga_id = m.id
             WHERE m.hidden = 0
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
    ) -> Result<Option<HistoryProgressRow>, AppError> {
        let row = sqlx::query_as::<_, HistoryProgressRow>(
            "SELECT chapter_url, chapter_name, scroll_ratio, page_index, read_at
             FROM history WHERE manga_id = ?",
        )
        .bind(manga_id)
        .fetch_optional(self.pool())
        .await?;
        Ok(row)
    }

    /// Removes a manga from reading history, clears its `has_new` flag if not bookmarked,
    /// and deactivates any active crash-recovery session pointing to it.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Db`] on SQLite failure.
    pub(crate) async fn remove_history(&self, manga_id: i64) -> Result<(), AppError> {
        sqlx::query("DELETE FROM history WHERE manga_id = ?")
            .bind(manga_id)
            .execute(self.pool())
            .await?;
        sqlx::query("UPDATE reader_session SET active = 0 WHERE id = 1 AND manga_id = ?")
            .bind(manga_id)
            .execute(self.pool())
            .await?;
        sqlx::query(
            "UPDATE manga SET has_new = 0
             WHERE id = ? AND id NOT IN (SELECT manga_id FROM bookmark)",
        )
        .bind(manga_id)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    /// Clears the entire reading history and resets `has_new` flags for non-bookmarked manga.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Db`] on SQLite failure.
    pub(crate) async fn clear_history(&self) -> Result<(), AppError> {
        sqlx::query("DELETE FROM history")
            .execute(self.pool())
            .await?;
        sqlx::query("UPDATE reader_session SET active = 0 WHERE id = 1")
            .execute(self.pool())
            .await?;
        sqlx::query("DELETE FROM settings WHERE key = ?")
            .bind(LAST_READING_KEY)
            .execute(self.pool())
            .await?;
        sqlx::query(
            "UPDATE manga SET has_new = 0
             WHERE id NOT IN (SELECT manga_id FROM bookmark)",
        )
        .execute(self.pool())
        .await?;
        Ok(())
    }
}
