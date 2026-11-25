//! Saved source onboarding, catalog browsing, parallel multi-source search,
//! supplemental visibility isolation, and custom JSON rule pack management.

use std::sync::Arc;
use std::time::Duration;

use mangacore::{
    CatalogPage, CmsKind, CustomRuleSpec, HttpClient, Manga, browse_or_search_cms,
    detect_site_structure, match_builtin_adapter_id, normalize_source_root,
    parse::meta::detect_adult_signals, validate_rule_spec,
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
    RuleSpecRow, SavedSourceRow, SourcePlacement, UpdateSavedSource, UpsertSavedSource,
    VisibilityFilter, VisibilityPatch,
};
use crate::services::library::{CatalogEntryView, CatalogPageView, map_catalog_view};
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
}

/// Custom declarative JSON rule pack summary sent to the UI.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RuleSpecView {
    pub rule_id: String,
    pub name: String,
    pub version: i64,
    pub origin: String,
    pub is_enabled: bool,
    pub domains: Vec<String>,
    pub updated_at: i64,
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

/// Probes `raw_url` using structure-first CMS classification + HTML favicon discovery,
/// caches its icon into `covers/`, and saves the source into SQLite.
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
    let custom_rules = state
        .db
        .list_enabled_custom_rules()
        .await
        .unwrap_or_default();
    let http = state.registry.http();
    let outcome = detect_site_structure(http, raw_url, &custom_rules)
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
    Ok(map_saved_source_view(row, &state.proxy_key))
}

/// Deletes a saved source by ID.
///
/// # Errors
///
/// Returns [`AppError::Db`] on SQLite failure.
pub(crate) async fn delete_saved_source(state: &AppState, id: i64) -> Result<(), AppError> {
    state.db.delete_saved_source(id).await
}

/// Browses or searches a single saved source (automatically applying its saved mirror URL,
/// CMS structure kind, and one-shot fallback).
///
/// # Errors
///
/// Returns [`AppError`] when the source does not exist or upstream resolution fails.
pub(crate) async fn browse_saved_source(
    state: &AppState,
    source_id: i64,
    query: Option<&str>,
    page: u32,
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
    let custom_rules = state
        .db
        .list_enabled_custom_rules()
        .await
        .unwrap_or_default();

    let mut cat = browse_or_search_cms(
        state.registry.http(),
        kind,
        &base_url,
        row.parser_ref.as_deref(),
        row.search_template.as_deref(),
        query,
        page.max(1),
        &custom_rules,
    )
    .await
    .map_err(AppError::from)?;

    if cat.site_name.trim().is_empty() || cat.site_name == cat.site.as_storage_key() {
        cat.site_name.clone_from(&row.display_name);
    }

    let max_mb = state.current_settings().await.cache.max_mb;
    prefetch_catalog_covers(state.registry.http(), &state.images, &cat, max_mb);
    Ok(map_catalog_view(cat, &state.proxy_key))
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

    let custom_rules = Arc::new(
        state
            .db
            .list_enabled_custom_rules()
            .await
            .unwrap_or_default(),
    );
    let max_mb = state.current_settings().await.cache.max_mb;
    let sem = Arc::new(Semaphore::new(4));
    let mut set = JoinSet::new();

    for (idx, row) in sources.into_iter().enumerate() {
        let permit_sem = Arc::clone(&sem);
        let state_ref = Arc::clone(state);
        let rules_ref = Arc::clone(&custom_rules);
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
            let group = search_single_source(&state_ref, row, &q_owned, &rules_ref, max_mb).await;
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
    custom_rules: &[CustomRuleSpec],
    max_mb: u32,
) -> SourceSearchGroupView {
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
        custom_rules,
    );

    match tokio::time::timeout(Duration::from_secs(12), fut).await {
        Ok(Ok(cat)) => {
            prefetch_catalog_covers(state.registry.http(), &state.images, &cat, max_mb);
            let mapped = map_catalog_view(cat, &state.proxy_key);
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
}

/// Spawns non-blocking background source recording when a manga is resolved.
pub(crate) fn spawn_auto_record_from_manga(deps: AutoRecordDeps, url: Url, manga: &Manga) {
    let record = AutoSourceRecord {
        adult_hint: detect_adult_signals(&url, "", &manga.genres, &manga.tags),
        site_label: infer_site_label(&url, manga.site.as_storage_key().as_str()),
        source_key_hint: manga.site.as_storage_key(),
        search_tmpl: None,
        url,
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
    };
    tokio::spawn(async move {
        auto_record_source_task(deps, record).await;
    });
}

async fn auto_record_source_task(deps: AutoRecordDeps, rec: AutoSourceRecord) {
    let Ok(root) = normalize_source_root(rec.url.as_str()) else {
        return;
    };
    let rules = deps
        .db
        .list_enabled_custom_rules()
        .await
        .unwrap_or_default();
    let outcome = detect_site_structure(&deps.http, root.as_str(), &rules)
        .await
        .ok();

    let (kind, parser_ref, detected_tmpl, icon_candidates, detected_adult, detected_name) =
        match outcome {
            Some(o) => (
                o.kind,
                o.parser_ref,
                o.search_template,
                o.icon_candidates,
                o.adult_detected || rec.adult_hint,
                o.display_name,
            ),
            None => (
                default_kind_from_hint(&rec.source_key_hint, &rec.url),
                None,
                None,
                Vec::new(),
                rec.adult_hint,
                rec.site_label.clone(),
            ),
        };

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
        })
        .await;
}

/// Lists all stored custom JSON rule specifications.
///
/// # Errors
///
/// Returns [`AppError::Db`] on SQLite failure.
pub(crate) async fn list_rule_specs(state: &AppState) -> Result<Vec<RuleSpecView>, AppError> {
    let rows = state.db.list_rule_specs().await?;
    Ok(rows.into_iter().map(map_rule_spec_view).collect())
}

/// Validates and imports a single [`CustomRuleSpec`] JSON object or an array of specs.
///
/// # Errors
///
/// Returns [`AppError::InvalidUrl`] on JSON syntax/selector validation errors or [`AppError::Db`] on SQLite failure.
pub(crate) async fn import_rule_specs_json(
    state: &AppState,
    raw_json: &str,
) -> Result<Vec<RuleSpecView>, AppError> {
    let trimmed = raw_json.trim();
    if trimmed.is_empty() {
        return Err(AppError::InvalidUrl {
            message: "JSON rule specification must not be empty".to_owned(),
        });
    }

    let specs: Vec<CustomRuleSpec> = if trimmed.starts_with('[') {
        serde_json::from_str(trimmed).map_err(|e| AppError::InvalidUrl {
            message: format!("invalid rule pack JSON array: {e}"),
        })?
    } else {
        let single: CustomRuleSpec =
            serde_json::from_str(trimmed).map_err(|e| AppError::InvalidUrl {
                message: format!("invalid rule pack JSON object: {e}"),
            })?;
        vec![single]
    };

    if specs.is_empty() {
        return Err(AppError::InvalidUrl {
            message: "rule pack array contains zero entries".to_owned(),
        });
    }

    let mut imported = Vec::with_capacity(specs.len());
    for spec in &specs {
        if let Err(msg) = validate_rule_spec(spec) {
            return Err(AppError::InvalidUrl {
                message: format!("invalid rule '{}': {msg}", spec.name),
            });
        }
        let pretty = serde_json::to_string_pretty(spec).map_err(|e| AppError::Internal {
            message: format!("failed to serialize validated rule spec: {e}"),
        })?;
        let row = state
            .db
            .upsert_rule_spec(spec, &pretty, "user_imported")
            .await?;
        imported.push(map_rule_spec_view(row));
    }

    Ok(imported)
}

/// Exports all stored custom JSON rule packs as a formatted JSON array string.
///
/// # Errors
///
/// Returns [`AppError`] on SQLite or JSON serialization error.
pub(crate) async fn export_rule_specs_json(state: &AppState) -> Result<String, AppError> {
    let rows = state.db.list_rule_specs().await?;
    let mut specs: Vec<CustomRuleSpec> = Vec::with_capacity(rows.len());
    for row in rows {
        if let Ok(spec) = serde_json::from_str::<CustomRuleSpec>(&row.raw_json) {
            specs.push(spec);
        }
    }
    serde_json::to_string_pretty(&specs).map_err(|e| AppError::Internal {
        message: format!("failed to export rule specs: {e}"),
    })
}

/// Enables or disables a custom JSON rule pack.
///
/// # Errors
///
/// Returns [`AppError::Db`] on SQLite failure.
#[expect(
    clippy::fn_params_excessive_bools,
    reason = "boolean enable/disable toggle for custom rule spec"
)]
pub(crate) async fn set_rule_spec_enabled(
    state: &AppState,
    rule_id: &str,
    enabled: bool,
) -> Result<(), AppError> {
    state.db.set_rule_spec_enabled(rule_id, enabled).await
}

/// Deletes a custom JSON rule pack by ID.
///
/// # Errors
///
/// Returns [`AppError::Db`] on SQLite failure.
pub(crate) async fn delete_rule_spec(state: &AppState, rule_id: &str) -> Result<(), AppError> {
    state.db.delete_rule_spec(rule_id).await
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

fn prefetch_catalog_covers(http: &HttpClient, images: &ImageCache, cat: &CatalogPage, max_mb: u32) {
    let site_key = cat.site.as_storage_key();
    for entry in cat.entries.iter().take(12) {
        if let Some(ref cover) = entry.cover {
            images.spawn_cover_prefetch(
                http.clone(),
                cover.clone(),
                cat.url.clone(),
                Some(site_key.clone()),
                None,
                max_mb,
            );
        }
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
    }
}

fn map_rule_spec_view(row: RuleSpecRow) -> RuleSpecView {
    let domains = serde_json::from_str::<CustomRuleSpec>(&row.raw_json)
        .map(|s| s.domains)
        .unwrap_or_default();
    RuleSpecView {
        rule_id: row.rule_id,
        name: row.name,
        version: row.version,
        origin: row.origin,
        is_enabled: row.is_enabled != 0,
        domains,
        updated_at: row.updated_at,
    }
}

fn build_source_key(kind: CmsKind, parser_ref: Option<&str>, base_url: &Url) -> String {
    if kind == CmsKind::BuiltInAdapter
        && let Some(id) = parser_ref
    {
        return format!("builtin:{id}");
    }
    if kind == CmsKind::CustomRule
        && let Some(id) = parser_ref
    {
        return format!("rule:{id}");
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

fn default_kind_from_hint(hint: &str, url: &Url) -> CmsKind {
    let host = url.host_str().unwrap_or("").to_ascii_lowercase();
    if match_builtin_adapter_id(&host).is_some() {
        return CmsKind::BuiltInAdapter;
    }
    if hint.starts_with("rule:") {
        return CmsKind::CustomRule;
    }
    CmsKind::UniversalHeuristic
}
