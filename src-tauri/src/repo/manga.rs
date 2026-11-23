//! Database operations for `manga` and `chapter` tables.

use std::collections::HashSet;

use mangacore::{ChapterRef, Manga};
use sqlx::FromRow;

use crate::error::AppError;
use crate::repo::{Database, now_unix_secs};

/// Action for hiding or unhiding a manga from normal library listings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub(crate) enum HideAction {
    Hide,
    Unhide,
}

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
    pub(crate) reader_mode_override: Option<String>,
    pub(crate) detected_webtoon: i64,
    pub(crate) hidden: i64,
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
        self.upsert_manga_with_id(None, manga).await
    }

    /// Inserts or updates a manga (targeting `known_id` directly when refreshing an existing entry)
    /// and synchronizes its chapter list while preserving read flags.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Db`] on SQLite failure.
    pub(crate) async fn upsert_manga_with_id(
        &self,
        known_id: Option<i64>,
        manga: &Manga,
    ) -> Result<i64, AppError> {
        let now = now_unix_secs();
        let latest = manga.chapters.last();
        let latest_url = latest.map(|c| c.url.as_str());
        let latest_name = latest.map(|c| c.name.as_str());
        let site = manga.site.as_storage_key();
        let cover = manga.cover.as_ref().map(url::Url::as_str);

        let mut tx = self.pool().begin().await?;

        let existing_id: Option<i64> = if let Some(id) = known_id {
            sqlx::query_as::<_, (i64,)>("SELECT id FROM manga WHERE id = ?")
                .bind(id)
                .fetch_optional(&mut *tx)
                .await?
                .map(|(found,)| found)
        } else {
            sqlx::query_as::<_, (i64,)>(
                "SELECT id FROM manga WHERE RTRIM(url, '/') = RTRIM(?, '/') LIMIT 1",
            )
            .bind(manga.url.as_str())
            .fetch_optional(&mut *tx)
            .await?
            .map(|(found,)| found)
        };

        let stored_urls: Vec<(String,)> = if let Some(id) = existing_id {
            sqlx::query_as("SELECT url FROM chapter WHERE manga_id = ?")
                .bind(id)
                .fetch_all(&mut *tx)
                .await?
        } else {
            Vec::new()
        };
        let is_single_chapter_fallback =
            existing_id.is_some() && manga.chapters.len() == 1 && stored_urls.len() > 1;

        let manga_id = if let Some(id) = existing_id {
            if is_single_chapter_fallback {
                sqlx::query(
                    "UPDATE manga
                     SET cover_url = COALESCE(?, cover_url),
                         updated_at = ?
                     WHERE id = ?",
                )
                .bind(cover)
                .bind(now)
                .bind(id)
                .execute(&mut *tx)
                .await?;
            } else {
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
            }
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

        Self::sync_upserted_chapters(&mut tx, manga_id, existing_id, manga, &stored_urls).await?;

        tx.commit().await?;
        Ok(manga_id)
    }

    async fn sync_upserted_chapters(
        tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        manga_id: i64,
        existing_id: Option<i64>,
        manga: &Manga,
        stored_urls: &[(String,)],
    ) -> Result<(), AppError> {
        let is_single_chapter_fallback =
            existing_id.is_some() && manga.chapters.len() == 1 && stored_urls.len() > 1;

        if is_single_chapter_fallback {
            if let Some(ch) = manga.chapters.first() {
                let norm = ch.url.as_str().trim_end_matches('/');
                let already = stored_urls
                    .iter()
                    .any(|(u,)| u.trim_end_matches('/') == norm);
                if !already {
                    let next_idx = i64::try_from(stored_urls.len()).unwrap_or(i64::MAX);
                    sqlx::query(
                        "INSERT INTO chapter (manga_id, url, name, number, idx, read)
                         VALUES (?, ?, ?, ?, ?, 0)
                         ON CONFLICT(manga_id, url) DO NOTHING",
                    )
                    .bind(manga_id)
                    .bind(ch.url.as_str())
                    .bind(&ch.name)
                    .bind(ch.number.map(f64::from))
                    .bind(next_idx)
                    .execute(&mut **tx)
                    .await?;
                }
            }
            return Ok(());
        }

        for (i, ch) in manga.chapters.iter().enumerate() {
            let idx = i64::try_from(i).unwrap_or(i64::MAX);
            let num = ch.number.map(f64::from);
            // Update in-place if a trailing-slash variant already exists so `read` flag is preserved.
            let updated = sqlx::query(
                "UPDATE chapter
                 SET url = ?, name = ?, number = ?, idx = ?
                 WHERE manga_id = ? AND RTRIM(url, '/') = RTRIM(?, '/')",
            )
            .bind(ch.url.as_str())
            .bind(&ch.name)
            .bind(num)
            .bind(idx)
            .bind(manga_id)
            .bind(ch.url.as_str())
            .execute(&mut **tx)
            .await?;

            if updated.rows_affected() == 0 {
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
                .execute(&mut **tx)
                .await?;
            }
        }

        // Prune stale chapter rows if a site renamed or removed old chapter slugs on a full listing.
        if manga.chapters.len() > 1 {
            let valid_norms: HashSet<&str> = manga
                .chapters
                .iter()
                .map(|c| c.url.as_str().trim_end_matches('/'))
                .collect();
            let current_urls: Vec<(String,)> =
                sqlx::query_as("SELECT url FROM chapter WHERE manga_id = ?")
                    .bind(manga_id)
                    .fetch_all(&mut **tx)
                    .await?;
            for (stored_url,) in current_urls {
                if !valid_norms.contains(stored_url.trim_end_matches('/')) {
                    sqlx::query("DELETE FROM chapter WHERE manga_id = ? AND url = ?")
                        .bind(manga_id)
                        .bind(&stored_url)
                        .execute(&mut **tx)
                        .await?;
                }
            }
        }

        Ok(())
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
        let row: Option<(i64,)> = sqlx::query_as(
            "SELECT manga_id FROM chapter WHERE RTRIM(url, '/') = RTRIM(?, '/') LIMIT 1",
        )
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
        sqlx::query(
            "UPDATE chapter SET read = 1 WHERE manga_id = ? AND RTRIM(url, '/') = RTRIM(?, '/')",
        )
        .bind(manga_id)
        .bind(chapter_url)
        .execute(self.pool())
        .await?;

        sqlx::query(
            "UPDATE manga SET has_new = 0 WHERE id = ? AND RTRIM(latest_chapter_url, '/') = RTRIM(?, '/')",
        )
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

    /// Sets or clears the per-manga manual reader mode override
    /// (`None`, `"vertical_strip"`, `"vertical_grouped"`, or `"horizontal"`).
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Db`] on SQLite failure.
    pub(crate) async fn set_manga_reader_override(
        &self,
        manga_id: i64,
        override_mode: Option<&str>,
    ) -> Result<(), AppError> {
        let clean = override_mode.and_then(|raw| match raw.trim() {
            "vertical_strip" | "vertical_grouped" | "horizontal" => Some(raw.trim()),
            _ => None,
        });
        sqlx::query("UPDATE manga SET reader_mode_override = ? WHERE id = ?")
            .bind(clean)
            .bind(manga_id)
            .execute(self.pool())
            .await?;
        Ok(())
    }

    /// Persists the passive Webtoon / Long-Strip detection verdict for a manga.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Db`] on SQLite failure.
    pub(crate) async fn mark_manga_detected_webtoon(&self, manga_id: i64) -> Result<(), AppError> {
        sqlx::query("UPDATE manga SET detected_webtoon = 1 WHERE id = ?")
            .bind(manga_id)
            .execute(self.pool())
            .await?;
        Ok(())
    }

    /// Lists manga eligible for background update checks: **bookmarked manga first**,
    /// followed by remaining reading-history manga, deduplicated by manga ID.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Db`] on SQLite failure.
    pub(crate) async fn list_targets_for_updates(&self) -> Result<Vec<MangaRow>, AppError> {
        let rows = sqlx::query_as::<_, MangaRow>(
            "SELECT m.*
             FROM manga m
             LEFT JOIN bookmark b ON b.manga_id = m.id
             LEFT JOIN history h ON h.manga_id = m.id
             WHERE m.hidden = 0 AND (b.manga_id IS NOT NULL OR h.manga_id IS NOT NULL)
             ORDER BY
                CASE WHEN b.manga_id IS NOT NULL THEN 0 ELSE 1 END ASC,
                COALESCE(h.read_at, b.created_at, m.updated_at) DESC",
        )
        .fetch_all(self.pool())
        .await?;
        Ok(rows)
    }

    /// Records the result of checking a manga for new chapters.
    /// Returns all genuinely newly discovered chapters (in ascending chapter order).
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Db`] on SQLite failure.
    pub(crate) async fn record_update_check(
        &self,
        manga_id: i64,
        fresh: &Manga,
    ) -> Result<Vec<ChapterRef>, AppError> {
        let now = now_unix_secs();
        let existing_urls: HashSet<String> =
            sqlx::query_as::<_, (String,)>("SELECT url FROM chapter WHERE manga_id = ?")
                .bind(manga_id)
                .fetch_all(self.pool())
                .await?
                .into_iter()
                .map(|(u,)| u.trim_end_matches('/').to_owned())
                .collect();

        let _ = self.upsert_manga_with_id(Some(manga_id), fresh).await?;

        let newly_discovered: Vec<ChapterRef> = if existing_urls.is_empty() {
            Vec::new()
        } else {
            fresh
                .chapters
                .iter()
                .filter(|ch| !existing_urls.contains(ch.url.as_str().trim_end_matches('/')))
                .cloned()
                .collect()
        };

        if newly_discovered.is_empty() {
            sqlx::query("UPDATE manga SET checked_at = ? WHERE id = ?")
                .bind(now)
                .bind(manga_id)
                .execute(self.pool())
                .await?;
        } else {
            sqlx::query("UPDATE manga SET has_new = 1, checked_at = ? WHERE id = ?")
                .bind(now)
                .bind(manga_id)
                .execute(self.pool())
                .await?;
        }

        Ok(newly_discovered)
    }

    /// Hides or restores a manga in normal Home/History/Bookmarks listings.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Db`] on SQLite failure.
    pub(crate) async fn set_manga_hidden(
        &self,
        manga_id: i64,
        action: HideAction,
    ) -> Result<bool, AppError> {
        let val: i64 = match action {
            HideAction::Hide => 1,
            HideAction::Unhide => 0,
        };
        sqlx::query("UPDATE manga SET hidden = ? WHERE id = ?")
            .bind(val)
            .bind(manga_id)
            .execute(self.pool())
            .await?;
        if action == HideAction::Hide {
            sqlx::query("UPDATE reader_session SET active = 0 WHERE id = 1 AND manga_id = ?")
                .bind(manga_id)
                .execute(self.pool())
                .await?;
        }
        Ok(val == 1)
    }

    /// Lists all hidden manga so the user can restore or permanently delete them.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Db`] on SQLite failure.
    pub(crate) async fn list_hidden_manga(&self) -> Result<Vec<MangaRow>, AppError> {
        let rows = sqlx::query_as::<_, MangaRow>(
            "SELECT * FROM manga WHERE hidden = 1 ORDER BY updated_at DESC",
        )
        .fetch_all(self.pool())
        .await?;
        Ok(rows)
    }

    /// Permanently deletes a manga and all dependent SQLite records (`chapter`, `history`,
    /// `bookmark`, `notification`, `notified_chapter`) and clears dangling session pointers.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Db`] on SQLite failure.
    pub(crate) async fn delete_manga_permanently(&self, manga_id: i64) -> Result<(), AppError> {
        let mut tx = self.pool().begin().await?;
        sqlx::query("DELETE FROM manga WHERE id = ?")
            .bind(manga_id)
            .execute(&mut *tx)
            .await?;
        sqlx::query("UPDATE reader_session SET active = 0 WHERE id = 1 AND manga_id = ?")
            .bind(manga_id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(())
    }

    /// Returns whether a manga is completed (latest chapter read and `has_new == 0`)
    /// along with its total unread chapter count.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Db`] on SQLite failure.
    pub(crate) async fn get_manga_read_summary(
        &self,
        manga_id: i64,
    ) -> Result<(i64, bool), AppError> {
        let row: Option<(i64, i64)> = sqlx::query_as(
            "SELECT
                (SELECT COUNT(*) FROM chapter c WHERE c.manga_id = m.id AND c.read = 0) AS unread_count,
                CASE WHEN m.has_new = 0
                     AND m.latest_chapter_url IS NOT NULL
                     AND EXISTS (
                         SELECT 1 FROM chapter c
                         WHERE c.manga_id = m.id
                           AND RTRIM(c.url, '/') = RTRIM(m.latest_chapter_url, '/')
                           AND c.read = 1
                     ) THEN 1 ELSE 0 END AS is_completed
             FROM manga m WHERE m.id = ?",
        )
        .bind(manga_id)
        .fetch_optional(self.pool())
        .await?;
        Ok(row.map_or((0, false), |(unread, completed)| (unread, completed != 0)))
    }

    /// Deletes cached manga records that are NOT in reading history, bookmarks, or hidden
    /// and haven't been updated for `max_age_days`, returning their IDs for disk cache cleanup.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Db`] on SQLite failure.
    pub(crate) async fn prune_stale_manga(&self, max_age_days: i64) -> Result<Vec<i64>, AppError> {
        let cutoff = now_unix_secs().saturating_sub(max_age_days.saturating_mul(86_400));
        let stale_ids: Vec<(i64,)> = sqlx::query_as(
            "SELECT id FROM manga
             WHERE updated_at < ?
               AND hidden = 0
               AND id NOT IN (SELECT manga_id FROM history)
               AND id NOT IN (SELECT manga_id FROM bookmark)",
        )
        .bind(cutoff)
        .fetch_all(self.pool())
        .await?;

        if stale_ids.is_empty() {
            return Ok(Vec::new());
        }

        sqlx::query(
            "DELETE FROM manga
             WHERE updated_at < ?
               AND hidden = 0
               AND id NOT IN (SELECT manga_id FROM history)
               AND id NOT IN (SELECT manga_id FROM bookmark)",
        )
        .bind(cutoff)
        .execute(self.pool())
        .await?;

        Ok(stale_ids.into_iter().map(|(id,)| id).collect())
    }
}
