//! Chapter loading, obsolete-scrape cancellation, neighbor-chapter fallback from SQLite,
//! and image token signing.

use std::sync::Arc;
use std::sync::atomic::Ordering;

use bytes::Bytes;
use mangacore::{normalize_url, redact_url_for_diagnostics};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

use crate::cache::ImageCache;
use crate::cache::image::CacheEntryMeta;
use crate::error::AppError;
use crate::image_proxy::{encode_cover_token, encode_page_token, resolve_image_bytes};
use crate::repo::diagnostics::NewDiagnosticLog;
use crate::state::AppState;

/// Outcome returned when saving a reader page image directly to Pictures/Gallery.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SavedImageOutcome {
    pub saved_path: String,
    pub file_name: String,
    pub bytes: u64,
}

/// Outcome returned after persisting a reader page as the manga's new cover.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SetCoverOutcome {
    pub manga_id: i64,
    pub cover_token: String,
}

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

fn infer_image_ext_and_mime(bytes: &[u8], declared_mime: &str) -> (&'static str, &'static str) {
    if let Some(kind) = infer::get(bytes) {
        return match kind.mime_type() {
            "image/png" => ("png", "image/png"),
            "image/webp" => ("webp", "image/webp"),
            "image/gif" => ("gif", "image/gif"),
            "image/avif" => ("avif", "image/avif"),
            "image/bmp" => ("bmp", "image/bmp"),
            _ => ("jpg", "image/jpeg"),
        };
    }
    let lower = declared_mime.to_ascii_lowercase();
    if lower.contains("png") {
        ("png", "image/png")
    } else if lower.contains("webp") {
        ("webp", "image/webp")
    } else if lower.contains("gif") {
        ("gif", "image/gif")
    } else if lower.contains("avif") {
        ("avif", "image/avif")
    } else {
        ("jpg", "image/jpeg")
    }
}

fn sanitize_slug(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len().min(40));
    let mut prev_sep = false;
    for ch in raw.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            prev_sep = false;
        } else if !prev_sep && !out.is_empty() {
            out.push('_');
            prev_sep = true;
        }
        if out.len() >= 40 {
            break;
        }
    }
    let trimmed = out.trim_matches('_');
    if trimmed.is_empty() {
        "rtmaga".to_owned()
    } else {
        trimmed.to_owned()
    }
}

/// Saves a reader page image (reusing the cached disk copy without re-fetching when available)
/// directly to Android `MediaStore` Gallery or Windows Pictures/Downloads.
///
/// # Errors
///
/// Returns [`AppError`] if the image token is invalid, fetching fails, or saving to storage fails.
pub(crate) async fn download_reader_image(
    app: &AppHandle,
    state: &AppState,
    token: &str,
    manga_title: Option<&str>,
    page_index: Option<u32>,
) -> Result<SavedImageOutcome, AppError> {
    let (bytes, declared_mime, _verified) = resolve_image_bytes(state, token).await?;
    let (ext, mime) = infer_image_ext_and_mime(&bytes, &declared_mime);
    let slug = sanitize_slug(manga_title.unwrap_or("rtmaga"));
    let page_num = page_index.map_or(1, |idx| idx.saturating_add(1));
    let ts = crate::repo::now_unix_secs();
    let file_name = format!("{slug}_p{page_num:03}_{ts}.{ext}");
    let byte_len = u64::try_from(bytes.len()).unwrap_or(0);

    #[cfg(target_os = "android")]
    {
        let cache_dir = app
            .path()
            .app_cache_dir()
            .unwrap_or_else(|_| state.data_dir.join("tmp"));
        let nonce: u64 = rand::random();
        let tmp_path = cache_dir.join(format!("rtmaga_img_{nonce:016x}.{ext}"));
        let tmp_for_write = tmp_path.clone();
        tokio::task::spawn_blocking(move || -> Result<(), AppError> {
            if let Some(parent) = tmp_for_write.parent() {
                std::fs::create_dir_all(parent).map_err(|e| AppError::Internal {
                    message: format!("failed to prepare image cache dir: {e}"),
                })?;
            }
            std::fs::write(&tmp_for_write, &bytes).map_err(|e| AppError::Internal {
                message: format!("failed to stage image for gallery: {e}"),
            })
        })
        .await
        .map_err(|e| AppError::Internal {
            message: format!("image stage worker failed: {e}"),
        })??;

        let res = crate::android_env::save_image_to_gallery(app, &tmp_path, &file_name, mime);
        let _ = tokio::fs::remove_file(&tmp_path).await;
        let reply = res?;
        Ok(SavedImageOutcome {
            saved_path: reply.saved_path,
            file_name,
            bytes: byte_len,
        })
    }

    #[cfg(not(target_os = "android"))]
    {
        let _ = mime;
        let base_dir = app
            .path()
            .picture_dir()
            .or_else(|_| app.path().download_dir())
            .or_else(|_| app.path().app_local_data_dir())
            .map_err(|e| AppError::Internal {
                message: format!("failed to resolve Pictures/Downloads directory: {e}"),
            })?;
        let target_dir = base_dir.join("RtMaga");
        let dest_path = target_dir.join(&file_name);
        let dest_for_write = dest_path.clone();

        tokio::task::spawn_blocking(move || -> Result<(), AppError> {
            std::fs::create_dir_all(&target_dir).map_err(|e| AppError::Internal {
                message: format!("failed to create Pictures/RtMaga folder: {e}"),
            })?;
            let nonce: u64 = rand::random();
            let tmp = dest_for_write.with_extension(format!("{ext}.tmp.{nonce:08x}"));
            if let Err(err) = std::fs::write(&tmp, &bytes) {
                let _ = std::fs::remove_file(&tmp);
                return Err(AppError::Internal {
                    message: format!("failed to write image file: {err}"),
                });
            }
            if let Err(err) = std::fs::rename(&tmp, &dest_for_write) {
                let _ = std::fs::remove_file(&tmp);
                return Err(AppError::Internal {
                    message: format!("failed to finalize saved image: {err}"),
                });
            }
            Ok(())
        })
        .await
        .map_err(|e| AppError::Internal {
            message: format!("image save worker failed: {e}"),
        })??;

        let display_path = dest_path
            .to_string_lossy()
            .trim_start_matches(r"\\?\")
            .to_owned();
        Ok(SavedImageOutcome {
            saved_path: display_path,
            file_name,
            bytes: byte_len,
        })
    }
}

/// Persists a reader page image as the cover for `manga_id`, caching it persistently in the
/// `covers/` bucket so it survives transient chapter page purges, and returns a fresh signed
/// `cover_token` for immediate reactive UI updates.
///
/// # Errors
///
/// Returns [`AppError`] if the manga does not exist, the image token is invalid, or DB update fails.
pub(crate) async fn set_manga_cover_from_page(
    state: &AppState,
    manga_id: i64,
    token: &str,
) -> Result<SetCoverOutcome, AppError> {
    let row = state
        .db
        .get_manga_by_id(manga_id)
        .await?
        .ok_or_else(|| AppError::NotFound {
            message: format!("manga {manga_id} not found"),
        })?;

    let (bytes, declared_mime, verified) = resolve_image_bytes(state, token).await?;
    let (_ext, mime) = infer_image_ext_and_mime(&bytes, &declared_mime);
    let max_mb = state.current_settings().await.cache.max_mb;

    let cover_url = if crate::services::comics::is_local_comic_url(&verified.url) {
        let ts = crate::repo::now_unix_secs();
        format!("http://local.comic/cover/custom/{manga_id}/{ts}")
    } else {
        verified.url.clone()
    };

    let cover_key = ImageCache::key_for_url(&cover_url);
    state
        .images
        .put(
            &cover_key,
            &Bytes::from(bytes),
            mime,
            max_mb,
            CacheEntryMeta {
                site: Some(&row.site),
                manga_id: Some(manga_id),
                is_cover: true,
                chapter_url: None,
            },
        )
        .await;

    state.db.set_manga_cover_url(manga_id, &cover_url).await?;

    let cover_token = encode_cover_token(
        &cover_url,
        &row.url,
        Some(manga_id),
        Some(&row.site),
        &state.proxy_key,
    );

    Ok(SetCoverOutcome {
        manga_id,
        cover_token,
    })
}
