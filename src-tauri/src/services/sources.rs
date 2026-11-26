//! Saved source onboarding, catalog browsing, dynamic per-website filter discovery,
//! parallel multi-source search, and supplemental visibility isolation.

use std::sync::Arc;
use std::time::Duration;

use mangacore::{
    CatalogFilterQuery, CatalogPage, CmsKind, HttpClient, Manga, Resolved, SiteFilterSchema,
    browse_or_filter_cms, browse_or_search_cms, default_filter_schema_for_adapter,
    detect_site_structure, match_builtin_adapter_id, merge_filter_schemas, normalize_source_root,
    normalize_url, parse::meta::detect_adult_signals,
};
use serde::Serialize;
use tokio::sync::Semaphore;
use tokio::task::JoinSet;
use url::Url;

use crate::cache::ImageCache;
use crate::cache::image::CacheEntryMeta;
use crate::error::AppError;
use crate::image_proxy::encode_cover_token;
use crate::repo::Database;
use crate::repo::sources::{
    SavedSourceRow, SourcePlacement, UpdateSavedSource, UpsertSavedSource, VisibilityFilter,
    VisibilityPatch,
};
use crate::services::library::{
    CatalogEntryView, CatalogPageView, enrich_and_cache_catalog_view, map_catalog_view,
};
use crate::state::AppState;

/// Source item rendered in the UI Sources list or Settings -> Supplemental Sources.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SavedSourceView {
    pub id: i64,
    pub source_key: String,
    pub display_name: String,
    pub base_url: String,
    pub source_kind: String,
    pub parser_ref: Option<String>,
    pub search_template: Option<String>,
    pub icon_token: Option<String>,
    pub is_enabled: bool,
    pub is_supplemental: bool,
    pub manual_visibility: bool,
    pub created_at: i64,
    pub updated_at: i64,
    pub filter_schema: SiteFilterSchema,
}

/// Per-source search results group in a multi-source parallel keyword search.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SourceSearchGroupView {
    pub source: SavedSourceView,
    pub entries: Vec<CatalogEntryView>,
    pub error: Option<String>,
}

/// Aggregated response for parallel keyword search across enabled saved sources.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MultiSourceSearchResponse {
    pub query: String,
    pub groups: Vec<SourceSearchGroupView>,
}

/// Detached runtime dependencies for zero-latency background source recording.
#[derive(Debug, Clone)]
pub(crate) struct AutoRecordDeps {
    pub db: Database,
    pub http: HttpClient,
    pub images: ImageCache,
    pub max_mb: u32,
}

/// Returns built-in baseline [`SiteFilterSchema`] for known first-party adapters.
pub(crate) fn default_schema_for_host_or_parser(
    host: Option<&str>,
    parser_ref: Option<&str>,
) -> SiteFilterSchema {
    default_filter_schema_for_adapter(parser_ref, host)
}

/// Lists saved sources filtered by visibility bucket (`main_only`, `supplemental_only`, `all`).
///
/// # Errors
///
/// Returns [`AppError::Db`] on SQLite failure.
pub(crate) async fn list_saved_sources(
    state: &AppState,
    filter: VisibilityFilter,
) -> Result<Vec<SavedSourceView>, AppError> {
    let rows = state.db.list_saved_sources(filter).await?;
    Ok(rows
        .into_iter()
        .map(|row| map_saved_source_view(row, &state.proxy_key))
        .collect())
}

/// Probes `raw_url` using structure-first CMS classification + dynamic filter discovery + favicon caching,
/// and saves the source into SQLite.
///
/// # Errors
///
/// Returns [`AppError`] on invalid/disallowed URL or SQLite failure.
pub(crate) async fn add_saved_source(
    state: &AppState,
    raw_url: &str,
    custom_name: Option<&str>,
    placement: SourcePlacement,
) -> Result<SavedSourceView, AppError> {
    let http = state.registry.http();
    let outcome = detect_site_structure(http, raw_url)
        .await
        .map_err(AppError::from)?;
    let max_mb = state.current_settings().await.cache.max_mb;

    let chosen_icon = resolve_and_cache_icon(
        http,
        &state.images,
        &outcome.icon_candidates,
        &outcome.base_url,
        max_mb,
    )
    .await;

    if let Some(h) = outcome.base_url.host_str() {
        let _ = state
            .db
            .merge_host_filter_schema(h, &outcome.filter_schema)
            .await;
    }

    let display_name = custom_name
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(outcome.display_name.as_str());
    let source_key = build_source_key(
        outcome.kind,
        outcome.parser_ref.as_deref(),
        &outcome.base_url,
    );
    let saved = state
        .db
        .upsert_discovered_source(&UpsertSavedSource {
            source_key: &source_key,
            display_name,
            base_url: outcome.base_url.as_str(),
            source_kind: outcome.kind.as_storage_str(),
            parser_ref: outcome.parser_ref.as_deref(),
            search_template: outcome.search_template.as_deref(),
            icon_url: chosen_icon.as_deref(),
            detected_adult: outcome.adult_detected,
            placement,
            prefer_new_name: custom_name.is_some_and(|s| !s.trim().is_empty()),
            filter_schema: Some(&outcome.filter_schema),
        })
        .await?;

    Ok(map_saved_source_view(saved, &state.proxy_key))
}

/// Updates user-editable source metadata (name, mirror `base_url`, enabled flag, or Main/Supplemental placement).
///
/// # Errors
///
/// Returns [`AppError`] on invalid mirror URL, missing source, or SQLite failure.
pub(crate) async fn update_saved_source(
    state: &AppState,
    id: i64,
    display_name: Option<&str>,
    base_url: Option<&str>,
    is_enabled: Option<bool>,
    visibility: VisibilityPatch,
) -> Result<SavedSourceView, AppError> {
    let row = state
        .db
        .update_saved_source(
            id,
            &UpdateSavedSource {
                display_name,
                base_url,
                is_enabled,
                visibility,
            },
        )
        .await?;
    state.catalogs.invalidate_all();
    Ok(map_saved_source_view(row, &state.proxy_key))
}

/// Deletes a saved source by ID.
///
/// # Errors
///
/// Returns [`AppError::Db`] on SQLite failure.
pub(crate) async fn delete_saved_source(state: &AppState, id: i64) -> Result<(), AppError> {
    state.catalogs.invalidate_all();
    state.db.delete_saved_source(id).await
}

/// Returns (and lazily discovers if not yet cached) the dynamic per-website [`SiteFilterSchema`]
/// for either a saved `source_id` or an arbitrary pasted `raw_url` so the multi-tag filter bar
/// is ready as soon as the user selects a source or types a website link.
///
/// # Errors
///
/// Returns [`AppError`] on invalid URL or network failure when probing an unseen website.
pub(crate) async fn get_web_filter_schema(
    state: &AppState,
    source_id: Option<i64>,
    raw_url: Option<&str>,
) -> Result<SiteFilterSchema, AppError> {
    if let Some(sid) = source_id {
        let row =
            state
                .db
                .get_saved_source_by_id(sid)
                .await?
                .ok_or_else(|| AppError::NotFound {
                    message: format!("saved source {sid} not found"),
                })?;
        let view = map_saved_source_view(row.clone(), &state.proxy_key);
        if !view.filter_schema.tags.is_empty() {
            return Ok(view.filter_schema);
        }
        if let Ok(outcome) = detect_site_structure(state.registry.http(), &row.base_url).await {
            let merged = state
                .db
                .merge_saved_source_filter_schema(sid, &outcome.filter_schema)
                .await?;
            return Ok(merge_filter_schemas(Some(&view.filter_schema), merged));
        }
        return Ok(view.filter_schema);
    }

    let Some(url_str) = raw_url.map(str::trim).filter(|s| !s.is_empty()) else {
        return Ok(SiteFilterSchema::default());
    };
    let parsed = normalize_url(url_str).map_err(AppError::from)?;
    let host = parsed.host_str().unwrap_or("");
    let mut schema = default_schema_for_host_or_parser(Some(host), None);

    if let Ok(Some(saved)) = state.db.find_matching_saved_source(None, Some(host)).await
        && let Some(ref json) = saved.filter_schema_json
        && let Ok(s) = serde_json::from_str::<SiteFilterSchema>(json)
    {
        schema = merge_filter_schemas(Some(&schema), s);
    }
    if let Ok(Some(cached)) = state.db.get_host_filter_schema(host).await {
        schema = merge_filter_schemas(Some(&schema), cached);
    }
    if !schema.tags.is_empty() {
        return Ok(schema);
    }

    if let Ok(outcome) = detect_site_structure(state.registry.http(), parsed.as_str()).await {
        let merged = state
            .db
            .merge_host_filter_schema(host, &outcome.filter_schema)
            .await?;
        return Ok(merge_filter_schemas(Some(&schema), merged));
    }

    Ok(schema)
}

/// Browses, searches, or applies multi-tag/chapter/recency filters against a single saved source,
/// backed by RAM `CatalogCache` and next-page prefetching.
///
/// # Errors
///
/// Returns [`AppError`] when the source does not exist or upstream resolution fails.
#[expect(
    clippy::fn_params_excessive_bools,
    reason = "force_refresh flag bypasses RAM CatalogCache on explicit user refresh"
)]
pub(crate) async fn browse_saved_source(
    state: &Arc<AppState>,
    source_id: i64,
    query: Option<&str>,
    filters: Option<CatalogFilterQuery>,
    page: u32,
    force_refresh: bool,
) -> Result<CatalogPageView, AppError> {
    let row = state
        .db
        .get_saved_source_by_id(source_id)
        .await?
        .ok_or_else(|| AppError::NotFound {
            message: format!("saved source {source_id} not found"),
        })?;
    let base_url = Url::parse(&row.base_url).map_err(|e| AppError::InvalidUrl {
        message: e.to_string(),
    })?;
    let kind = CmsKind::from_storage_str(&row.source_kind);
    let target_page = page.max(1);

    let mut effective_filters = filters.unwrap_or_default();
    if let Some(q) = query.map(str::trim).filter(|s| !s.is_empty()) {
        effective_filters.query = Some(q.to_owned());
    }

    let filter_sig = serde_json::to_string(&effective_filters).unwrap_or_default();
    let cache_key = format!("src:{source_id}|p:{target_page}|f:{filter_sig}");
    if !force_refresh && let Some(cached) = state.catalogs.get(&cache_key).await {
        return Ok(cached);
    }

    let known_schema = build_saved_source_schema(&row);
    let mut cat = browse_or_filter_cms(
        state.registry.http(),
        kind,
        &base_url,
        None,
        row.parser_ref.as_deref(),
        row.search_template.as_deref(),
        Some(&known_schema),
        &effective_filters,
        target_page,
    )
    .await
    .map_err(AppError::from)?;

    if cat.site_name.trim().is_empty() || cat.site_name == cat.site.as_storage_key() {
        cat.site_name.clone_from(&row.display_name);
    }

    Ok(enrich_and_cache_catalog_view(state, cat, Some(source_id), Some(cache_key)).await)
}

/// Spawns a low-priority background prefetch of `next_url` into `state.catalogs` so clicking
/// "Next Page" renders instantaneously without competing with visible cover image streams.
pub(crate) fn spawn_next_catalog_page_prefetch(
    state: Arc<AppState>,
    next_url: Url,
    source_id: Option<i64>,
) {
    let empty_sig = serde_json::to_string(&CatalogFilterQuery::default()).unwrap_or_default();
    let expected_page = mangacore::extract_page_number_from_url(&next_url).unwrap_or(2);
    let url_key = format!("url:{}|p:{expected_page}|f:{empty_sig}", next_url.as_str());

    tokio::spawn(async move {
        // Yield briefly so the active page's IPC response and visible `<img>` requests go first.
        tokio::time::sleep(Duration::from_millis(320)).await;
        if state.catalogs.get(&url_key).await.is_some() {
            return;
        }
        let policy = state.current_settings().await.headless_policy();
        if let Ok((Resolved::Catalog(cat), _)) = state
            .registry
            .resolve_with_diagnostics(next_url.as_str(), policy)
            .await
        {
            let host = cat.url.host_str().unwrap_or("").to_owned();
            let mut merged_cat = *cat;
            let base_schema = default_schema_for_host_or_parser(Some(&host), None);
            merged_cat.filter_schema =
                merge_filter_schemas(Some(&base_schema), merged_cat.filter_schema);
            if let Ok(Some(cached_host)) = state.db.get_host_filter_schema(&host).await {
                merged_cat.filter_schema =
                    merge_filter_schemas(Some(&cached_host), merged_cat.filter_schema);
            }
            let actual_page = merged_cat.pagination.current_page;
            let actual_key = format!(
                "url:{}|p:{actual_page}|f:{empty_sig}",
                merged_cat.url.as_str()
            );
            let view = map_catalog_view(merged_cat, source_id, &state.proxy_key);
            state.catalogs.insert(actual_key, view.clone()).await;
            if actual_key_differs(&url_key, &view.url, actual_page, &empty_sig) {
                state.catalogs.insert(url_key, view).await;
            }
        }
    });
}

fn actual_key_differs(orig_key: &str, final_url: &str, page: u32, sig: &str) -> bool {
    let candidate = format!("url:{final_url}|p:{page}|f:{sig}");
    orig_key != candidate
}

/// Searches in parallel (capped at 4 concurrent requests, 12s timeout each) across all enabled
/// saved sources inside the requested visibility bucket.
///
/// # Errors
///
/// Returns [`AppError::InvalidUrl`] if `query` is empty or [`AppError::Db`] on SQLite failure.
pub(crate) async fn search_across_sources(
    state: &Arc<AppState>,
    query: &str,
    filter: VisibilityFilter,
) -> Result<MultiSourceSearchResponse, AppError> {
    let clean_q = query.trim();
    if clean_q.is_empty() {
        return Err(AppError::InvalidUrl {
            message: "search keyword must not be empty".to_owned(),
        });
    }

    let sources: Vec<SavedSourceRow> = state
        .db
        .list_saved_sources(filter)
        .await?
        .into_iter()
        .filter(|s| s.is_enabled != 0)
        .collect();

    if sources.is_empty() {
        return Ok(MultiSourceSearchResponse {
            query: clean_q.to_owned(),
            groups: Vec::new(),
        });
    }

    let sem = Arc::new(Semaphore::new(4));
    let mut set = JoinSet::new();

    for (idx, row) in sources.into_iter().enumerate() {
        let permit_sem = Arc::clone(&sem);
        let state_ref = Arc::clone(state);
        let q_owned = clean_q.to_owned();
        set.spawn(async move {
            let Ok(_permit) = permit_sem.acquire().await else {
                return (
                    idx,
                    SourceSearchGroupView {
                        source: map_saved_source_view(row, &state_ref.proxy_key),
                        entries: Vec::new(),
                        error: Some("cancelled".to_owned()),
                    },
                );
            };
            let group = search_single_source(&state_ref, row, &q_owned).await;
            (idx, group)
        });
    }

    let mut ordered: Vec<(usize, SourceSearchGroupView)> = Vec::new();
    while let Some(res) = set.join_next().await {
        if let Ok(item) = res {
            ordered.push(item);
        }
    }
    ordered.sort_by(|a, b| {
        let a_has = !a.1.entries.is_empty();
        let b_has = !b.1.entries.is_empty();
        b_has.cmp(&a_has).then_with(|| a.0.cmp(&b.0))
    });

    Ok(MultiSourceSearchResponse {
        query: clean_q.to_owned(),
        groups: ordered.into_iter().map(|(_, g)| g).collect(),
    })
}

async fn search_single_source(
    state: &AppState,
    row: SavedSourceRow,
    query: &str,
) -> SourceSearchGroupView {
    let source_id = row.id;
    let view = map_saved_source_view(row.clone(), &state.proxy_key);
    let Ok(base_url) = Url::parse(&row.base_url) else {
        return SourceSearchGroupView {
            source: view,
            entries: Vec::new(),
            error: Some("invalid base url".to_owned()),
        };
    };
    let kind = CmsKind::from_storage_str(&row.source_kind);
    let fut = browse_or_search_cms(
        state.registry.http(),
        kind,
        &base_url,
        row.parser_ref.as_deref(),
        row.search_template.as_deref(),
        Some(query),
        1,
    );

    match tokio::time::timeout(Duration::from_secs(12), fut).await {
        Ok(Ok(cat)) => {
            let mapped = map_catalog_view(cat, Some(source_id), &state.proxy_key);
            SourceSearchGroupView {
                source: view,
                entries: mapped.entries,
                error: None,
            }
        }
        Ok(Err(err)) => SourceSearchGroupView {
            source: view,
            entries: Vec::new(),
            error: Some(err.to_string()),
        },
        Err(_) => SourceSearchGroupView {
            source: view,
            entries: Vec::new(),
            error: Some("timeout (12s)".to_owned()),
        },
    }
}

struct AutoSourceRecord {
    url: Url,
    site_label: String,
    source_key_hint: String,
    search_tmpl: Option<String>,
    adult_hint: bool,
    initial_schema: Option<SiteFilterSchema>,
}

/// Spawns non-blocking background source recording when a manga is resolved.
pub(crate) fn spawn_auto_record_from_manga(deps: AutoRecordDeps, url: Url, manga: &Manga) {
    if crate::services::comics::is_local_comic_url(url.as_str())
        || crate::services::comics::is_local_site(&manga.site.as_storage_key())
    {
        return;
    }
    let record = AutoSourceRecord {
        adult_hint: detect_adult_signals(&url, "", &manga.genres, &manga.tags),
        site_label: infer_site_label(&url, manga.site.as_storage_key().as_str()),
        source_key_hint: manga.site.as_storage_key(),
        search_tmpl: None,
        url,
        initial_schema: None,
    };
    tokio::spawn(async move {
        auto_record_source_task(deps, record).await;
    });
}

/// Spawns non-blocking background source recording when a catalog/homepage URL is resolved.
pub(crate) fn spawn_auto_record_from_catalog(deps: AutoRecordDeps, url: Url, cat: &CatalogPage) {
    let mut all_genres: Vec<String> = Vec::new();
    for entry in cat.entries.iter().take(12) {
        all_genres.extend(entry.genres.iter().cloned());
    }
    let adult_hint = detect_adult_signals(&url, "", &all_genres, &[]);
    let site_label = if cat.site_name.trim().is_empty() {
        infer_site_label(&url, cat.site.as_storage_key().as_str())
    } else {
        cat.site_name.clone()
    };
    let record = AutoSourceRecord {
        url,
        site_label,
        source_key_hint: cat.site.as_storage_key(),
        search_tmpl: cat.search_url_template.clone(),
        adult_hint,
        initial_schema: Some(cat.filter_schema.clone()),
    };
    tokio::spawn(async move {
        auto_record_source_task(deps, record).await;
    });
}

async fn auto_record_source_task(deps: AutoRecordDeps, rec: AutoSourceRecord) {
    let Ok(root) = normalize_source_root(rec.url.as_str()) else {
        return;
    };
    let outcome = detect_site_structure(&deps.http, root.as_str()).await.ok();

    let (
        kind,
        parser_ref,
        detected_tmpl,
        icon_candidates,
        detected_adult,
        detected_name,
        detected_schema,
    ) = match outcome {
        Some(o) => (
            o.kind,
            o.parser_ref,
            o.search_template,
            o.icon_candidates,
            o.adult_detected || rec.adult_hint,
            o.display_name,
            Some(o.filter_schema),
        ),
        None => (
            default_kind_from_hint(&rec.source_key_hint, &rec.url),
            None,
            None,
            Vec::new(),
            rec.adult_hint,
            rec.site_label.clone(),
            None,
        ),
    };

    let combined_schema = match (rec.initial_schema, detected_schema) {
        (Some(a), Some(b)) => Some(merge_filter_schemas(Some(&a), b)),
        (Some(a), None) => Some(a),
        (None, Some(b)) => Some(b),
        (None, None) => None,
    };

    if let (Some(h), Some(sc)) = (root.host_str(), combined_schema.as_ref()) {
        let _ = deps.db.merge_host_filter_schema(h, sc).await;
    }

    let chosen_icon = resolve_and_cache_icon(
        &deps.http,
        &deps.images,
        &icon_candidates,
        &root,
        deps.max_mb,
    )
    .await;
    let source_key = build_source_key(kind, parser_ref.as_deref(), &root);
    let final_name = if detected_name.trim().is_empty() {
        rec.site_label.as_str()
    } else {
        detected_name.as_str()
    };
    let final_search = rec.search_tmpl.or(detected_tmpl);

    let _ = deps
        .db
        .upsert_discovered_source(&UpsertSavedSource {
            source_key: &source_key,
            display_name: final_name,
            base_url: root.as_str(),
            source_kind: kind.as_storage_str(),
            parser_ref: parser_ref.as_deref(),
            search_template: final_search.as_deref(),
            icon_url: chosen_icon.as_deref(),
            detected_adult,
            placement: SourcePlacement::Auto,
            prefer_new_name: false,
            filter_schema: combined_schema.as_ref(),
        })
        .await;
}

async fn resolve_and_cache_icon(
    http: &HttpClient,
    images: &ImageCache,
    candidates: &[Url],
    base_url: &Url,
    max_mb: u32,
) -> Option<String> {
    let site_host = base_url
        .host_str()
        .map(|h| h.trim_start_matches("www.").to_ascii_lowercase());

    for cand in candidates.iter().take(4) {
        let key = ImageCache::key_for_url(cand.as_str());
        if images.has_cached_url(cand.as_str()) {
            return Some(cand.as_str().to_owned());
        }
        let Ok(Ok(fetched)) = tokio::time::timeout(
            Duration::from_secs(4),
            http.fetch_image(cand, Some(base_url)),
        )
        .await
        else {
            continue;
        };
        if !fetched.bytes.is_empty() {
            images
                .put(
                    &key,
                    &fetched.bytes,
                    &fetched.content_type,
                    max_mb,
                    CacheEntryMeta {
                        site: site_host.as_deref(),
                        manga_id: None,
                        is_cover: true,
                        chapter_url: None,
                    },
                )
                .await;
            return Some(cand.as_str().to_owned());
        }
    }

    candidates.first().map(|u| u.as_str().to_owned())
}

fn build_saved_source_schema(row: &SavedSourceRow) -> SiteFilterSchema {
    let host = Url::parse(&row.base_url)
        .ok()
        .and_then(|u| u.host_str().map(str::to_owned));
    let builtin = default_schema_for_host_or_parser(host.as_deref(), row.parser_ref.as_deref());
    match row
        .filter_schema_json
        .as_deref()
        .and_then(|raw| serde_json::from_str::<SiteFilterSchema>(raw).ok())
    {
        Some(stored) => merge_filter_schemas(Some(&builtin), stored),
        None => builtin,
    }
}

fn map_saved_source_view(row: SavedSourceRow, proxy_key: &[u8; 32]) -> SavedSourceView {
    let site_host = Url::parse(&row.base_url).ok().and_then(|u| {
        u.host_str()
            .map(|h| h.trim_start_matches("www.").to_ascii_lowercase())
    });
    let icon_token = row
        .icon_url
        .as_deref()
        .map(|icon| encode_cover_token(icon, &row.base_url, None, site_host.as_deref(), proxy_key));
    let filter_schema = build_saved_source_schema(&row);

    SavedSourceView {
        id: row.id,
        source_key: row.source_key,
        display_name: row.display_name,
        base_url: row.base_url,
        source_kind: row.source_kind,
        parser_ref: row.parser_ref,
        search_template: row.search_template,
        icon_token,
        is_enabled: row.is_enabled != 0,
        is_supplemental: row.is_supplemental != 0,
        manual_visibility: row.manual_visibility != 0,
        created_at: row.created_at,
        updated_at: row.updated_at,
        filter_schema,
    }
}

fn build_source_key(kind: CmsKind, parser_ref: Option<&str>, base_url: &Url) -> String {
    if kind == CmsKind::BuiltInAdapter
        && let Some(id) = parser_ref
    {
        return format!("builtin:{id}");
    }
    let bare = base_url
        .host_str()
        .unwrap_or("unknown")
        .trim_start_matches("www.")
        .to_ascii_lowercase();
    format!("host:{bare}")
}

fn infer_site_label(url: &Url, hint: &str) -> String {
    if let Some(host) = url.host_str() {
        return host.trim_start_matches("www.").to_owned();
    }
    hint.to_owned()
}

fn default_kind_from_hint(_hint: &str, url: &Url) -> CmsKind {
    let host = url.host_str().unwrap_or("").to_ascii_lowercase();
    if match_builtin_adapter_id(&host).is_some() {
        return CmsKind::BuiltInAdapter;
    }
    CmsKind::UniversalHeuristic
}
