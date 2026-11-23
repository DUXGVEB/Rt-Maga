//! Database operations for manga `bookmark` records.

use serde::{Deserialize, Serialize};
use sqlx::FromRow;

use crate::error::AppError;
use crate::repo::{Database, now_unix_secs};

/// Ordering options for the dedicated Bookmarks screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub(crate) enum BookmarkSort {
    /// Most recently bookmarked first (default).
    #[default]
    Newest,
    /// Alphabetical by manga title.
    Title,
    /// Most recently read first (falling back to bookmark time).
    RecentRead,
    /// Manga with newly discovered chapters pinned at the top.
    HasNew,
}

/// Joined bookmark entry returned to the UI.
#[derive(Debug, Clone, FromRow)]
pub(crate) struct BookmarkEntryRow {
    pub manga_id: i64,
    pub manga_url: String,
    pub site: String,
    pub title: String,
    pub cover_url: Option<String>,
    pub bookmarked_at: i64,
    pub latest_chapter_url: Option<String>,
    pub latest_chapter_name: Option<String>,
    pub has_new: i64,
    pub total_chapters: i64,
    pub unread_count: i64,
    pub is_completed: i64,
    pub chapter_url: Option<String>,
    pub chapter_name: Option<String>,
    pub scroll_ratio: Option<f64>,
    pub page_index: Option<i64>,
    pub read_at: Option<i64>,
    pub current_chapter_idx: Option<i64>,
    pub reader_mode_override: Option<String>,
    pub detected_webtoon: i64,
}

impl Database {
    /// Returns whether `manga_id` is currently bookmarked.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Db`] on SQLite failure.
    pub(crate) async fn is_bookmarked(&self, manga_id: i64) -> Result<bool, AppError> {
        let row: Option<(i64,)> =
            sqlx::query_as("SELECT manga_id FROM bookmark WHERE manga_id = ? LIMIT 1")
                .bind(manga_id)
                .fetch_optional(self.pool())
                .await?;
        Ok(row.is_some())
    }

    /// Toggles the bookmark state for `manga_id` and returns the new state (`true` = bookmarked).
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Db`] on SQLite failure.
    pub(crate) async fn toggle_bookmark(&self, manga_id: i64) -> Result<bool, AppError> {
        let mut tx = self.pool().begin().await?;
        let exists: Option<(i64,)> =
            sqlx::query_as("SELECT manga_id FROM bookmark WHERE manga_id = ? LIMIT 1")
                .bind(manga_id)
                .fetch_optional(&mut *tx)
                .await?;

        let now_bookmarked = if exists.is_some() {
            sqlx::query("DELETE FROM bookmark WHERE manga_id = ?")
                .bind(manga_id)
                .execute(&mut *tx)
                .await?;
            false
        } else {
            let now = now_unix_secs();
            sqlx::query(
                "INSERT INTO bookmark (manga_id, created_at) VALUES (?, ?)
                 ON CONFLICT(manga_id) DO NOTHING",
            )
            .bind(manga_id)
            .bind(now)
            .execute(&mut *tx)
            .await?;
            true
        };

        tx.commit().await?;
        Ok(now_bookmarked)
    }

    /// Lists all bookmarked manga joined with optional reading progress and chapter counts.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Db`] on SQLite failure.
    pub(crate) async fn list_bookmarks(
        &self,
        sort: BookmarkSort,
    ) -> Result<Vec<BookmarkEntryRow>, AppError> {
        let order_clause = match sort {
            BookmarkSort::Newest => "b.created_at DESC",
            BookmarkSort::Title => "LOWER(m.title) ASC, b.created_at DESC",
            BookmarkSort::RecentRead => "COALESCE(h.read_at, 0) DESC, b.created_at DESC",
            BookmarkSort::HasNew => "m.has_new DESC, COALESCE(h.read_at, b.created_at) DESC",
        };

        let sql = format!(
            "SELECT
                b.manga_id,
                m.url AS manga_url,
                m.site,
                m.title,
                m.cover_url,
                b.created_at AS bookmarked_at,
                m.latest_chapter_url,
                m.latest_chapter_name,
                m.has_new,
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
                h.chapter_url,
                h.chapter_name,
                h.scroll_ratio,
                h.page_index,
                h.read_at,
                (SELECT c.idx FROM chapter c WHERE c.manga_id = m.id AND h.chapter_url IS NOT NULL AND RTRIM(c.url, '/') = RTRIM(h.chapter_url, '/') LIMIT 1) AS current_chapter_idx,
                m.reader_mode_override,
                m.detected_webtoon
             FROM bookmark b
             INNER JOIN manga m ON m.id = b.manga_id
             LEFT JOIN history h ON h.manga_id = m.id
             WHERE m.hidden = 0
             ORDER BY {order_clause}"
        );

        let rows = sqlx::query_as::<_, BookmarkEntryRow>(&sql)
            .fetch_all(self.pool())
            .await?;
        Ok(rows)
    }
}
