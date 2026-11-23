//! Database operations for persistent in-app chapter notifications and deduplication.

use std::collections::HashSet;

use mangacore::ChapterRef;
use sqlx::FromRow;

use crate::error::AppError;
use crate::repo::manga::MangaRow;
use crate::repo::{Database, now_unix_secs};

/// Maximum number of persistent notifications retained in SQLite.
const MAX_STORED_NOTIFICATIONS: i64 = 100;

/// Stored notification entry returned to the Notification Center UI.
#[derive(Debug, Clone, FromRow)]
pub(crate) struct NotificationRow {
    pub id: i64,
    pub manga_id: i64,
    pub manga_title: String,
    pub cover_url: Option<String>,
    pub manga_url: String,
    pub chapter_range: String,
    pub latest_chapter_url: String,
    pub chapter_count: i64,
    pub read: i64,
    pub created_at: i64,
}

/// Formats one or more newly discovered chapters into a concise range label such as
/// `"Chapter 120"` (single) or `"120–125"` / `"120.5–122"` (multiple).
pub(crate) fn format_chapter_range(chapters: &[ChapterRef]) -> String {
    let Some(first) = chapters.first() else {
        return String::new();
    };
    if chapters.len() == 1 {
        let trimmed = first.name.trim();
        if !trimmed.is_empty() {
            return trimmed.to_owned();
        }
        return first
            .number
            .map_or_else(|| "New Chapter".to_owned(), format_chapter_number);
    }

    let Some(last) = chapters.last() else {
        return first.name.trim().to_owned();
    };

    let first_num = first
        .number
        .or_else(|| mangacore::parse::chapters::extract_chapter_number(&first.name));
    let last_num = last
        .number
        .or_else(|| mangacore::parse::chapters::extract_chapter_number(&last.name));

    if let (Some(a), Some(b)) = (first_num, last_num) {
        let low = a.min(b);
        let high = a.max(b);
        if (low - high).abs() < f32::EPSILON {
            return format_chapter_number(low);
        }
        return format!(
            "{}–{}",
            format_chapter_number(low),
            format_chapter_number(high)
        );
    }

    let a_name = first.name.trim();
    let b_name = last.name.trim();
    if a_name.is_empty() || b_name.is_empty() || a_name == b_name {
        format!("{} chapters", chapters.len())
    } else {
        format!("{a_name} – {b_name}")
    }
}

fn format_chapter_number(n: f32) -> String {
    let rounded = n.round();
    if (n - rounded).abs() < 1e-4 {
        format!("{rounded:.0}")
    } else {
        let formatted = format!("{n:.2}");
        formatted
            .trim_end_matches('0')
            .trim_end_matches('.')
            .to_owned()
    }
}

impl Database {
    /// Filters `discovered` chapters down to those that have never been notified for `manga_id`.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Db`] on SQLite failure.
    pub(crate) async fn filter_unnotified_chapters(
        &self,
        manga_id: i64,
        discovered: &[ChapterRef],
    ) -> Result<Vec<ChapterRef>, AppError> {
        if discovered.is_empty() {
            return Ok(Vec::new());
        }

        let already: HashSet<String> = sqlx::query_as::<_, (String,)>(
            "SELECT chapter_url FROM notified_chapter WHERE manga_id = ?",
        )
        .bind(manga_id)
        .fetch_all(self.pool())
        .await?
        .into_iter()
        .map(|(u,)| u.trim_end_matches('/').to_owned())
        .collect();

        Ok(discovered
            .iter()
            .filter(|ch| !already.contains(ch.url.as_str().trim_end_matches('/')))
            .cloned()
            .collect())
    }

    /// Persists a deduplicated chapter-update notification and marks the chapters as notified.
    /// Returns `None` if all supplied chapters were already notified.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Db`] on SQLite failure.
    pub(crate) async fn record_chapter_notification(
        &self,
        manga: &MangaRow,
        discovered: &[ChapterRef],
    ) -> Result<Option<NotificationRow>, AppError> {
        let fresh = self
            .filter_unnotified_chapters(manga.id, discovered)
            .await?;
        if fresh.is_empty() {
            return Ok(None);
        }

        let now = now_unix_secs();
        let chapter_range = format_chapter_range(&fresh);
        let latest_url = fresh
            .last()
            .map_or_else(|| manga.url.clone(), |c| c.url.as_str().to_owned());
        let chapter_count = i64::try_from(fresh.len()).unwrap_or(i64::MAX);

        let mut tx = self.pool().begin().await?;

        for ch in &fresh {
            sqlx::query(
                "INSERT INTO notified_chapter (manga_id, chapter_url, notified_at)
                 VALUES (?, ?, ?)
                 ON CONFLICT(manga_id, chapter_url) DO NOTHING",
            )
            .bind(manga.id)
            .bind(ch.url.as_str().trim_end_matches('/'))
            .bind(now)
            .execute(&mut *tx)
            .await?;
        }

        let res = sqlx::query(
            "INSERT INTO notification
                (manga_id, manga_title, cover_url, manga_url, chapter_range, latest_chapter_url, chapter_count, read, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, 0, ?)",
        )
        .bind(manga.id)
        .bind(&manga.title)
        .bind(manga.cover_url.as_deref())
        .bind(&manga.url)
        .bind(&chapter_range)
        .bind(&latest_url)
        .bind(chapter_count)
        .bind(now)
        .execute(&mut *tx)
        .await?;

        let inserted_id = res.last_insert_rowid();

        // Keep notification history bounded to avoid unbounded SQLite growth.
        sqlx::query(
            "DELETE FROM notification
             WHERE id NOT IN (
                 SELECT id FROM notification ORDER BY created_at DESC, id DESC LIMIT ?
             )",
        )
        .bind(MAX_STORED_NOTIFICATIONS)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;

        Ok(Some(NotificationRow {
            id: inserted_id,
            manga_id: manga.id,
            manga_title: manga.title.clone(),
            cover_url: manga.cover_url.clone(),
            manga_url: manga.url.clone(),
            chapter_range,
            latest_chapter_url: latest_url,
            chapter_count,
            read: 0,
            created_at: now,
        }))
    }

    /// Lists stored notifications newest first (excluding hidden manga).
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Db`] on SQLite failure.
    pub(crate) async fn list_notifications(&self) -> Result<Vec<NotificationRow>, AppError> {
        let rows = sqlx::query_as::<_, NotificationRow>(
            "SELECT n.*
             FROM notification n
             INNER JOIN manga m ON m.id = n.manga_id
             WHERE m.hidden = 0
             ORDER BY n.created_at DESC, n.id DESC
             LIMIT ?",
        )
        .bind(MAX_STORED_NOTIFICATIONS)
        .fetch_all(self.pool())
        .await?;
        Ok(rows)
    }

    /// Marks a single notification as read.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Db`] on SQLite failure.
    pub(crate) async fn mark_notification_read(&self, id: i64) -> Result<(), AppError> {
        sqlx::query("UPDATE notification SET read = 1 WHERE id = ?")
            .bind(id)
            .execute(self.pool())
            .await?;
        Ok(())
    }

    /// Marks all notifications as read.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Db`] on SQLite failure.
    pub(crate) async fn mark_all_notifications_read(&self) -> Result<(), AppError> {
        sqlx::query("UPDATE notification SET read = 1 WHERE read = 0")
            .execute(self.pool())
            .await?;
        Ok(())
    }

    /// Deletes a single notification by ID.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Db`] on SQLite failure.
    pub(crate) async fn delete_notification(&self, id: i64) -> Result<(), AppError> {
        sqlx::query("DELETE FROM notification WHERE id = ?")
            .bind(id)
            .execute(self.pool())
            .await?;
        Ok(())
    }

    /// Clears all stored notifications from the Notification Center.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Db`] on SQLite failure.
    pub(crate) async fn clear_notifications(&self) -> Result<(), AppError> {
        sqlx::query("DELETE FROM notification")
            .execute(self.pool())
            .await?;
        Ok(())
    }
}
