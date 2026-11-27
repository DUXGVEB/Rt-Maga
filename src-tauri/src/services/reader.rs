//! Chapter loading, obsolete-scrape cancellation, neighbor-chapter fallback from SQLite,
//! and image token signing.

use std::sync::Arc;
use std::sync::atomic::Ordering;

use mangacore::{normalize_url, redact_url_for_diagnostics};
use serde::{Deserialize, Serialize};

use crate::error::AppError;
use crate::image_proxy::encode_page_token;
use crate::repo::diagnostics::NewDiagnosticLog;
use crate::state::AppState;

/// Priority distinguishing user-initiated chapter loads (which cancel older obsolete scrapes)
/// from background next-chapter prefetches.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub(crate) enum ChapterLoadPriority {
    /// Active chapter requested directly by the user.
    #[default]
    Primary,
    /// Speculative prefetch for seamless auto-append reading.
    Prefetch,
}

/// Resolved chapter payload delivered to the Svelte vertical/horizontal reader.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ChapterView {
    pub url: String,
    pub name: String,
    pub images: Vec<String>,
    pub prev: Option<String>,
    pub next: Option<String>,
    pub manga_id: Option<i64>,
}

/// Fetches a chapter's ordered image tokens and neighbor URLs, cancelling obsolete in-flight
/// primary loads if the user switches chapters rapidly.
///
/// # Errors
///
/// Returns [`AppError`] on invalid URL, scraping failure, superseded cancellation, or database error.
pub(crate) async fn get_chapter(
    state: &AppState,
    raw_url: &str,
    known_manga_id: Option<i64>,
    priority: ChapterLoadPriority,
) -> Result<ChapterView, AppError> {
    if crate::services::comics::is_tachibk_url(raw_url.trim()) {
        return Err(AppError::NotFound {
            message: "This chapter is a metadata-only entry imported from a .tachibk backup with no local comic files linked yet. Attach local CBZ/CBR/PDF/folder files on the manga page to read offline.".to_owned(),
        });
    }

    if crate::services::comics::is_local_comic_url(raw_url.trim()) {
        let local_url = url::Url::parse(raw_url.trim()).map_err(|e| AppError::InvalidUrl {
            message: format!("invalid local comic chapter URL: {e}"),
        })?;
        let (chapter, manga_id) =
            crate::services::comics::load_local_chapter(state, &local_url).await?;
        return build_chapter_view(state, &chapter, known_manga_id.or(Some(manga_id))).await;
    }

    let url = normalize_url(raw_url)?;

    let chapter = if let Some(cached) = state.chapters.get(&url).await {
        cached
    } else {
        let policy = state.current_settings().await.headless_policy();
        let scrape_res = fetch_chapter_respecting_cancel(state, &url, policy, priority).await?;
        let fetched = match scrape_res {
            Ok((c, _report)) => Arc::new(c),
            Err(boxed) => {
                let (err, report) = *boxed;
                let detail_json =
                    serde_json::to_string_pretty(&report).unwrap_or_else(|_| err.to_string());
                let app_err = AppError::from_scrape_and_report(&err, report.clone());
                let src = url
                    .host_str()
                    .map(|h| h.trim_start_matches("www.").to_ascii_lowercase());
                let redacted_url = redact_url_for_diagnostics(url.as_str());
                let _ = state
                    .db
                    .record_diagnostic_log(&NewDiagnosticLog {
                        level: "error".to_owned(),
                        category: app_err.category().to_owned(),
                        message: format!("Failed to load chapter pages ({:?})", report.error_code),
                        detail: Some(detail_json),
                        source: src,
                        chapter_url: Some(redacted_url.clone()),
                        failed_url: Some(redacted_url),
                        resolved_url: report.final_url.clone(),
                        http_status: report.http_status.map(i64::from),
                        adapter: report.adapter_name.clone(),
                        parsing_stage: report.failed_stage.clone(),
                        ..NewDiagnosticLog::default()
                    })
                    .await;
                return Err(app_err);
            }
        };
        state
            .chapters
            .insert(url.clone(), Arc::clone(&fetched))
            .await;
        fetched
    };

    build_chapter_view(state, &chapter, known_manga_id).await
}

async fn fetch_chapter_respecting_cancel(
    state: &AppState,
    url: &url::Url,
    policy: mangacore::HeadlessPolicy,
    priority: ChapterLoadPriority,
) -> Result<
    Result<
        (mangacore::Chapter, mangacore::SourceDiagnosticReport),
        Box<(mangacore::ScrapeError, mangacore::SourceDiagnosticReport)>,
    >,
    AppError,
> {
    if priority != ChapterLoadPriority::Primary {
        return Ok(state.registry.chapter_with_diagnostics(url, policy).await);
    }

    let my_seq = state.chapter_load_seq.fetch_add(1, Ordering::AcqRel) + 1;
    let _ = state.chapter_cancel_tx.send(my_seq);
    let mut rx = state.chapter_cancel_tx.subscribe();

    tokio::select! {
        res = state.registry.chapter_with_diagnostics(url, policy) => Ok(res),
        () = async move {
            while rx.changed().await.is_ok() {
                if *rx.borrow() != my_seq {
                    break;
                }
            }
        } => Err(AppError::Internal {
            message: "chapter load cancelled because user switched chapters".to_owned(),
        }),
    }
}

async fn build_chapter_view(
    state: &AppState,
    chapter: &mangacore::Chapter,
    known_manga_id: Option<i64>,
) -> Result<ChapterView, AppError> {
    let manga_id = match known_manga_id {
        Some(id) => Some(id),
        None => {
            state
                .db
                .find_manga_id_by_chapter_url(chapter.url.as_str())
                .await?
        }
    };

    let mut prev = chapter.prev.as_ref().map(|u| u.as_str().to_owned());
    let mut next = chapter.next.as_ref().map(|u| u.as_str().to_owned());
    let mut display_name = chapter.name.clone();

    // Fill missing prev/next and canonical chapter name from SQLite chapter list.
    if let Some(id) = manga_id {
        let chapters = state.db.list_chapters(id).await?;
        let target_norm = chapter.url.as_str().trim_end_matches('/');
        if let Some(pos) = chapters
            .iter()
            .position(|c| c.url.trim_end_matches('/') == target_norm)
        {
            display_name.clone_from(&chapters[pos].name);
            if prev.is_none() && pos > 0 {
                prev = Some(chapters[pos - 1].url.clone());
            }
            if next.is_none() && pos + 1 < chapters.len() {
                next = Some(chapters[pos + 1].url.clone());
            }
        }
    }

    let site_for_cache = if let Some(id) = manga_id {
        state.db.get_manga_by_id(id).await?.map(|m| m.site)
    } else {
        chapter
            .url
            .host_str()
            .map(|h| h.trim_start_matches("www.").to_ascii_lowercase())
    };

    let images = chapter
        .images
        .iter()
        .map(|img| {
            encode_page_token(
                img,
                manga_id,
                site_for_cache.as_deref(),
                Some(chapter.url.as_str()),
                &state.proxy_key,
            )
        })
        .collect();

    Ok(ChapterView {
        url: chapter.url.as_str().to_owned(),
        name: display_name,
        images,
        prev,
        next,
        manga_id,
    })
}
