//! SQLite export and smart conflict-resolving merge for peer-to-peer LAN synchronization.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::error::AppError;
use crate::repo::{Database, now_unix_secs};

/// Serializable chapter record inside a LAN sync bundle.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SyncChapterPayload {
    pub url: String,
    pub name: String,
    pub number: Option<f64>,
    pub idx: i64,
    pub read: bool,
}

/// Serializable reading history pointer inside a LAN sync bundle.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SyncHistoryPayload {
    pub chapter_url: String,
    pub chapter_name: String,
    pub scroll_ratio: f64,
    #[serde(default)]
    pub page_index: i64,
    pub read_at: i64,
}

/// Serializable manga + chapters + history + bookmark record transferred over LAN.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SyncMangaPayload {
    pub url: String,
    pub site: String,
    pub title: String,
    pub cover_url: Option<String>,
    pub latest_chapter_url: Option<String>,
    pub latest_chapter_name: Option<String>,
    #[serde(default)]
    pub bookmarked: bool,
    pub chapters: Vec<SyncChapterPayload>,
    pub history: Option<SyncHistoryPayload>,
}

/// Root JSON payload exchanged during a confirmed LAN sync session.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SyncBundle {
    pub version: u8,
    pub device_name: String,
    pub exported_at: i64,
    pub manga: Vec<SyncMangaPayload>,
}

/// Summary report returned after merging an incoming [`SyncBundle`] into SQLite.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SyncMergeReport {
    pub added_manga: usize,
    pub updated_newer_chapter: usize,
    pub skipped_same_chapter: usize,
    pub kept_local_newer: usize,
    pub synced_bookmarks: usize,
    pub total_received: usize,
}

#[derive(Debug, sqlx::FromRow)]
struct ExportMangaRow {
    id: i64,
    url: String,
    site: String,
    title: String,
    cover_url: Option<String>,
    latest_chapter_url: Option<String>,
    latest_chapter_name: Option<String>,
    bookmarked: i64,
}

#[derive(Debug, sqlx::FromRow)]
struct ExportChapterRow {
    url: String,
    name: String,
    number: Option<f64>,
    idx: i64,
    read: i64,
}

#[derive(Debug, sqlx::FromRow)]
struct LocalHistoryRow {
    chapter_url: String,
    chapter_name: String,
    scroll_ratio: f64,
    page_index: i64,
    read_at: i64,
}

impl Database {
    /// Exports all manga in reading history and/or bookmarks (along with chapters, read flags,
    /// and bookmark state) into a [`SyncBundle`].
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Db`] on SQLite query failure.
    pub(crate) async fn export_sync_bundle(
        &self,
        device_name: &str,
    ) -> Result<SyncBundle, AppError> {
        let rows: Vec<ExportMangaRow> = sqlx::query_as(
            "SELECT
                m.id,
                m.url,
                m.site,
                m.title,
                m.cover_url,
                m.latest_chapter_url,
                m.latest_chapter_name,
                CASE WHEN b.manga_id IS NOT NULL THEN 1 ELSE 0 END AS bookmarked
             FROM manga m
             LEFT JOIN history h ON h.manga_id = m.id
             LEFT JOIN bookmark b ON b.manga_id = m.id
             WHERE h.manga_id IS NOT NULL OR b.manga_id IS NOT NULL
             ORDER BY COALESCE(h.read_at, b.created_at, m.updated_at) DESC",
        )
        .fetch_all(self.pool())
        .await?;

        let mut manga = Vec::with_capacity(rows.len());
        for row in rows {
            let chap_rows: Vec<ExportChapterRow> = sqlx::query_as(
                "SELECT url, name, number, idx, read FROM chapter WHERE manga_id = ? ORDER BY idx ASC",
            )
            .bind(row.id)
            .fetch_all(self.pool())
            .await?;

            let hist = self
                .get_history_for_manga(row.id)
                .await?
                .map(|h| SyncHistoryPayload {
                    chapter_url: h.chapter_url,
                    chapter_name: h.chapter_name,
                    scroll_ratio: h.scroll_ratio,
                    page_index: h.page_index,
                    read_at: h.read_at,
                });

            manga.push(SyncMangaPayload {
                url: row.url,
                site: row.site,
                title: row.title,
                cover_url: row.cover_url,
                latest_chapter_url: row.latest_chapter_url,
                latest_chapter_name: row.latest_chapter_name,
                bookmarked: row.bookmarked != 0,
                chapters: chap_rows
                    .into_iter()
                    .map(|c| SyncChapterPayload {
                        url: c.url,
                        name: c.name,
                        number: c.number,
                        idx: c.idx,
                        read: c.read != 0,
                    })
                    .collect(),
                history: hist,
            });
        }

        Ok(SyncBundle {
            version: 2,
            device_name: device_name.to_owned(),
            exported_at: now_unix_secs(),
            manga,
        })
    }

    /// Merges an incoming [`SyncBundle`] atomically into SQLite using progress-aware deduplication:
    /// - New manga are inserted with all chapters and reading history.
    /// - Duplicate manga union all per-chapter `read` checkmarks.
    /// - Duplicate manga on the same chapter keep the higher `scroll_ratio` (or skip if equal/lower).
    /// - Duplicate manga on different chapters keep whichever history is on a higher chapter number
    ///   (falling back to chapter `idx`, then `read_at`).
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Db`] if the transaction fails.
    pub(crate) async fn merge_sync_bundle(
        &self,
        bundle: &SyncBundle,
    ) -> Result<SyncMergeReport, AppError> {
        let mut report = SyncMergeReport {
            total_received: bundle.manga.len(),
            ..SyncMergeReport::default()
        };
        let now = now_unix_secs();

        let mut tx = self.pool().begin().await?;

        // Build lookup map of existing manga by normalized URL (trailing slash stripped).
        let existing_rows: Vec<(i64, String)> = sqlx::query_as("SELECT id, url FROM manga")
            .fetch_all(&mut *tx)
            .await?;
        let mut existing_by_norm: HashMap<String, i64> = existing_rows
            .into_iter()
            .map(|(id, url)| (norm_url(&url).to_owned(), id))
            .collect();

        for item in &bundle.manga {
            let trimmed_url = item.url.trim();
            if !is_valid_sync_http_url(trimmed_url) || item.title.trim().is_empty() {
                continue;
            }
            let key = norm_url(trimmed_url).to_owned();

            if let Some(&manga_id) = existing_by_norm.get(&key) {
                merge_existing_manga(&mut tx, manga_id, item, now, &mut report).await?;
            } else {
                let manga_id = insert_new_sync_manga(&mut tx, item, now, &mut report).await?;
                existing_by_norm.insert(key, manga_id);
                report.added_manga += 1;
            }
        }

        tx.commit().await?;
        Ok(report)
    }
}

async fn insert_new_sync_manga(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    item: &SyncMangaPayload,
    now: i64,
    report: &mut SyncMergeReport,
) -> Result<i64, AppError> {
    let clean_cover = item
        .cover_url
        .as_deref()
        .map(str::trim)
        .filter(|u| is_valid_sync_http_url(u));
    let clean_latest_url = item
        .latest_chapter_url
        .as_deref()
        .map(str::trim)
        .filter(|u| is_valid_sync_http_url(u));
    let clean_latest_name = item
        .latest_chapter_name
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());

    let res = sqlx::query(
        "INSERT INTO manga
            (url, site, title, cover_url, latest_chapter_url, latest_chapter_name, has_new, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, 0, ?)",
    )
    .bind(item.url.trim())
    .bind(item.site.trim())
    .bind(item.title.trim())
    .bind(clean_cover)
    .bind(clean_latest_url)
    .bind(clean_latest_name)
    .bind(now)
    .execute(&mut **tx)
    .await?;

    let manga_id = res.last_insert_rowid();
    upsert_sync_chapters(tx, manga_id, &item.chapters).await?;

    if item.bookmarked {
        union_bookmark(tx, manga_id, now, report).await?;
    }

    if let Some(hist) = item
        .history
        .as_ref()
        .filter(|h| is_valid_sync_http_url(h.chapter_url.trim()))
    {
        write_history_row(tx, manga_id, hist).await?;
    }

    Ok(manga_id)
}

async fn union_bookmark(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    manga_id: i64,
    now: i64,
    report: &mut SyncMergeReport,
) -> Result<(), AppError> {
    let res = sqlx::query(
        "INSERT INTO bookmark (manga_id, created_at) VALUES (?, ?)
         ON CONFLICT(manga_id) DO NOTHING",
    )
    .bind(manga_id)
    .bind(now)
    .execute(&mut **tx)
    .await?;
    if res.rows_affected() > 0 {
        report.synced_bookmarks += 1;
    }
    Ok(())
}

async fn update_merged_manga_metadata(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    manga_id: i64,
    item: &SyncMangaPayload,
    now: i64,
) -> Result<(), AppError> {
    let clean_cover = item
        .cover_url
        .as_deref()
        .map(str::trim)
        .filter(|u| is_valid_sync_http_url(u));

    // Derive the furthest known latest chapter from the unioned `chapter` table so an older
    // device's snapshot never overwrites a newer local `latest_chapter_url` backwards.
    let highest_chap: Option<(String, String)> = sqlx::query_as(
        "SELECT url, name FROM chapter WHERE manga_id = ?
         ORDER BY COALESCE(number, CAST(idx AS REAL)) DESC, idx DESC LIMIT 1",
    )
    .bind(manga_id)
    .fetch_optional(&mut **tx)
    .await?;

    let (derived_latest_url, derived_latest_name) = match highest_chap {
        Some((u, n)) => (Some(u), Some(n)),
        None => (
            item.latest_chapter_url
                .as_deref()
                .map(str::trim)
                .filter(|u| is_valid_sync_http_url(u))
                .map(str::to_owned),
            item.latest_chapter_name
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_owned),
        ),
    };

    sqlx::query(
        "UPDATE manga
         SET cover_url = COALESCE(cover_url, ?),
             latest_chapter_url = COALESCE(?, latest_chapter_url),
             latest_chapter_name = COALESCE(?, latest_chapter_name),
             updated_at = ?
         WHERE id = ?",
    )
    .bind(clean_cover)
    .bind(derived_latest_url.as_deref())
    .bind(derived_latest_name.as_deref())
    .bind(now)
    .bind(manga_id)
    .execute(&mut **tx)
    .await?;

    // Clear `has_new` if the latest chapter is now marked read after unioning chapter read flags.
    sqlx::query(
        "UPDATE manga SET has_new = 0
         WHERE id = ? AND EXISTS (
             SELECT 1 FROM chapter c
             WHERE c.manga_id = manga.id
               AND RTRIM(c.url, '/') = RTRIM(manga.latest_chapter_url, '/')
               AND c.read = 1
         )",
    )
    .bind(manga_id)
    .execute(&mut **tx)
    .await?;

    Ok(())
}

async fn merge_existing_manga(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    manga_id: i64,
    item: &SyncMangaPayload,
    now: i64,
    report: &mut SyncMergeReport,
) -> Result<(), AppError> {
    upsert_sync_chapters(tx, manga_id, &item.chapters).await?;
    update_merged_manga_metadata(tx, manga_id, item, now).await?;
    if item.bookmarked {
        union_bookmark(tx, manga_id, now, report).await?;
    }

    let Some(incoming) = item
        .history
        .as_ref()
        .filter(|h| is_valid_sync_http_url(h.chapter_url.trim()))
    else {
        report.skipped_same_chapter += 1;
        return Ok(());
    };

    let local_hist: Option<LocalHistoryRow> = sqlx::query_as(
        "SELECT chapter_url, chapter_name, scroll_ratio, page_index, read_at FROM history WHERE manga_id = ?",
    )
    .bind(manga_id)
    .fetch_optional(&mut **tx)
    .await?;

    let Some(local) = local_hist else {
        write_history_row(tx, manga_id, incoming).await?;
        report.updated_newer_chapter += 1;
        return Ok(());
    };

    if norm_url(&local.chapter_url) == norm_url(&incoming.chapter_url) {
        let inc_ratio = sanitize_ratio(incoming.scroll_ratio);
        let inc_page = incoming.page_index.max(0);
        if inc_ratio > local.scroll_ratio + 0.01 || inc_page > local.page_index {
            let best_ratio = local.scroll_ratio.max(inc_ratio);
            let best_page = local.page_index.max(inc_page);
            sqlx::query(
                "UPDATE history SET scroll_ratio = ?, page_index = ?, read_at = MAX(read_at, ?) WHERE manga_id = ?",
            )
            .bind(best_ratio)
            .bind(best_page)
            .bind(incoming.read_at)
            .bind(manga_id)
            .execute(&mut **tx)
            .await?;
        }
        report.skipped_same_chapter += 1;
        return Ok(());
    }

    // Different chapters: compare chapter reading progress (higher chapter number / idx wins).
    let chap_rows: Vec<(String, Option<f64>, i64)> =
        sqlx::query_as("SELECT url, number, idx FROM chapter WHERE manga_id = ?")
            .bind(manga_id)
            .fetch_all(&mut **tx)
            .await?;

    let local_rank = chapter_progress_rank(
        &chap_rows,
        &local.chapter_url,
        Some(local.chapter_name.as_str()),
    );
    let incoming_rank = chapter_progress_rank(
        &chap_rows,
        &incoming.chapter_url,
        Some(incoming.chapter_name.as_str()),
    );

    let incoming_is_further = match (incoming_rank, local_rank) {
        (Some((inc_num, inc_idx)), Some((loc_num, loc_idx))) => {
            if (inc_num - loc_num).abs() > f64::EPSILON {
                inc_num > loc_num
            } else if inc_idx != loc_idx {
                inc_idx > loc_idx
            } else {
                incoming.read_at > local.read_at
            }
        }
        (Some(_), None) => true,
        (None, Some(_)) => false,
        (None, None) => incoming.read_at > local.read_at,
    };

    if incoming_is_further {
        write_history_row(tx, manga_id, incoming).await?;
        report.updated_newer_chapter += 1;
    } else {
        report.kept_local_newer += 1;
    }

    Ok(())
}

async fn upsert_sync_chapters(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    manga_id: i64,
    chapters: &[SyncChapterPayload],
) -> Result<(), AppError> {
    for ch in chapters {
        let trimmed_url = ch.url.trim();
        if !is_valid_sync_http_url(trimmed_url) {
            continue;
        }
        let clean_num = ch.number.filter(|n| n.is_finite());
        let read_int: i64 = i64::from(ch.read);
        sqlx::query(
            "INSERT INTO chapter (manga_id, url, name, number, idx, read)
             VALUES (?, ?, ?, ?, ?, ?)
             ON CONFLICT(manga_id, url) DO UPDATE SET
                name = excluded.name,
                number = COALESCE(excluded.number, chapter.number),
                idx = excluded.idx,
                read = MAX(chapter.read, excluded.read)",
        )
        .bind(manga_id)
        .bind(trimmed_url)
        .bind(ch.name.trim())
        .bind(clean_num)
        .bind(ch.idx)
        .bind(read_int)
        .execute(&mut **tx)
        .await?;
    }
    Ok(())
}

async fn write_history_row(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    manga_id: i64,
    hist: &SyncHistoryPayload,
) -> Result<(), AppError> {
    let ratio = sanitize_ratio(hist.scroll_ratio);
    let page = hist.page_index.max(0);
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
    .bind(hist.chapter_url.trim())
    .bind(hist.chapter_name.trim())
    .bind(ratio)
    .bind(page)
    .bind(hist.read_at)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

fn chapter_progress_rank(
    chapters: &[(String, Option<f64>, i64)],
    chapter_url: &str,
    chapter_name: Option<&str>,
) -> Option<(f64, i64)> {
    let target = norm_url(chapter_url);
    if let Some((_, num, idx)) = chapters.iter().find(|(u, _, _)| norm_url(u) == target) {
        if let Some(n) = num {
            return Some((*n, *idx));
        }
        return Some((f64::from(i32::try_from(*idx).unwrap_or(i32::MAX)), *idx));
    }

    // Fallback: extract chapter number from chapter name or URL path using mangacore heuristic.
    chapter_name
        .and_then(mangacore::parse::chapters::extract_chapter_number)
        .or_else(|| mangacore::parse::chapters::extract_chapter_number(chapter_url))
        .map(|n| (f64::from(n), 0))
}

fn sanitize_ratio(raw: f64) -> f64 {
    if raw.is_finite() {
        raw.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

fn norm_url(raw: &str) -> &str {
    raw.trim().trim_end_matches('/')
}

fn is_valid_sync_http_url(raw: &str) -> bool {
    url::Url::parse(raw).is_ok_and(|u| mangacore::http::validate_public_http_url(&u).is_ok())
}
