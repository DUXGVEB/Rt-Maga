//! Database operations for `manga` and `chapter` tables.

use std::collections::HashSet;

use mangacore::Manga;
use sqlx::FromRow;

use crate::error::AppError;
use crate::repo::{Database, now_unix_secs};

/// Persisted manga record.
#[derive(Debug, Clone, FromRow)]
pub(crate) struct MangaRow {
    pub(crate) id: i64,
    pub(crate) url: String,
    pub(crate) site: String,
    pub(crate) title: String,
    pub(crate) cover_url: Option<String>,
    pub(crate) latest_chapter_name: Option<String>,
    pub(crate) has_new: i64,
    pub(crate) checked_at: Option<i64>,
}

/// Persisted chapter record belonging to a manga.
#[derive(Debug, Clone, FromRow)]
pub(crate) struct ChapterRow {
    pub(crate) url: String,
    pub(crate) name: String,
    pub(crate) number: Option<f64>,
    pub(crate) idx: i64,
    pub(crate) read: i64,
}

impl Database {
    /// Inserts or updates a manga and synchronizes its chapter list while preserving read flags.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Db`] on SQLite failure.
    pub(crate) async fn upsert_manga(&self, manga: &Manga) -> Result<i64, AppError> {
        let now = now_unix_secs();
        let latest = manga.chapters.last();
        let latest_url = latest.map(|c| c.url.as_str());
        let latest_name = latest.map(|c| c.name.as_str());
        let site = manga.site.as_storage_key();
        let cover = manga.cover.as_ref().map(url::Url::as_str);

        let mut tx = self.pool().begin().await?;

        let existing: Option<(i64, Option<String>)> =
            sqlx::query_as("SELECT id, latest_chapter_url FROM manga WHERE url = ?")
                .bind(manga.url.as_str())
                .fetch_optional(&mut *tx)
                .await?;

        let manga_id = if let Some((id, _prev_latest)) = existing {
            sqlx::query(
                "UPDATE manga
                 SET site = ?, title = ?, cover_url = COALESCE(?, cover_url),
                     latest_chapter_url = COALESCE(?, latest_chapter_url),
                     latest_chapter_name = COALESCE(?, latest_chapter_name),
                     updated_at = ?
                 WHERE id = ?",
            )
            .bind(&site)
            .bind(&manga.title)
            .bind(cover)
            .bind(latest_url)
            .bind(latest_name)
            .bind(now)
            .bind(id)
            .execute(&mut *tx)
            .await?;
            id
        } else {
            let res = sqlx::query(
                "INSERT INTO manga
                    (url, site, title, cover_url, latest_chapter_url, latest_chapter_name, has_new, updated_at)
                 VALUES (?, ?, ?, ?, ?, ?, 0, ?)",
            )
            .bind(manga.url.as_str())
            .bind(&site)
            .bind(&manga.title)
            .bind(cover)
            .bind(latest_url)
            .bind(latest_name)
            .bind(now)
            .execute(&mut *tx)
            .await?;
            res.last_insert_rowid()
        };

        for (i, ch) in manga.chapters.iter().enumerate() {
            let idx = i64::try_from(i).unwrap_or(i64::MAX);
            let num = ch.number.map(f64::from);
            sqlx::query(
                "INSERT INTO chapter (manga_id, url, name, number, idx, read)
                 VALUES (?, ?, ?, ?, ?, 0)
                 ON CONFLICT(manga_id, url) DO UPDATE SET
                    name = excluded.name,
                    number = excluded.number,
                    idx = excluded.idx",
            )
            .bind(manga_id)
            .bind(ch.url.as_str())
            .bind(&ch.name)
            .bind(num)
            .bind(idx)
            .execute(&mut *tx)
            .await?;
        }

        // Prune stale chapter rows if a site renamed or removed old chapter slugs on a full listing.
        if manga.chapters.len() > 1 {
            let valid_urls: HashSet<&str> = manga.chapters.iter().map(|c| c.url.as_str()).collect();
            let stored_urls: Vec<(String,)> =
                sqlx::query_as("SELECT url FROM chapter WHERE manga_id = ?")
                    .bind(manga_id)
                    .fetch_all(&mut *tx)
                    .await?;
            for (stored_url,) in stored_urls {
                if !valid_urls.contains(stored_url.as_str()) {
                    sqlx::query("DELETE FROM chapter WHERE manga_id = ? AND url = ?")
                        .bind(manga_id)
                        .bind(&stored_url)
                        .execute(&mut *tx)
                        .await?;
                }
            }
        }

        tx.commit().await?;
        Ok(manga_id)
    }

    /// Loads a manga row by its numeric primary key.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Db`] on SQLite failure.
    pub(crate) async fn get_manga_by_id(&self, id: i64) -> Result<Option<MangaRow>, AppError> {
        let row = sqlx::query_as::<_, MangaRow>("SELECT * FROM manga WHERE id = ?")
            .bind(id)
            .fetch_optional(self.pool())
            .await?;
        Ok(row)
    }

    /// Finds the manga ID owning a given chapter URL.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Db`] on SQLite failure.
    pub(crate) async fn find_manga_id_by_chapter_url(
        &self,
        chapter_url: &str,
    ) -> Result<Option<i64>, AppError> {
        let row: Option<(i64,)> =
            sqlx::query_as("SELECT manga_id FROM chapter WHERE url = ? LIMIT 1")
                .bind(chapter_url)
                .fetch_optional(self.pool())
                .await?;
        Ok(row.map(|(id,)| id))
    }

    /// Lists all chapters for `manga_id` in ascending reading order (`idx ASC`).
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Db`] on SQLite failure.
    pub(crate) async fn list_chapters(&self, manga_id: i64) -> Result<Vec<ChapterRow>, AppError> {
        let rows = sqlx::query_as::<_, ChapterRow>(
            "SELECT * FROM chapter WHERE manga_id = ? ORDER BY idx ASC",
        )
        .bind(manga_id)
        .fetch_all(self.pool())
        .await?;
        Ok(rows)
    }

    /// Marks a specific chapter as read and clears `has_new` if it was the latest chapter.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Db`] on SQLite failure.
    pub(crate) async fn mark_chapter_read(
        &self,
        manga_id: i64,
        chapter_url: &str,
    ) -> Result<(), AppError> {
        sqlx::query("UPDATE chapter SET read = 1 WHERE manga_id = ? AND url = ?")
            .bind(manga_id)
            .bind(chapter_url)
            .execute(self.pool())
            .await?;

        sqlx::query("UPDATE manga SET has_new = 0 WHERE id = ? AND latest_chapter_url = ?")
            .bind(manga_id)
            .bind(chapter_url)
            .execute(self.pool())
            .await?;
        Ok(())
    }

    /// Clears the `has_new` badge for a manga once the user opens its page.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Db`] on SQLite failure.
    pub(crate) async fn clear_has_new(&self, manga_id: i64) -> Result<(), AppError> {
        sqlx::query("UPDATE manga SET has_new = 0 WHERE id = ?")
            .bind(manga_id)
            .execute(self.pool())
            .await?;
        Ok(())
    }

    /// Lists manga that currently exist in `history` and are eligible for background update checks.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Db`] on SQLite failure.
    pub(crate) async fn list_history_manga_for_updates(&self) -> Result<Vec<MangaRow>, AppError> {
        let rows = sqlx::query_as::<_, MangaRow>(
            "SELECT m.* FROM manga m
             INNER JOIN history h ON h.manga_id = m.id
             ORDER BY h.read_at DESC",
        )
        .fetch_all(self.pool())
        .await?;
        Ok(rows)
    }

    /// Records the result of checking a manga for new chapters.
    /// Returns `true` if a genuinely newer latest chapter was discovered.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Db`] on SQLite failure.
    pub(crate) async fn record_update_check(
        &self,
        manga_id: i64,
        fresh: &Manga,
    ) -> Result<Option<String>, AppError> {
        let now = now_unix_secs();
        let prev: Option<(Option<String>,)> =
            sqlx::query_as("SELECT latest_chapter_url FROM manga WHERE id = ?")
                .bind(manga_id)
                .fetch_optional(self.pool())
                .await?;

        let prev_latest = prev.and_then(|(u,)| u);
        let new_latest = fresh.chapters.last();

        let _ = self.upsert_manga(fresh).await?;

        let discovered_name = match (prev_latest.as_deref(), new_latest) {
            (Some(old_url), Some(latest)) if old_url != latest.url.as_str() => {
                sqlx::query("UPDATE manga SET has_new = 1, checked_at = ? WHERE id = ?")
                    .bind(now)
                    .bind(manga_id)
                    .execute(self.pool())
                    .await?;
                Some(latest.name.clone())
            }
            _ => {
                sqlx::query("UPDATE manga SET checked_at = ? WHERE id = ?")
                    .bind(now)
                    .bind(manga_id)
                    .execute(self.pool())
                    .await?;
                None
            }
        };

        Ok(discovered_name)
    }

    /// Deletes cached manga records that are NOT in reading history and haven't been updated for `max_age_days`.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Db`] on SQLite failure.
    pub(crate) async fn prune_stale_manga(&self, max_age_days: i64) -> Result<u64, AppError> {
        let cutoff = now_unix_secs().saturating_sub(max_age_days.saturating_mul(86_400));
        let res = sqlx::query(
            "DELETE FROM manga
             WHERE updated_at < ?
               AND id NOT IN (SELECT manga_id FROM history)",
        )
        .bind(cutoff)
        .execute(self.pool())
        .await?;
        Ok(res.rows_affected())
    }
}
