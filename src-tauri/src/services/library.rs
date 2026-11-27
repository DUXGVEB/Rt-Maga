//! Manga resolution, catalog browsing/filtering, detail loading, and reading history workflows.

use std::sync::Arc;

use mangacore::{
    CatalogFilterQuery, CatalogPage, CmsKind, RecognitionDiagnostic, RecognitionStatus, Resolved,
    ScrapeError, SiteFilterSchema, UrlKind, UserQueryKind, apply_page_to_template,
    browse_or_filter_cms, classify_user_input, match_builtin_adapter_id, merge_filter_schemas,
    normalize_url, resolve_exact_from_source, rewrite_domain_override,
};
use serde::Serialize;
use url::Url;

use crate::error::AppError;
use crate::image_proxy::encode_cover_token;
use crate::repo::bookmark::{BookmarkEntryRow, BookmarkSort};
use crate::repo::diagnostics::NewDiagnosticLog;
use crate::repo::history::{HistoryEntryRow, HistoryProgressRow, LastReadingDestination};
use crate::repo::manga::{ChapterRow, HideAction, MangaRow};
use crate::repo::notification::NotificationRow;
use crate::services::sources::{
    AutoRecordDeps, default_schema_for_host_or_parser, spawn_auto_record_from_catalog,
    spawn_auto_record_from_manga, spawn_next_catalog_page_prefetch,
};
use crate::settings::ReaderSettings;
use crate::state::AppState;

/// Controls whether loading a manga uses local DB data or refetches from the remote website.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FetchMode {
    /// Return locally stored metadata and chapters.
    LocalOnly,
    /// Re-scrape the manga page from the web and update SQLite.
    RefreshFromWeb,
}

/// Chapter entry sent to the Svelte UI.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ChapterRefView {
    pub url: String,
    pub name: String,
    pub number: Option<f64>,
    pub idx: i64,
    pub read: bool,
    pub volume: Option<String>,
    pub published_at: Option<String>,
    pub language: Option<String>,
    pub source_id: Option<String>,
}

/// Reading progress snapshot attached to a manga view.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProgressView {
    pub chapter_url: String,
    pub chapter_name: String,
    pub scroll_ratio: f64,
    pub page_index: i64,
    pub read_at: i64,
}

/// Full manga detail payload sent to the Svelte UI.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
#[expect(
    clippy::struct_excessive_bools,
    reason = "JSON view DTO carrying independent UI status flags for Svelte"
)]
pub(crate) struct MangaView {
    pub id: i64,
    pub url: String,
    pub site: String,
    pub title: String,
    pub cover_token: Option<String>,
    pub has_new: bool,
    pub bookmarked: bool,
    pub hidden: bool,
    pub unread_count: i64,
    pub is_completed: bool,
    pub reader_mode_override: Option<String>,
    pub reader_settings: Option<ReaderSettings>,
    pub detected_webtoon: bool,
    pub detected_layout: Option<String>,
    pub latest_chapter_name: Option<String>,
    pub source_id: Option<String>,
    pub canonical_url: Option<String>,
    pub alt_titles: Vec<String>,
    pub original_title: Option<String>,
    pub authors: Vec<String>,
    pub artists: Vec<String>,
    pub description: Option<String>,
    pub genres: Vec<String>,
    pub tags: Vec<String>,
    pub status: Option<String>,
    pub language: Option<String>,
    pub available_languages: Vec<String>,
    pub chapters: Vec<ChapterRefView>,
    pub progress: Option<ProgressView>,
}

/// Hidden manga record shown in Settings -> Hidden Manga Manager.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct HiddenMangaView {
    pub id: i64,
    pub url: String,
    pub site: String,
    pub title: String,
    pub cover_token: Option<String>,
    pub latest_chapter_name: Option<String>,
}

/// Single manga entry extracted from a website homepage, catalog, category, or search page.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CatalogEntryView {
    pub url: String,
    pub title: String,
    pub alt_titles: Vec<String>,
    pub cover_token: Option<String>,
    pub latest_chapter: Option<String>,
    pub status: Option<String>,
    pub genres: Vec<String>,
    pub source_id: Option<String>,
    pub chapter_count: Option<u32>,
    pub updated_at: Option<String>,
}

/// Category or navigation shortcut extracted from a website catalog/homepage.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NavLinkView {
    pub label: String,
    pub url: String,
    pub kind: UrlKind,
}

/// Pagination metadata for in-app catalog browsing.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PaginationView {
    pub current_page: u32,
    pub total_pages: Option<u32>,
    pub prev_url: Option<String>,
    pub next_url: Option<String>,
    pub page_url_template: Option<String>,
}

/// Resolved catalog/homepage/search view sent to the Svelte Browse page.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CatalogPageView {
    pub url: String,
    pub site: String,
    pub site_name: String,
    pub title: String,
    pub url_kind: UrlKind,
    pub query: Option<String>,
    pub search_url_template: Option<String>,
    pub entries: Vec<CatalogEntryView>,
    pub categories: Vec<NavLinkView>,
    pub pagination: PaginationView,
    pub filter_schema: SiteFilterSchema,
    pub source_id: Option<i64>,
}

/// Response returned when the user submits any homepage, catalog, search, manga, or chapter URL.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ResolveResponse {
    pub kind: String,
    pub manga: Option<MangaView>,
    pub start_at: Option<ChapterRefView>,
    pub catalog: Option<CatalogPageView>,
    pub diagnostic: RecognitionDiagnostic,
}

/// Reading history entry rendered on Home and History pages.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct HistoryItemView {
    pub manga_id: i64,
    pub manga_url: String,
    pub site: String,
    pub title: String,
    pub cover_token: Option<String>,
    pub chapter_url: String,
    pub chapter_name: String,
    pub scroll_ratio: f64,
    pub page_index: i64,
    pub read_at: i64,
    pub added_at: i64,
    pub latest_chapter_url: Option<String>,
    pub latest_chapter_name: Option<String>,
    pub has_new: bool,
    pub bookmarked: bool,
    pub total_chapters: i64,
    pub unread_count: i64,
    pub is_completed: bool,
    pub current_chapter_idx: Option<i64>,
}

/// Bookmarked manga entry rendered on the dedicated Bookmarks screen.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BookmarkItemView {
    pub manga_id: i64,
    pub manga_url: String,
    pub site: String,
    pub title: String,
    pub cover_token: Option<String>,
    pub bookmarked_at: i64,
    pub latest_chapter_url: Option<String>,
    pub latest_chapter_name: Option<String>,
    pub has_new: bool,
    pub total_chapters: i64,
    pub unread_count: i64,
    pub is_completed: bool,
    pub chapter_url: Option<String>,
    pub chapter_name: Option<String>,
    pub scroll_ratio: Option<f64>,
    pub page_index: Option<i64>,
    pub read_at: Option<i64>,
    pub current_chapter_idx: Option<i64>,
    pub reader_mode_override: Option<String>,
    pub detected_webtoon: bool,
}

/// Notification Center entry rendered in the in-app notification drawer.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NotificationItemView {
    pub id: i64,
    pub manga_id: i64,
    pub manga_title: String,
    pub cover_token: Option<String>,
    pub chapter_range: String,
    pub latest_chapter_url: String,
    pub chapter_count: i64,
    pub read: bool,
    pub created_at: i64,
}

/// Resolves an arbitrary pasted link (web URL or `rtmaga://` deep link), persists manga/filter
/// metadata in SQLite, auto-records discovered sources, and returns the UI view.
///
/// # Errors
///
/// Returns [`AppError`] on invalid URL, network/scrape failure, or database error.
pub(crate) async fn resolve_url(
    state: &Arc<AppState>,
    raw_url: &str,
) -> Result<ResolveResponse, AppError> {
    let policy = state.current_settings().await.headless_policy();

    if let UserQueryKind::AppDeepLink {
        manga_id,
        source_key,
        url,
        title,
    } = classify_user_input(raw_url)
    {
        return resolve_app_deep_link(state, raw_url, manga_id, source_key, url, title, policy)
            .await;
    }

    let effective_input = rewrite_raw_url_if_mirrored(state, raw_url).await;
    match state
        .registry
        .resolve_with_diagnostics(&effective_input, policy)
        .await
    {
        Ok((resolved, diagnostic)) => {
            let parsed_target = Url::parse(
                diagnostic
                    .resolved_url
                    .as_deref()
                    .unwrap_or(&effective_input),
            )
            .or_else(|_| normalize_url(&effective_input))
            .ok();
            finish_resolved_response(state, parsed_target, resolved, diagnostic).await
        }
        Err(boxed) => record_and_return_resolve_error(state, boxed.0, boxed.1).await,
    }
}

/// Browses, paginates, or applies multi-tag/chapter/recency filters directly against an arbitrary
/// catalog/category/homepage URL, backed by RAM `CatalogCache` and next-page prefetching.
///
/// # Errors
///
/// Returns [`AppError`] on invalid URL or upstream scraping failure.
#[expect(
    clippy::too_many_lines,
    clippy::fn_params_excessive_bools,
    reason = "handles cached catalog browsing, pagination jumps, and dynamic filter pushdown"
)]
pub(crate) async fn browse_catalog_url(
    state: &Arc<AppState>,
    raw_url: &str,
    filters: Option<CatalogFilterQuery>,
    page: u32,
    force_refresh: bool,
) -> Result<CatalogPageView, AppError> {
    let effective_input = rewrite_raw_url_if_mirrored(state, raw_url).await;
    let parsed_url = normalize_url(&effective_input).map_err(AppError::from)?;
    let filter_query = filters.unwrap_or_default();
    let target_page = page.max(1);
    let filter_sig = serde_json::to_string(&filter_query).unwrap_or_default();
    let cache_key = format!("url:{}|p:{target_page}|f:{filter_sig}", parsed_url.as_str());

    if !force_refresh && let Some(cached) = state.catalogs.get(&cache_key).await {
        return Ok(cached);
    }

    let host = parsed_url.host_str().unwrap_or("");
    let matched_source = state
        .db
        .find_matching_saved_source(None, Some(host))
        .await
        .ok()
        .flatten();

    let mut cat = if filter_query.is_empty() {
        let concrete_url = if target_page > 1
            && let Some(tmpl) = mangacore::template_from_numbered_url(&parsed_url)
            && let Some(jumped) = apply_page_to_template(&tmpl, target_page)
        {
            jumped
        } else {
            parsed_url.clone()
        };
        let policy = state.current_settings().await.headless_policy();
        let (resolved, diag) = match state
            .registry
            .resolve_with_diagnostics(concrete_url.as_str(), policy)
            .await
        {
            Ok(pair) => pair,
            Err(boxed) => return record_and_return_resolve_error(state, boxed.0, boxed.1).await,
        };
        match resolved {
            Resolved::Catalog(c) => *c,
            other => {
                let finish =
                    finish_resolved_response(state, Some(concrete_url), other, diag).await?;
                if let Some(cview) = finish.catalog {
                    return Ok(cview);
                }
                return Err(AppError::InvalidUrl {
                    message: "target URL resolved to a single manga rather than a catalog page"
                        .to_owned(),
                });
            }
        }
    } else {
        let (kind, parser_ref, search_tmpl, filter_base) =
            if let Some(saved) = matched_source.as_ref() {
                (
                    CmsKind::from_storage_str(&saved.source_kind),
                    saved.parser_ref.clone(),
                    saved.search_template.clone(),
                    Url::parse(&saved.base_url).unwrap_or_else(|_| parsed_url.clone()),
                )
            } else {
                let builtin = match_builtin_adapter_id(host);
                let kind = if builtin.is_some() {
                    CmsKind::BuiltInAdapter
                } else {
                    CmsKind::UniversalHeuristic
                };
                (
                    kind,
                    builtin.map(str::to_owned),
                    None,
                    mangacore::normalize_source_root(parsed_url.as_str())
                        .unwrap_or_else(|_| parsed_url.clone()),
                )
            };
        let known_schema = load_known_schema_for_url(state, matched_source.as_ref(), host).await;
        browse_or_filter_cms(
            state.registry.http(),
            kind,
            &filter_base,
            Some(&parsed_url),
            parser_ref.as_deref(),
            search_tmpl.as_deref(),
            Some(&known_schema),
            &filter_query,
            target_page,
        )
        .await
        .map_err(AppError::from)?
    };

    if let Some(ref saved) = matched_source
        && (cat.site_name.trim().is_empty() || cat.site_name == cat.site.as_storage_key())
    {
        cat.site_name.clone_from(&saved.display_name);
    }

    let view = enrich_and_cache_catalog_view(
        state,
        cat,
        matched_source.as_ref().map(|s| s.id),
        Some(cache_key),
    )
    .await;
    Ok(view)
}

async fn load_known_schema_for_url(
    state: &AppState,
    saved: Option<&crate::repo::sources::SavedSourceRow>,
    host: &str,
) -> SiteFilterSchema {
    let mut base =
        default_schema_for_host_or_parser(Some(host), saved.and_then(|s| s.parser_ref.as_deref()));
    if let Some(s) = saved
        && let Some(ref raw) = s.filter_schema_json
        && let Ok(parsed) = serde_json::from_str::<SiteFilterSchema>(raw)
    {
        base = merge_filter_schemas(Some(&base), parsed);
    }
    if let Ok(Some(host_cached)) = state.db.get_host_filter_schema(host).await {
        base = merge_filter_schemas(Some(&base), host_cached);
    }
    base
}

pub(crate) async fn enrich_and_cache_catalog_view(
    state: &Arc<AppState>,
    mut cat: CatalogPage,
    explicit_source_id: Option<i64>,
    cache_key: Option<String>,
) -> CatalogPageView {
    let host = cat.url.host_str().unwrap_or("").to_owned();
    let matched_source = if let Some(sid) = explicit_source_id {
        state.db.get_saved_source_by_id(sid).await.ok().flatten()
    } else {
        state
            .db
            .find_matching_saved_source(None, Some(&host))
            .await
            .ok()
            .flatten()
    };

    let builtin_schema = default_schema_for_host_or_parser(
        Some(&host),
        matched_source
            .as_ref()
            .and_then(|s| s.parser_ref.as_deref()),
    );
    cat.filter_schema = merge_filter_schemas(Some(&builtin_schema), cat.filter_schema);

    if !host.is_empty()
        && let Ok(merged_host) = state
            .db
            .merge_host_filter_schema(&host, &cat.filter_schema)
            .await
    {
        cat.filter_schema = merged_host;
    }

    let resolved_source_id = if let Some(ref saved) = matched_source {
        if let Ok(merged_src) = state
            .db
            .merge_saved_source_filter_schema(saved.id, &cat.filter_schema)
            .await
        {
            cat.filter_schema = merged_src;
        }
        Some(saved.id)
    } else {
        explicit_source_id
    };

    let next_to_prefetch = cat.pagination.next_url.clone();
    let view = map_catalog_view(cat, resolved_source_id, &state.proxy_key);

    let url_cache_key = format!(
        "url:{}|p:{}|f:{}",
        view.url,
        view.pagination.current_page,
        serde_json::to_string(&CatalogFilterQuery::default()).unwrap_or_default()
    );
    if let Some(key) = cache_key {
        state.catalogs.insert(key, view.clone()).await;
    } else {
        state.catalogs.insert(url_cache_key, view.clone()).await;
    }

    if let Some(next_url) = next_to_prefetch {
        spawn_next_catalog_page_prefetch(Arc::clone(state), next_url, resolved_source_id);
    }

    view
}

async fn rewrite_raw_url_if_mirrored(state: &AppState, raw_url: &str) -> String {
    let Ok(parsed) = normalize_url(raw_url) else {
        return raw_url.to_owned();
    };
    let Some(host) = parsed.host_str() else {
        return raw_url.to_owned();
    };
    let Ok(Some(saved)) = state.db.find_matching_saved_source(None, Some(host)).await else {
        return raw_url.to_owned();
    };
    let Ok(mirror_base) = Url::parse(&saved.base_url) else {
        return raw_url.to_owned();
    };
    if mirror_base
        .host_str()
        .is_some_and(|mh| !mh.eq_ignore_ascii_case(host))
    {
        return rewrite_domain_override(&parsed, &mirror_base).into();
    }
    raw_url.to_owned()
}

async fn resolve_app_deep_link(
    state: &Arc<AppState>,
    raw_url: &str,
    manga_id: Option<i64>,
    source_key: Option<String>,
    target_url: Option<String>,
    title: Option<String>,
    policy: mangacore::HeadlessPolicy,
) -> Result<ResolveResponse, AppError> {
    if let Some(id) = manga_id
        && let Ok(view) = build_manga_view(state, id).await
    {
        let diag = local_diagnostic(raw_url, &view);
        return Ok(ResolveResponse {
            kind: "manga".to_owned(),
            manga: Some(view),
            start_at: None,
            catalog: None,
            diagnostic: diag,
        });
    }

    let Some(url_str) = target_url else {
        return Err(AppError::InvalidUrl {
            message: "deep link missing target url or known local id".to_owned(),
        });
    };
    let parsed = normalize_url(&url_str).map_err(AppError::from)?;
    if let Ok(Some(saved)) = state
        .db
        .find_matching_saved_source(source_key.as_deref(), parsed.host_str())
        .await
        && let Ok(mirror_base) = Url::parse(&saved.base_url)
    {
        let kind = CmsKind::from_storage_str(&saved.source_kind);
        if let Ok(resolved) = resolve_exact_from_source(
            state.registry.http(),
            kind,
            &mirror_base,
            saved.parser_ref.as_deref(),
            saved.search_template.as_deref(),
            &parsed,
            title.as_deref(),
        )
        .await
        {
            let diag = RecognitionDiagnostic {
                status: RecognitionStatus::MangaIdentified,
                url_kind: UrlKind::MangaDetail,
                website: mirror_base.host_str().map(str::to_owned),
                adapter: Some(saved.source_kind),
                submitted_url: raw_url.to_owned(),
                resolved_url: Some(mirror_base.as_str().to_owned()),
                stage: "deep_link_two_phase".to_owned(),
                http_status: None,
                detail: "Resolved via saved source two-phase lookup".to_owned(),
                suggested_action: None,
                confidence: 0.9,
                report: None,
                timestamp_unix: crate::repo::now_unix_secs(),
            };
            return finish_resolved_response(state, Some(parsed), resolved, diag).await;
        }
    }

    let (resolved, diag) = match state
        .registry
        .resolve_with_diagnostics(&url_str, policy)
        .await
    {
        Ok(pair) => pair,
        Err(boxed) => return record_and_return_resolve_error(state, boxed.0, boxed.1).await,
    };
    finish_resolved_response(state, Some(parsed), resolved, diag).await
}

async fn finish_resolved_response(
    state: &Arc<AppState>,
    submitted_url: Option<Url>,
    resolved: Resolved,
    diagnostic: RecognitionDiagnostic,
) -> Result<ResolveResponse, AppError> {
    let settings = state.current_settings().await;
    let deps = AutoRecordDeps {
        db: state.db.clone(),
        http: state.registry.http().clone(),
        images: state.images.clone(),
        max_mb: settings.cache.max_mb,
    };

    match resolved {
        Resolved::Catalog(cat) => {
            if let Some(u) = submitted_url.or_else(|| Some(cat.url.clone())) {
                spawn_auto_record_from_catalog(deps, u, &cat);
            }
            // Note: do NOT bulk-prefetch catalog cover images here; letting `<img loading="lazy">`
            // stream visible covers through the dedicated `img_semaphores` lane keeps navigation fast!
            let view = enrich_and_cache_catalog_view(state, *cat, None, None).await;
            Ok(ResolveResponse {
                kind: "catalog".to_owned(),
                manga: None,
                start_at: None,
                catalog: Some(view),
                diagnostic,
            })
        }
        Resolved::Manga(manga) => {
            let manga_id = state.db.upsert_manga(&manga).await?;
            let site_key = manga.site.as_storage_key();
            let _ = crate::services::layout_detect::classify_manga_on_import(
                state,
                manga_id,
                &manga.genres,
                &manga.tags,
                &site_key,
                &[],
            )
            .await;
            prefetch_manga_cover(state, manga_id, &manga, settings.cache.max_mb);
            if let Some(u) = submitted_url.or_else(|| Some(manga.url.clone())) {
                spawn_auto_record_from_manga(deps, u, &manga);
            }
            let view = build_manga_view(state, manga_id).await?;
            Ok(ResolveResponse {
                kind: "manga".to_owned(),
                manga: Some(view),
                start_at: None,
                catalog: None,
                diagnostic,
            })
        }
        Resolved::Chapter { manga, at } => {
            let manga_id = state.db.upsert_manga(&manga).await?;
            let site_key = manga.site.as_storage_key();
            let _ = crate::services::layout_detect::classify_manga_on_import(
                state,
                manga_id,
                &manga.genres,
                &manga.tags,
                &site_key,
                &[],
            )
            .await;
            prefetch_manga_cover(state, manga_id, &manga, settings.cache.max_mb);
            if let Some(u) = submitted_url.or_else(|| Some(manga.url.clone())) {
                spawn_auto_record_from_manga(deps, u, &manga);
            }
            let view = build_manga_view(state, manga_id).await?;
            let target_norm = at.url.as_str().trim_end_matches('/');
            let matched = view
                .chapters
                .iter()
                .find(|c| c.url.trim_end_matches('/') == target_norm)
                .cloned()
                .unwrap_or_else(|| ChapterRefView {
                    url: at.url.into(),
                    name: at.name,
                    number: at.number.map(f64::from),
                    idx: 0,
                    read: false,
                    volume: at.volume,
                    published_at: at.published_at,
                    language: at.language,
                    source_id: at.source_id,
                });
            Ok(ResolveResponse {
                kind: "chapter".to_owned(),
                manga: Some(view),
                start_at: Some(matched),
                catalog: None,
                diagnostic,
            })
        }
        _ => Err(AppError::Internal {
            message: "unexpected resolve variant".to_owned(),
        }),
    }
}

async fn record_and_return_resolve_error<T>(
    state: &AppState,
    err: ScrapeError,
    diag: RecognitionDiagnostic,
) -> Result<T, AppError> {
    let report_json = diag
        .report
        .as_ref()
        .and_then(|r| serde_json::to_string_pretty(r).ok())
        .unwrap_or_else(|| diag.detail.clone());
    let app_err = match diag.report.clone() {
        Some(report) => AppError::from_scrape_and_report(&err, report),
        None => AppError::from(err),
    };
    let _ = state
        .db
        .record_diagnostic_log(&NewDiagnosticLog {
            level: "error".to_owned(),
            category: app_err.category().to_owned(),
            message: format!("Failed to resolve URL ({:?})", diag.status),
            detail: Some(report_json),
            source: diag.website.clone(),
            chapter_url: Some(diag.submitted_url.clone()),
            failed_url: Some(diag.submitted_url.clone()),
            resolved_url: diag.resolved_url.clone(),
            http_status: diag.http_status.map(i64::from),
            adapter: diag.adapter.clone(),
            parsing_stage: Some(diag.stage.clone()),
            api_error: matches!(app_err, AppError::ApiFailure { .. }).then(|| app_err.to_string()),
            stack_trace: None,
        })
        .await;
    Err(app_err)
}

fn prefetch_manga_cover(state: &AppState, manga_id: i64, manga: &mangacore::Manga, max_mb: u32) {
    if let Some(ref cover) = manga.cover
        && !crate::services::comics::is_local_comic_url(cover.as_str())
    {
        state.images.spawn_cover_prefetch(
            state.registry.http().clone(),
            cover.clone(),
            manga.url.clone(),
            Some(manga.site.as_storage_key()),
            Some(manga_id),
            max_mb,
        );
    }
}

fn local_diagnostic(raw_url: &str, view: &MangaView) -> RecognitionDiagnostic {
    RecognitionDiagnostic {
        status: RecognitionStatus::MangaIdentified,
        url_kind: UrlKind::MangaDetail,
        website: Some(view.site.clone()),
        adapter: Some("local".to_owned()),
        submitted_url: raw_url.to_owned(),
        resolved_url: Some(view.url.clone()),
        stage: "deep_link_local".to_owned(),
        http_status: None,
        detail: "Loaded from local library".to_owned(),
        suggested_action: None,
        confidence: 1.0,
        report: None,
        timestamp_unix: crate::repo::now_unix_secs(),
    }
}

pub(crate) fn map_catalog_view(
    cat: CatalogPage,
    source_id: Option<i64>,
    proxy_key: &[u8; 32],
) -> CatalogPageView {
    let site_key = cat.site.as_storage_key();
    let referer = cat.url.as_str();
    let entries =
        cat.entries
            .into_iter()
            .map(|entry| {
                let cover_token = entry.cover.as_ref().map(|c| {
                    encode_cover_token(c.as_str(), referer, None, Some(&site_key), proxy_key)
                });
                CatalogEntryView {
                    url: entry.url.into(),
                    title: entry.title,
                    alt_titles: entry.alt_titles,
                    cover_token,
                    latest_chapter: entry.latest_chapter,
                    status: entry.status,
                    genres: entry.genres,
                    source_id: entry.source_id,
                    chapter_count: entry.chapter_count,
                    updated_at: entry.updated_at,
                }
            })
            .collect();

    let categories = cat
        .categories
        .into_iter()
        .map(|nav| NavLinkView {
            label: nav.label,
            url: nav.url.into(),
            kind: nav.kind,
        })
        .collect();

    CatalogPageView {
        url: cat.url.into(),
        site: site_key,
        site_name: cat.site_name,
        title: cat.title,
        url_kind: cat.url_kind,
        query: cat.query,
        search_url_template: cat.search_url_template,
        entries,
        categories,
        pagination: PaginationView {
            current_page: cat.pagination.current_page,
            total_pages: cat.pagination.total_pages,
            prev_url: cat.pagination.prev_url.map(Into::into),
            next_url: cat.pagination.next_url.map(Into::into),
            page_url_template: cat.pagination.page_url_template,
        },
        filter_schema: cat.filter_schema,
        source_id,
    }
}

/// Loads a stored manga by ID, optionally refreshing its chapter list from the web first.
/// Local `.cbz` archives return stored chapters directly without remote HTTP scraping.
///
/// # Errors
///
/// Returns [`AppError::NotFound`] if the manga ID does not exist, or scraper/DB errors.
pub(crate) async fn get_manga(
    state: &AppState,
    manga_id: i64,
    mode: FetchMode,
) -> Result<MangaView, AppError> {
    let row = state
        .db
        .get_manga_by_id(manga_id)
        .await?
        .ok_or_else(|| AppError::NotFound {
            message: format!("manga {manga_id} not found"),
        })?;

    if mode == FetchMode::RefreshFromWeb && !crate::services::comics::is_local_site(&row.site) {
        let policy = state.current_settings().await.headless_policy();
        let (resolved, _diag) = match state
            .registry
            .resolve_with_diagnostics(&row.url, policy)
            .await
        {
            Ok(pair) => pair,
            Err(boxed) => {
                let (err, diag) = *boxed;
                let report_json = diag
                    .report
                    .as_ref()
                    .and_then(|r| serde_json::to_string_pretty(r).ok())
                    .unwrap_or_else(|| diag.detail.clone());
                let app_err = match diag.report.clone() {
                    Some(report) => AppError::from_scrape_and_report(&err, report),
                    None => AppError::from(err),
                };
                let _ = state
                    .db
                    .record_diagnostic_log(&NewDiagnosticLog {
                        level: "error".to_owned(),
                        category: app_err.category().to_owned(),
                        message: format!("Failed to refresh chapter list for '{}'", row.title),
                        detail: Some(report_json),
                        source: Some(row.site.clone()),
                        chapter_url: Some(diag.submitted_url.clone()),
                        failed_url: Some(diag.submitted_url.clone()),
                        resolved_url: diag.resolved_url.clone(),
                        http_status: diag.http_status.map(i64::from),
                        adapter: diag.adapter.clone(),
                        parsing_stage: Some(diag.stage.clone()),
                        ..NewDiagnosticLog::default()
                    })
                    .await;
                return Err(app_err);
            }
        };
        if let Some(refreshed_manga) = resolved.as_manga() {
            let max_mb = state.current_settings().await.cache.max_mb;
            let _ = state
                .db
                .upsert_manga_with_id(Some(manga_id), refreshed_manga)
                .await?;
            let site_key = refreshed_manga.site.as_storage_key();
            let _ = crate::services::layout_detect::classify_manga_on_import(
                state,
                manga_id,
                &refreshed_manga.genres,
                &refreshed_manga.tags,
                &site_key,
                &[],
            )
            .await;
            prefetch_manga_cover(state, manga_id, refreshed_manga, max_mb);
            state.db.clear_has_new(manga_id).await?;
        }
    }

    build_manga_view(state, manga_id).await
}

/// Launches a visible browser window so the user can manually complete a site verification check.
///
/// # Errors
///
/// Returns [`AppError`] if the URL is invalid or the browser fails to launch/complete.
pub(crate) async fn open_manual_verification(
    state: &AppState,
    raw_url: &str,
) -> Result<(), AppError> {
    let url = normalize_url(raw_url)?;
    state.registry.open_manual_verification(&url).await?;
    Ok(())
}

/// Persists the user's current reading position into `history`.
///
/// # Errors
///
/// Returns [`AppError::Db`] on SQLite failure.
pub(crate) async fn save_progress(
    state: &AppState,
    manga_id: i64,
    chapter_url: &str,
    chapter_name: &str,
    scroll_ratio: f64,
    page_index: i64,
) -> Result<(), AppError> {
    state
        .db
        .save_history(
            manga_id,
            chapter_url,
            chapter_name,
            scroll_ratio,
            page_index,
        )
        .await
}

/// Returns the persisted last reading destination used for History double-tap resume.
///
/// # Errors
///
/// Returns [`AppError::Db`] on SQLite failure.
pub(crate) async fn get_last_reading_destination(
    state: &AppState,
) -> Result<Option<LastReadingDestination>, AppError> {
    state.db.get_last_reading_destination().await
}

/// Toggles bookmark status for a manga and returns the updated state (`true` = bookmarked).
///
/// # Errors
///
/// Returns [`AppError::Db`] on SQLite failure.
pub(crate) async fn toggle_bookmark(state: &AppState, manga_id: i64) -> Result<bool, AppError> {
    state.db.toggle_bookmark(manga_id).await
}

/// Lists all bookmarked manga with signed cover proxy tokens.
///
/// # Errors
///
/// Returns [`AppError::Db`] on SQLite failure.
pub(crate) async fn list_bookmarks(
    state: &AppState,
    sort: BookmarkSort,
) -> Result<Vec<BookmarkItemView>, AppError> {
    let rows = state.db.list_bookmarks(sort).await?;
    Ok(rows
        .into_iter()
        .map(|row| map_bookmark_row(row, &state.proxy_key))
        .collect())
}

/// Sets or clears the per-manga manual reader mode override.
///
/// # Errors
///
/// Returns [`AppError::Db`] on SQLite failure.
pub(crate) async fn set_manga_reader_override(
    state: &AppState,
    manga_id: i64,
    override_mode: Option<&str>,
) -> Result<(), AppError> {
    state
        .db
        .set_manga_reader_override(manga_id, override_mode)
        .await
}

/// Persists or clears a manga's full per-title [`ReaderSettings`] configuration.
///
/// # Errors
///
/// Returns [`AppError::Db`] or [`AppError::Internal`] on serialization/SQLite failure.
pub(crate) async fn set_manga_reader_settings(
    state: &AppState,
    manga_id: i64,
    reader_settings: Option<ReaderSettings>,
) -> Result<Option<ReaderSettings>, AppError> {
    let clean = reader_settings.map(ReaderSettings::sanitized);
    let json = match clean.as_ref() {
        Some(rs) => Some(serde_json::to_string(rs).map_err(|e| AppError::Internal {
            message: format!("failed to serialize per-manga reader settings: {e}"),
        })?),
        None => None,
    };
    state
        .db
        .set_manga_reader_settings(manga_id, json.as_deref())
        .await?;
    Ok(clean)
}

/// Records that passive Webtoon / Long-Strip detection matched this manga.
///
/// # Errors
///
/// Returns [`AppError::Db`] on SQLite failure.
pub(crate) async fn mark_manga_detected_webtoon(
    state: &AppState,
    manga_id: i64,
) -> Result<(), AppError> {
    state.db.mark_manga_detected_webtoon(manga_id).await
}

/// Hides or unhides a manga from normal library views.
///
/// # Errors
///
/// Returns [`AppError::Db`] on SQLite failure.
pub(crate) async fn set_manga_hidden(
    state: &AppState,
    manga_id: i64,
    action: HideAction,
) -> Result<bool, AppError> {
    state.db.set_manga_hidden(manga_id, action).await
}

/// Lists all hidden manga so they can be unhidden or deleted.
///
/// # Errors
///
/// Returns [`AppError::Db`] on SQLite failure.
pub(crate) async fn list_hidden_manga(state: &AppState) -> Result<Vec<HiddenMangaView>, AppError> {
    let rows = state.db.list_hidden_manga().await?;
    Ok(rows
        .into_iter()
        .map(|row| {
            let cover_token = row.cover_url.as_deref().map(|c| {
                encode_cover_token(c, &row.url, Some(row.id), Some(&row.site), &state.proxy_key)
            });
            HiddenMangaView {
                id: row.id,
                url: row.url,
                site: row.site,
                title: row.title,
                cover_token,
                latest_chapter_name: row.latest_chapter_name,
            }
        })
        .collect())
}

/// Permanently deletes a manga, its internal `.cbz` archives (if any), chapters, history,
/// bookmarks, notifications, and cached images.
///
/// # Errors
///
/// Returns [`AppError::Db`] on SQLite failure.
pub(crate) async fn delete_manga_permanently(
    state: &AppState,
    manga_id: i64,
) -> Result<(), AppError> {
    let cover_url = state
        .db
        .get_manga_by_id(manga_id)
        .await?
        .and_then(|m| m.cover_url);
    crate::services::comics::delete_local_files_for_manga(&state.data_dir, state, manga_id).await;
    state.db.delete_manga_permanently(manga_id).await?;
    let _ = state.images.clear_by_manga(manga_id).await;
    if let Some(ref url) = cover_url {
        let _ = state.images.remove_cover_if_unreferenced(url).await;
    }
    state.chapters.invalidate_all();
    Ok(())
}

/// Lists stored chapter notifications for the Notification Center.
///
/// # Errors
///
/// Returns [`AppError::Db`] on SQLite failure.
pub(crate) async fn list_notifications(
    state: &AppState,
) -> Result<Vec<NotificationItemView>, AppError> {
    let rows = state.db.list_notifications().await?;
    Ok(rows
        .into_iter()
        .map(|row| map_notification_row(row, &state.proxy_key))
        .collect())
}

/// Marks a single notification as read.
///
/// # Errors
///
/// Returns [`AppError::Db`] on SQLite failure.
pub(crate) async fn mark_notification_read(state: &AppState, id: i64) -> Result<(), AppError> {
    state.db.mark_notification_read(id).await
}

/// Marks all notifications as read.
///
/// # Errors
///
/// Returns [`AppError::Db`] on SQLite failure.
pub(crate) async fn mark_all_notifications_read(state: &AppState) -> Result<(), AppError> {
    state.db.mark_all_notifications_read().await
}

/// Deletes a single notification by ID.
///
/// # Errors
///
/// Returns [`AppError::Db`] on SQLite failure.
pub(crate) async fn delete_notification(state: &AppState, id: i64) -> Result<(), AppError> {
    state.db.delete_notification(id).await
}

/// Clears all notifications from the Notification Center.
///
/// # Errors
///
/// Returns [`AppError::Db`] on SQLite failure.
pub(crate) async fn clear_notifications(state: &AppState) -> Result<(), AppError> {
    state.db.clear_notifications().await
}

/// Marks a chapter as read in SQLite.
///
/// # Errors
///
/// Returns [`AppError::Db`] on SQLite failure.
pub(crate) async fn mark_read(
    state: &AppState,
    manga_id: i64,
    chapter_url: &str,
) -> Result<(), AppError> {
    state.db.mark_chapter_read(manga_id, chapter_url).await
}

/// Lists all reading history entries with signed cover proxy tokens.
///
/// # Errors
///
/// Returns [`AppError::Db`] on SQLite failure.
pub(crate) async fn list_history(state: &AppState) -> Result<Vec<HistoryItemView>, AppError> {
    let rows = state.db.list_history().await?;
    Ok(rows
        .into_iter()
        .map(|row| map_history_row(row, &state.proxy_key))
        .collect())
}

/// Removes one manga from reading history.
///
/// # Errors
///
/// Returns [`AppError::Db`] on SQLite failure.
pub(crate) async fn remove_history(state: &AppState, manga_id: i64) -> Result<(), AppError> {
    state.db.remove_history(manga_id).await
}

/// Clears the entire reading history.
///
/// # Errors
///
/// Returns [`AppError::Db`] on SQLite failure.
pub(crate) async fn clear_history(state: &AppState) -> Result<(), AppError> {
    state.db.clear_history().await
}

async fn build_manga_view(state: &AppState, manga_id: i64) -> Result<MangaView, AppError> {
    let row = state
        .db
        .get_manga_by_id(manga_id)
        .await?
        .ok_or_else(|| AppError::NotFound {
            message: format!("manga {manga_id} not found"),
        })?;
    let chapters = state.db.list_chapters(manga_id).await?;
    let progress = state.db.get_history_for_manga(manga_id).await?;
    let bookmarked = state.db.is_bookmarked(manga_id).await?;
    let (unread_count, is_completed) = state.db.get_manga_read_summary(manga_id).await?;

    Ok(map_manga_view(
        row,
        chapters,
        progress,
        bookmarked,
        unread_count,
        is_completed,
        &state.proxy_key,
    ))
}

fn parse_json_vec(raw: Option<String>) -> Vec<String> {
    raw.and_then(|s| serde_json::from_str::<Vec<String>>(&s).ok())
        .unwrap_or_default()
}

#[expect(
    clippy::fn_params_excessive_bools,
    reason = "internal view mapper receiving boolean display flags"
)]
fn map_manga_view(
    row: MangaRow,
    chapters: Vec<ChapterRow>,
    progress: Option<HistoryProgressRow>,
    bookmarked: bool,
    unread_count: i64,
    is_completed: bool,
    proxy_key: &[u8; 32],
) -> MangaView {
    let cover_token = row
        .cover_url
        .as_deref()
        .map(|c| encode_cover_token(c, &row.url, Some(row.id), Some(&row.site), proxy_key));

    let reader_settings = row
        .reader_settings_json
        .as_deref()
        .and_then(|raw| serde_json::from_str::<ReaderSettings>(raw).ok())
        .map(ReaderSettings::sanitized);

    let detected_layout = row
        .detected_layout
        .filter(|s| matches!(s.as_str(), "manga" | "webtoon"))
        .or_else(|| (row.detected_webtoon != 0).then(|| "webtoon".to_owned()));
    let detected_webtoon =
        detected_layout.as_deref() == Some("webtoon") || row.detected_webtoon != 0;

    MangaView {
        id: row.id,
        url: row.url,
        site: row.site,
        title: row.title,
        cover_token,
        has_new: row.has_new != 0,
        bookmarked,
        hidden: row.hidden != 0,
        unread_count,
        is_completed,
        reader_mode_override: row.reader_mode_override,
        reader_settings,
        detected_webtoon,
        detected_layout,
        latest_chapter_name: row.latest_chapter_name,
        source_id: row.source_id,
        canonical_url: row.canonical_url,
        alt_titles: parse_json_vec(row.alt_titles_json),
        original_title: row.original_title,
        authors: parse_json_vec(row.authors_json),
        artists: parse_json_vec(row.artists_json),
        description: row.description,
        genres: parse_json_vec(row.genres_json),
        tags: parse_json_vec(row.tags_json),
        status: row.status,
        language: row.language,
        available_languages: parse_json_vec(row.available_languages_json),
        chapters: chapters
            .into_iter()
            .map(|c| ChapterRefView {
                url: c.url,
                name: c.name,
                number: c.number,
                idx: c.idx,
                read: c.read != 0,
                volume: c.volume,
                published_at: c.published_at,
                language: c.language,
                source_id: c.source_id,
            })
            .collect(),
        progress: progress.map(|p| ProgressView {
            chapter_url: p.chapter_url,
            chapter_name: p.chapter_name,
            scroll_ratio: p.scroll_ratio,
            page_index: p.page_index,
            read_at: p.read_at,
        }),
    }
}

fn map_history_row(row: HistoryEntryRow, proxy_key: &[u8; 32]) -> HistoryItemView {
    let cover_token = row.cover_url.as_deref().map(|c| {
        encode_cover_token(
            c,
            &row.manga_url,
            Some(row.manga_id),
            Some(&row.site),
            proxy_key,
        )
    });

    HistoryItemView {
        manga_id: row.manga_id,
        manga_url: row.manga_url,
        site: row.site,
        title: row.title,
        cover_token,
        chapter_url: row.chapter_url,
        chapter_name: row.chapter_name,
        scroll_ratio: row.scroll_ratio,
        page_index: row.page_index,
        read_at: row.read_at,
        added_at: row.added_at,
        latest_chapter_url: row.latest_chapter_url,
        latest_chapter_name: row.latest_chapter_name,
        has_new: row.has_new != 0,
        bookmarked: row.bookmarked != 0,
        total_chapters: row.total_chapters,
        unread_count: row.unread_count,
        is_completed: row.is_completed != 0,
        current_chapter_idx: row.current_chapter_idx,
    }
}

fn map_bookmark_row(row: BookmarkEntryRow, proxy_key: &[u8; 32]) -> BookmarkItemView {
    let cover_token = row.cover_url.as_deref().map(|c| {
        encode_cover_token(
            c,
            &row.manga_url,
            Some(row.manga_id),
            Some(&row.site),
            proxy_key,
        )
    });

    BookmarkItemView {
        manga_id: row.manga_id,
        manga_url: row.manga_url,
        site: row.site,
        title: row.title,
        cover_token,
        bookmarked_at: row.bookmarked_at,
        latest_chapter_url: row.latest_chapter_url,
        latest_chapter_name: row.latest_chapter_name,
        has_new: row.has_new != 0,
        total_chapters: row.total_chapters,
        unread_count: row.unread_count,
        is_completed: row.is_completed != 0,
        chapter_url: row.chapter_url,
        chapter_name: row.chapter_name,
        scroll_ratio: row.scroll_ratio,
        page_index: row.page_index,
        read_at: row.read_at,
        current_chapter_idx: row.current_chapter_idx,
        reader_mode_override: row.reader_mode_override,
        detected_webtoon: row.detected_webtoon != 0,
    }
}

fn map_notification_row(row: NotificationRow, proxy_key: &[u8; 32]) -> NotificationItemView {
    let cover_token = row
        .cover_url
        .as_deref()
        .map(|c| encode_cover_token(c, &row.manga_url, Some(row.manga_id), None, proxy_key));

    NotificationItemView {
        id: row.id,
        manga_id: row.manga_id,
        manga_title: row.manga_title,
        cover_token,
        chapter_range: row.chapter_range,
        latest_chapter_url: row.latest_chapter_url,
        chapter_count: row.chapter_count,
        read: row.read != 0,
        created_at: row.created_at,
    }
}
