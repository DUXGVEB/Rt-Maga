//! Dynamic per-website filter schema discovery, server filter URL construction,
//! and client-side fallback filtering (3-state multi-tag, minimum chapters, update recency).

use std::collections::{BTreeMap, HashSet};
use std::sync::LazyLock;

use regex::Regex;
use scraper::{ElementRef, Html, Selector};
use serde::{Deserialize, Serialize};
use url::Url;

use crate::cms::CmsKind;
use crate::error::ScrapeError;
use crate::model::CatalogEntry;
use crate::parse::chapters::extract_chapter_number;
use crate::parse::dom::{abs_url, clean_text};

static FORM_SEL: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("form").expect("valid selector"));
static SELECT_SEL: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("select[name]").expect("valid selector"));
static OPTION_SEL: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("option").expect("valid selector"));
static CHECKBOX_SEL: LazyLock<Selector> = LazyLock::new(|| {
    Selector::parse("input[type='checkbox'][name], input[type='radio'][name]")
        .expect("valid selector")
});
static LABEL_SEL: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("label").expect("valid selector"));
static LINK_SEL: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("a[href]").expect("valid selector"));

static RELATIVE_TIME_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)(\d+)\s*(giây|phút|giờ|tiếng|ngày|tuần|tháng|năm|sec|second|min|minute|hour|hr|day|week|month|year)s?",
    )
    .expect("valid relative time regex")
});
static DMY_DATE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\b(\d{1,2})[/-](\d{1,2})[/-](\d{2,4})\b").expect("valid dmy regex")
});
static YMD_DATE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\b(\d{4})-(\d{2})-(\d{2})\b").expect("valid ymd regex"));

/// Single genre/tag option belonging to a specific website's filter catalog.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FilterTagOption {
    /// Upstream parameter value (numeric category ID, UUID, or slug).
    pub id: String,
    /// Human-readable display label on that website.
    pub label: String,
    /// Optional standalone category page URL for single-tag fallback.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

/// Selection mode of a dynamically discovered site filter group.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum FilterAxisKind {
    /// Single value dropdown/pill group (`status`, `country`, `sort`, `gender`, `year`).
    #[default]
    SingleSelect,
    /// Multi-value checkbox group.
    MultiSelect,
}

/// Option inside a [`FilterAxis`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FilterOptionItem {
    /// Parameter value sent in the URL query string.
    pub value: String,
    /// Human-readable option label.
    pub label: String,
}

/// Dynamically discovered filter group specific to a website (e.g. Status, Country, Sort, Demographic).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FilterAxis {
    /// Query parameter key (e.g. `"status"`, `"country"`, `"sort"`).
    pub key: String,
    /// Human-readable filter title.
    pub label: String,
    /// Selection cardinality.
    #[serde(default)]
    pub kind: FilterAxisKind,
    /// Choices exposed by the website.
    pub options: Vec<FilterOptionItem>,
}

/// Always-available update recency window filter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum UpdatedWithinFilter {
    /// Show manga updated at any time.
    #[default]
    Any,
    /// Updated within the last 24 hours.
    Hours24,
    /// Updated within the last 7 days.
    Days7,
    /// Updated within the last 30 days.
    Days30,
    /// Updated within the last 365 days.
    Days365,
}

impl UpdatedWithinFilter {
    /// Maximum age in hours allowed by this filter, or `None` for [`Self::Any`].
    #[must_use]
    pub const fn max_hours(self) -> Option<u64> {
        match self {
            Self::Any => None,
            Self::Hours24 => Some(24),
            Self::Days7 => Some(24 * 7),
            Self::Days30 => Some(24 * 30),
            Self::Days365 => Some(24 * 365),
        }
    }
}

/// Per-website filter capabilities and discovered options persisted per saved source.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SiteFilterSchema {
    /// Target endpoint URL for submitting multi-parameter filter queries, if discovered.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filter_endpoint: Option<String>,
    /// Query parameter key for included (`✓`) tags.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include_tag_param: Option<String>,
    /// Query parameter key for excluded (`✕`) tags.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exclude_tag_param: Option<String>,
    /// `true` when multiple tags are joined with `,` in one parameter (e.g. `category=26,30`).
    pub comma_joined_tags: bool,
    /// Native server parameter for minimum chapter count (e.g. `"minchapter"`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_chapters_param: Option<String>,
    /// Native server parameter for keyword search on the filter endpoint.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keyword_param: Option<String>,
    /// Site-specific tag/genre catalog supporting 3-state (`✓` / `✕` / off) selection.
    pub tags: Vec<FilterTagOption>,
    /// Additional site-specific filter axes discovered from HTML `<form>`/`<select>` or API specs.
    pub extra_axes: Vec<FilterAxis>,
}

/// Active filter selection submitted by the user in `Sources` or `Browse`.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct CatalogFilterQuery {
    /// Optional keyword query.
    pub query: Option<String>,
    /// Tag IDs that must be included (`✓`).
    pub include_tags: Vec<String>,
    /// Tag IDs that must be excluded (`✕`).
    pub exclude_tags: Vec<String>,
    /// Minimum chapter count (`None` or `0` = any).
    pub min_chapters: Option<u32>,
    /// Relative update time window.
    pub updated_within: UpdatedWithinFilter,
    /// Dynamic extra axis values keyed by `FilterAxis::key`.
    pub extra: BTreeMap<String, Vec<String>>,
}

impl CatalogFilterQuery {
    /// Returns `true` when no filter or search criterion is active.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.query.as_deref().is_none_or(|q| q.trim().is_empty())
            && self.include_tags.is_empty()
            && self.exclude_tags.is_empty()
            && self.min_chapters.unwrap_or(0) == 0
            && self.updated_within == UpdatedWithinFilter::Any
            && self
                .extra
                .values()
                .all(|vals| vals.iter().all(|v| v.trim().is_empty()))
    }
}

/// Tracks which active filter criteria could not be pushed down to the upstream server URL
/// and therefore must be enforced client-side on the returned catalog entries.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "internal bitmask of independent unhandled filter dimensions"
)]
pub struct UnhandledFilterMask {
    /// Client must verify `include_tags` against entry genres when available.
    pub include_tags: bool,
    /// Client must filter out `exclude_tags` against entry genres.
    pub exclude_tags: bool,
    /// Client must enforce `min_chapters`.
    pub min_chapters: bool,
    /// Client must enforce `updated_within`.
    pub updated_within: bool,
}

impl UnhandledFilterMask {
    /// Returns `true` if at least one filter criterion requires client-side post-filtering.
    #[must_use]
    pub const fn any(self) -> bool {
        self.include_tags || self.exclude_tags || self.min_chapters || self.updated_within
    }
}

/// Dynamically extracts a website's filter schema (tags, `<form>` endpoints, `<select>` dropdowns,
/// checkbox groups, and category navigation links) directly from its HTML DOM.
#[must_use]
#[expect(
    clippy::too_many_lines,
    reason = "inspects HTML forms, selects, checkboxes, and category links in one pass"
)]
pub fn extract_generic_filter_schema(
    doc: &Html,
    base: &Url,
    cms_kind: Option<CmsKind>,
) -> SiteFilterSchema {
    let mut schema = SiteFilterSchema::default();
    let host = base.host_str().unwrap_or("").to_ascii_lowercase();

    // 1. Inspect `<form>` controls for native multi-tag checkboxes and `<select>` filter axes.
    for form in doc.select(&FORM_SEL) {
        let method = form.value().attr("method").unwrap_or("get");
        if !method.eq_ignore_ascii_case("get") {
            continue;
        }
        let selects: Vec<ElementRef<'_>> = form.select(&SELECT_SEL).collect();
        let checkboxes: Vec<ElementRef<'_>> = form.select(&CHECKBOX_SEL).collect();
        if selects.is_empty() && checkboxes.len() < 3 {
            continue;
        }

        if schema.filter_endpoint.is_none()
            && let Some(action_url) = abs_url(base, form.value().attr("action").unwrap_or("/"))
        {
            schema.filter_endpoint = Some(action_url.into());
        }

        extract_checkbox_tags_from_form(form, &checkboxes, &mut schema);
    }

    // 2. Collect `<select name="...">` axes across the document.
    for axis in extract_select_axes(doc) {
        let key_low = axis.key.to_ascii_lowercase();
        if key_low.contains("minchap") || key_low == "min_chapters" {
            schema.min_chapters_param = Some(axis.key);
            continue;
        }
        if !schema
            .extra_axes
            .iter()
            .any(|a| a.key.eq_ignore_ascii_case(&axis.key))
        {
            schema.extra_axes.push(axis);
        }
    }

    // 3. Apply CMS-family defaults when known (`Madara` / `MangaThemesia` / `WpComics`).
    match cms_kind {
        Some(CmsKind::Madara) => {
            if schema.filter_endpoint.is_none() {
                schema.filter_endpoint = Some(format!(
                    "{}://{}/?s=&post_type=wp-manga",
                    base.scheme(),
                    host
                ));
            }
            if schema.include_tag_param.is_none() {
                schema.include_tag_param = Some("genre[]".to_owned());
            }
            if schema.keyword_param.is_none() {
                schema.keyword_param = Some("s".to_owned());
            }
            ensure_default_madara_axes(&mut schema.extra_axes);
        }
        Some(CmsKind::MangaThemesia) => {
            if schema.filter_endpoint.is_none()
                && let Ok(u) = base.join("/manga/")
            {
                schema.filter_endpoint = Some(u.into());
            }
            if schema.include_tag_param.is_none() {
                schema.include_tag_param = Some("genre[]".to_owned());
            }
        }
        Some(CmsKind::WpComics) => {
            if schema.filter_endpoint.is_none()
                && let Ok(u) = base.join("/tim-truyen-nang-cao")
            {
                schema.filter_endpoint = Some(u.into());
            }
            if schema.include_tag_param.is_none() {
                schema.include_tag_param = Some("genres".to_owned());
                schema.comma_joined_tags = true;
            }
            if schema.exclude_tag_param.is_none() {
                schema.exclude_tag_param = Some("notgenres".to_owned());
            }
            if schema.min_chapters_param.is_none() {
                schema.min_chapters_param = Some("minchapter".to_owned());
            }
        }
        _ => {}
    }

    // 4. Harvest category/genre/tag links from the page so tags are always populated even without `<form>`.
    let mut seen_labels: HashSet<String> = schema
        .tags
        .iter()
        .map(|t| t.label.to_ascii_lowercase())
        .collect();
    for a in doc.select(&LINK_SEL) {
        let Some(href) = a.value().attr("href") else {
            continue;
        };
        let Some(u) = abs_url(base, href) else {
            continue;
        };
        if u.host_str().unwrap_or("").to_ascii_lowercase() != host {
            continue;
        }
        let path_low = u.path().to_ascii_lowercase();
        if ![
            "/genre/",
            "/genres/",
            "/the-loai/",
            "/manga-genre/",
            "/tag/",
            "/tags/",
            "/category/",
            "/danh-muc/",
        ]
        .iter()
        .any(|p| path_low.contains(p))
        {
            continue;
        }
        let label = clean_text(&a.text().collect::<String>());
        if label.len() < 2 || label.len() > 40 {
            continue;
        }
        if !seen_labels.insert(label.to_ascii_lowercase()) {
            continue;
        }
        let slug = u
            .path_segments()
            .and_then(|mut segs| segs.rfind(|s| !s.is_empty()))
            .unwrap_or(&label)
            .trim_end_matches(".html")
            .trim_end_matches(".htm")
            .to_owned();
        schema.tags.push(FilterTagOption {
            id: slug,
            label,
            url: Some(u.into()),
        });
        if schema.tags.len() >= 120 {
            break;
        }
    }

    schema
}

/// Merges a newly observed [`SiteFilterSchema`] into an existing cached schema without losing
/// previously discovered tags or dynamic filter axes.
#[must_use]
pub fn merge_filter_schemas(
    existing: Option<&SiteFilterSchema>,
    mut fresh: SiteFilterSchema,
) -> SiteFilterSchema {
    let Some(prev) = existing else {
        return fresh;
    };

    if fresh.filter_endpoint.is_none() {
        fresh.filter_endpoint.clone_from(&prev.filter_endpoint);
    }
    if fresh.include_tag_param.is_none() {
        fresh.include_tag_param.clone_from(&prev.include_tag_param);
    }
    if fresh.exclude_tag_param.is_none() {
        fresh.exclude_tag_param.clone_from(&prev.exclude_tag_param);
    }
    if !fresh.comma_joined_tags && prev.comma_joined_tags {
        fresh.comma_joined_tags = true;
    }
    if fresh.min_chapters_param.is_none() {
        fresh
            .min_chapters_param
            .clone_from(&prev.min_chapters_param);
    }
    if fresh.keyword_param.is_none() {
        fresh.keyword_param.clone_from(&prev.keyword_param);
    }

    let mut seen_tag_ids: HashSet<String> = fresh
        .tags
        .iter()
        .map(|t| t.id.to_ascii_lowercase())
        .collect();
    for old_tag in &prev.tags {
        if seen_tag_ids.insert(old_tag.id.to_ascii_lowercase()) {
            fresh.tags.push(old_tag.clone());
        }
    }

    for old_axis in &prev.extra_axes {
        if let Some(target) = fresh
            .extra_axes
            .iter_mut()
            .find(|a| a.key.eq_ignore_ascii_case(&old_axis.key))
        {
            for opt in &old_axis.options {
                if !target.options.iter().any(|o| o.value == opt.value) {
                    target.options.push(opt.clone());
                }
            }
        } else {
            fresh.extra_axes.push(old_axis.clone());
        }
    }

    fresh
}

fn extract_checkbox_tags_from_form(
    form: ElementRef<'_>,
    checkboxes: &[ElementRef<'_>],
    schema: &mut SiteFilterSchema,
) {
    let mut labels_by_for: BTreeMap<String, String> = BTreeMap::new();
    for lbl in form.select(&LABEL_SEL) {
        if let Some(for_id) = lbl.value().attr("for") {
            let txt = clean_text(&lbl.text().collect::<String>());
            if !txt.is_empty() {
                labels_by_for.insert(for_id.to_owned(), txt);
            }
        }
    }

    let mut seen_ids: HashSet<String> = schema.tags.iter().map(|t| t.id.clone()).collect();
    for cb in checkboxes {
        let Some(name) = cb.value().attr("name").map(str::trim) else {
            continue;
        };
        let Some(val) = cb.value().attr("value").map(str::trim) else {
            continue;
        };
        if name.is_empty() || val.is_empty() {
            continue;
        }
        let name_low = name.to_ascii_lowercase();
        let label = cb
            .value()
            .attr("id")
            .and_then(|id| labels_by_for.get(id).cloned())
            .or_else(|| {
                cb.parent()
                    .and_then(ElementRef::wrap)
                    .filter(|p| p.value().name() == "label")
                    .map(|p| clean_text(&p.text().collect::<String>()))
            })
            .unwrap_or_else(|| val.to_owned());
        if label.is_empty() || label.len() > 42 {
            continue;
        }

        if name_low.contains("genre") || name_low.contains("tag") || name_low.contains("cat") {
            if name_low.contains("not") || name_low.contains("excl") {
                if schema.exclude_tag_param.is_none() {
                    schema.exclude_tag_param = Some(name.to_owned());
                }
            } else if schema.include_tag_param.is_none() {
                schema.include_tag_param = Some(name.to_owned());
            }
            if seen_ids.insert(val.to_owned()) {
                schema.tags.push(FilterTagOption {
                    id: val.to_owned(),
                    label,
                    url: None,
                });
            }
        }
    }
}

pub(crate) fn extract_select_axes(doc: &Html) -> Vec<FilterAxis> {
    let mut labels_by_for: BTreeMap<String, String> = BTreeMap::new();
    for lbl in doc.select(&LABEL_SEL) {
        if let Some(for_id) = lbl.value().attr("for") {
            let txt = clean_text(&lbl.text().collect::<String>());
            if !txt.is_empty() {
                labels_by_for.insert(for_id.to_owned(), txt);
            }
        }
    }

    let mut axes = Vec::new();
    for sel_el in doc.select(&SELECT_SEL) {
        let Some(name) = sel_el.value().attr("name").map(str::trim) else {
            continue;
        };
        if name.is_empty()
            || matches!(
                name.to_ascii_lowercase().as_str(),
                "page" | "paged" | "limit" | "per_page"
            )
        {
            continue;
        }

        let mut options = Vec::new();
        let mut seen_vals = HashSet::new();
        for opt in sel_el.select(&OPTION_SEL) {
            let val = opt.value().attr("value").unwrap_or("").trim().to_owned();
            let label = clean_text(&opt.text().collect::<String>());
            if label.is_empty() || label.len() > 48 {
                continue;
            }
            if seen_vals.insert(val.clone()) {
                options.push(FilterOptionItem { value: val, label });
            }
            if options.len() >= 40 {
                break;
            }
        }
        if options.len() < 2 {
            continue;
        }

        let label = sel_el
            .value()
            .attr("id")
            .and_then(|id| labels_by_for.get(id).cloned())
            .or_else(|| sel_el.value().attr("aria-label").map(clean_text))
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| humanize_axis_key(name));
        let kind = if sel_el.value().attr("multiple").is_some() || name.ends_with("[]") {
            FilterAxisKind::MultiSelect
        } else {
            FilterAxisKind::SingleSelect
        };

        axes.push(FilterAxis {
            key: name.to_owned(),
            label,
            kind,
            options,
        });
    }
    axes
}

fn ensure_default_madara_axes(axes: &mut Vec<FilterAxis>) {
    if !axes.iter().any(|a| a.key.contains("status")) {
        axes.push(FilterAxis {
            key: "status[]".to_owned(),
            label: "Status".to_owned(),
            kind: FilterAxisKind::MultiSelect,
            options: vec![
                FilterOptionItem {
                    value: "on-going".to_owned(),
                    label: "Ongoing".to_owned(),
                },
                FilterOptionItem {
                    value: "end".to_owned(),
                    label: "Completed".to_owned(),
                },
                FilterOptionItem {
                    value: "on-hold".to_owned(),
                    label: "On Hold".to_owned(),
                },
            ],
        });
    }
    if !axes
        .iter()
        .any(|a| a.key.contains("order") || a.key.contains("sort"))
    {
        axes.push(FilterAxis {
            key: "m_orderby".to_owned(),
            label: "Order By".to_owned(),
            kind: FilterAxisKind::SingleSelect,
            options: vec![
                FilterOptionItem {
                    value: "latest".to_owned(),
                    label: "Latest".to_owned(),
                },
                FilterOptionItem {
                    value: "views".to_owned(),
                    label: "Most Views".to_owned(),
                },
                FilterOptionItem {
                    value: "rating".to_owned(),
                    label: "Rating".to_owned(),
                },
                FilterOptionItem {
                    value: "new-manga".to_owned(),
                    label: "New".to_owned(),
                },
            ],
        });
    }
}

fn humanize_axis_key(raw: &str) -> String {
    let bare = raw.trim_end_matches("[]").trim_matches('_');
    match bare.to_ascii_lowercase().as_str() {
        "status" | "tinh-trang" => "Status".to_owned(),
        "sort" | "order" | "orderby" | "m_orderby" | "sap-xep" => "Sort".to_owned(),
        "country" | "quoc-gia" | "type" => "Type / Country".to_owned(),
        "gender" | "doi-tuong" => "Target".to_owned(),
        "year" | "nam" | "release" => "Year".to_owned(),
        other => {
            let mut chars = other.replace(['_', '-'], " ").chars().collect::<Vec<_>>();
            if let Some(first) = chars.first_mut() {
                *first = first.to_ascii_uppercase();
            }
            chars.into_iter().collect()
        }
    }
}

/// Builds the upstream catalog/filter request URL for `filters` and `page`, returning both the
/// target [`Url`] and an [`UnhandledFilterMask`] indicating any criteria that must be filtered
/// client-side after fetching.
///
/// # Errors
///
/// Returns [`ScrapeError::InvalidUrl`] if URL construction fails.
#[expect(
    clippy::too_many_arguments,
    clippy::too_many_lines,
    reason = "maps multi-tag, min-chapter, recency, dynamic axes, and pagination across CMS families"
)]
pub fn build_filtered_catalog_url(
    base_url: &Url,
    current_url: Option<&Url>,
    kind: CmsKind,
    parser_ref: Option<&str>,
    search_template: Option<&str>,
    schema: &SiteFilterSchema,
    filters: &CatalogFilterQuery,
    page: u32,
) -> Result<(Url, UnhandledFilterMask), ScrapeError> {
    let page_num = page.max(1);
    let clean_q = filters
        .query
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());

    // Delegate to dedicated first-party adapter filter routing when applicable.
    if let Some(res) = crate::source::try_build_builtin_filtered_url(
        base_url,
        current_url,
        parser_ref,
        clean_q,
        filters,
        page_num,
    ) {
        return res;
    }

    let mut unhandled = UnhandledFilterMask {
        include_tags: !filters.include_tags.is_empty(),
        exclude_tags: !filters.exclude_tags.is_empty(),
        min_chapters: filters.min_chapters.unwrap_or(0) > 0,
        updated_within: filters.updated_within != UpdatedWithinFilter::Any,
    };

    // Single-tag direct URL shortcut when the site has no `include_tag_param` form endpoint
    if schema.include_tag_param.is_none()
        && filters.include_tags.len() == 1
        && clean_q.is_none()
        && let Some(tag_opt) = schema
            .tags
            .iter()
            .find(|t| t.id.eq_ignore_ascii_case(&filters.include_tags[0]))
        && let Some(tag_url_str) = tag_opt.url.as_deref()
        && let Ok(mut tag_url) = Url::parse(tag_url_str)
    {
        unhandled.include_tags = false;
        if page_num > 1 {
            append_generic_page(&mut tag_url, kind, page_num);
        }
        return Ok((tag_url, unhandled));
    }

    // Keyword-only search when no tag/extra form parameters are active
    if let Some(q) = clean_q
        && filters.include_tags.is_empty()
        && filters.exclude_tags.is_empty()
        && filters.extra.is_empty()
        && schema.keyword_param.is_none()
        && let Some(tmpl) = search_template.filter(|t| t.contains("{query}"))
    {
        let encoded: String = url::form_urlencoded::byte_serialize(q.as_bytes()).collect();
        let mut u = Url::parse(&tmpl.replace("{query}", &encoded))
            .map_err(|e| ScrapeError::InvalidUrl(e.to_string()))?;
        if page_num > 1 {
            append_generic_page(&mut u, kind, page_num);
        }
        return Ok((u, unhandled));
    }

    let mut target = if !filters.is_empty()
        && let Some(ep) = schema.filter_endpoint.as_deref()
        && let Ok(u) = Url::parse(ep)
    {
        u
    } else if let Some(cur) = current_url {
        cur.clone()
    } else {
        base_url.clone()
    };

    {
        let mut pairs = target.query_pairs_mut();
        if let Some(q) = clean_q {
            let kw = schema.keyword_param.as_deref().unwrap_or("s");
            pairs.append_pair(kw, q);
        }
        if !filters.include_tags.is_empty()
            && let Some(param) = schema.include_tag_param.as_deref()
        {
            if schema.comma_joined_tags {
                pairs.append_pair(param, &filters.include_tags.join(","));
            } else {
                for t in &filters.include_tags {
                    pairs.append_pair(param, t);
                }
            }
            unhandled.include_tags = false;
        }
        if !filters.exclude_tags.is_empty()
            && let Some(param) = schema.exclude_tag_param.as_deref()
        {
            if schema.comma_joined_tags {
                pairs.append_pair(param, &filters.exclude_tags.join(","));
            } else {
                for t in &filters.exclude_tags {
                    pairs.append_pair(param, t);
                }
            }
            unhandled.exclude_tags = false;
        }
        if let Some(min_ch) = filters.min_chapters.filter(|&n| n > 0)
            && let Some(param) = schema.min_chapters_param.as_deref()
        {
            pairs.append_pair(param, &min_ch.to_string());
            unhandled.min_chapters = false;
        }
        for (k, vals) in &filters.extra {
            for v in vals {
                if !v.trim().is_empty() {
                    pairs.append_pair(k, v.trim());
                }
            }
        }
    }

    if page_num > 1 {
        append_generic_page(&mut target, kind, page_num);
    }

    Ok((target, unhandled))
}

fn append_generic_page(url: &mut Url, kind: CmsKind, page_num: u32) {
    if page_num <= 1 {
        return;
    }
    if matches!(kind, CmsKind::Madara | CmsKind::MangaThemesia) {
        let base_path = url.path().trim_end_matches('/');
        if !base_path.contains("/page/") {
            url.set_path(&format!("{base_path}/page/{page_num}/"));
            return;
        }
    }
    let kept: Vec<(String, String)> = url
        .query_pairs()
        .filter(|(k, _)| !k.eq_ignore_ascii_case("page"))
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();
    let mut qp = url.query_pairs_mut();
    qp.clear();
    for (k, v) in &kept {
        qp.append_pair(k, v);
    }
    qp.append_pair("page", &page_num.to_string());
}

/// Applies client-side fallback filters (`include_tags`, `exclude_tags`, `min_chapters`,
/// `updated_within`) onto `entries` for criteria not natively supported by the upstream website.
pub fn filter_entries_client_side(
    entries: &mut Vec<CatalogEntry>,
    schema: &SiteFilterSchema,
    filters: &CatalogFilterQuery,
    unhandled: UnhandledFilterMask,
) {
    if !unhandled.any() {
        return;
    }

    let inc_labels: Vec<String> = if unhandled.include_tags {
        resolve_tag_labels(schema, &filters.include_tags)
    } else {
        Vec::new()
    };
    let exc_labels: Vec<String> = if unhandled.exclude_tags {
        resolve_tag_labels(schema, &filters.exclude_tags)
    } else {
        Vec::new()
    };
    let min_ch = filters.min_chapters.unwrap_or(0);
    let max_age_hours = filters.updated_within.max_hours();

    entries.retain(|entry| {
        if !inc_labels.is_empty() && !entry.genres.is_empty() {
            let entry_genres: Vec<String> = entry
                .genres
                .iter()
                .map(|g| g.to_ascii_lowercase())
                .collect();
            let all_present = inc_labels.iter().all(|req| {
                entry_genres
                    .iter()
                    .any(|eg| eg.contains(req) || req.contains(eg))
            });
            if !all_present {
                return false;
            }
        }

        if !exc_labels.is_empty() && !entry.genres.is_empty() {
            let entry_genres: Vec<String> = entry
                .genres
                .iter()
                .map(|g| g.to_ascii_lowercase())
                .collect();
            let any_excluded = exc_labels.iter().any(|exc| {
                entry_genres
                    .iter()
                    .any(|eg| eg.contains(exc) || exc.contains(eg))
            });
            if any_excluded {
                return false;
            }
        }

        if unhandled.min_chapters && min_ch > 0 {
            let count = entry.chapter_count.or_else(|| {
                entry
                    .latest_chapter
                    .as_deref()
                    .and_then(extract_chapter_number)
                    .and_then(|n| {
                        if n >= 0.0 {
                            #[expect(
                                clippy::as_conversions,
                                clippy::cast_possible_truncation,
                                clippy::cast_sign_loss,
                                reason = "chapter number is non-negative and clamped to u32 range"
                            )]
                            Some(n.floor().min(f32::from(u16::MAX)) as u32)
                        } else {
                            None
                        }
                    })
            });
            if let Some(c) = count
                && c < min_ch
            {
                return false;
            }
            if count.is_none() {
                return false;
            }
        }

        if unhandled.updated_within
            && let Some(limit_hours) = max_age_hours
        {
            match entry
                .updated_at
                .as_deref()
                .and_then(estimate_age_hours_from_label)
            {
                Some(hours) if hours <= limit_hours => {}
                _ => return false,
            }
        }

        true
    });
}

fn resolve_tag_labels(schema: &SiteFilterSchema, ids: &[String]) -> Vec<String> {
    ids.iter()
        .map(|id| {
            schema
                .tags
                .iter()
                .find(|t| t.id.eq_ignore_ascii_case(id))
                .map_or_else(
                    || id.replace('-', " ").to_ascii_lowercase(),
                    |t| t.label.to_ascii_lowercase(),
                )
        })
        .collect()
}

/// Estimates age in hours from common Vietnamese, English, or `DD/MM/YYYY` / `YYYY-MM-DD` update strings.
#[must_use]
pub fn estimate_age_hours_from_label(raw: &str) -> Option<u64> {
    let lower = raw.trim().to_lowercase();
    if lower.is_empty() {
        return None;
    }
    if lower.contains("vừa xong")
        || lower.contains("just now")
        || lower.contains("hôm nay")
        || lower.contains("today")
    {
        return Some(1);
    }
    if lower.contains("hôm qua") || lower.contains("yesterday") {
        return Some(24);
    }
    if let Some(caps) = RELATIVE_TIME_RE.captures(&lower) {
        let num = caps.get(1)?.as_str().parse::<u64>().ok()?;
        let unit = caps.get(2)?.as_str();
        let hours = match unit {
            "giây" | "sec" | "second" | "phút" | "min" | "minute" => 0,
            "giờ" | "tiếng" | "hour" | "hr" => num,
            "ngày" | "day" => num.saturating_mul(24),
            "tuần" | "week" => num.saturating_mul(24 * 7),
            "tháng" | "month" => num.saturating_mul(24 * 30),
            "năm" | "year" => num.saturating_mul(24 * 365),
            _ => return None,
        };
        return Some(hours);
    }

    let now_days = unix_days_now();
    if let Some(caps) = DMY_DATE_RE.captures(&lower) {
        let d = caps.get(1)?.as_str().parse::<u32>().ok()?;
        let m = caps.get(2)?.as_str().parse::<u32>().ok()?;
        let mut y = caps.get(3)?.as_str().parse::<i32>().ok()?;
        if y < 100 {
            y += 2000;
        }
        let target_days = civil_to_days(y, m, d)?;
        return Some(now_days.saturating_sub(target_days).saturating_mul(24));
    }
    if let Some(caps) = YMD_DATE_RE.captures(&lower) {
        let y = caps.get(1)?.as_str().parse::<i32>().ok()?;
        let m = caps.get(2)?.as_str().parse::<u32>().ok()?;
        let d = caps.get(3)?.as_str().parse::<u32>().ok()?;
        let target_days = civil_to_days(y, m, d)?;
        return Some(now_days.saturating_sub(target_days).saturating_mul(24));
    }

    None
}

fn unix_days_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        / 86_400
}

/// Howard Hinnant's civil-from-epoch day formula (`1970-01-01` = day 0).
fn civil_to_days(year: i32, month: u32, day: u32) -> Option<u64> {
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) || year < 1970 {
        return None;
    }
    let y = i64::from(year) - i64::from(month <= 2);
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let m = i64::from(month);
    let doy = (153 * (m + if m > 2 { -3 } else { 9 }) + 2) / 5 + i64::from(day) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    u64::try_from(days.max(0)).ok()
}
