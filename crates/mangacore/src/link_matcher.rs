//! Story link classification, deep-link parsing, domain mirror rewriting, and two-phase
//! exact series matching (`search + Levenshtein <= 20%` followed by direct seed resolution).

use url::Url;

use crate::cms::{CmsKind, browse_or_search_cms, fetch_madara_chapters_ajax};
use crate::error::ScrapeError;
use crate::http::{HttpClient, normalize_url};
use crate::model::Resolved;
use crate::rule_pack::{CustomRuleSpec, execute_rule_manga};
use crate::source::generic::GenericSource;

/// Classification of any user-entered search box or deep-link string.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum UserQueryKind {
    /// Custom application deep link (`rtmaga://manga?...`).
    AppDeepLink {
        /// Optional local SQLite manga primary key.
        manga_id: Option<i64>,
        /// Optional saved source key (`custom_...`).
        source_key: Option<String>,
        /// Optional target manga/chapter URL inside the source.
        url: Option<String>,
        /// Optional manga title hint used for two-phase resolution.
        title: Option<String>,
    },
    /// Public HTTP(S) URL or normalized bare domain link.
    WebUrl(Url),
    /// Plain-text manga search query across saved sources.
    KeywordSearch(String),
}

/// Classifies any raw input from the search bar or OS deep-link handler into an action intent.
#[must_use]
pub fn classify_user_input(raw: &str) -> UserQueryKind {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return UserQueryKind::KeywordSearch(String::new());
    }

    if trimmed.to_ascii_lowercase().starts_with("rtmaga://")
        && let Ok(parsed) = Url::parse(trimmed)
    {
        let mut manga_id = None;
        let mut source_key = None;
        let mut url = None;
        let mut title = None;
        for (k, v) in parsed.query_pairs() {
            let val = v.trim();
            if val.is_empty() {
                continue;
            }
            match k.to_ascii_lowercase().as_str() {
                "id" => manga_id = val.parse::<i64>().ok(),
                "source" => source_key = Some(val.to_owned()),
                "url" => url = Some(val.to_owned()),
                "title" => title = Some(val.to_owned()),
                _ => {}
            }
        }
        return UserQueryKind::AppDeepLink {
            manga_id,
            source_key,
            url,
            title,
        };
    }

    let has_scheme = trimmed.to_ascii_lowercase().starts_with("http://")
        || trimmed.to_ascii_lowercase().starts_with("https://");
    let looks_like_bare_domain = !trimmed.chars().any(char::is_whitespace)
        && trimmed.contains('.')
        && !trimmed.starts_with('.')
        && !trimmed.ends_with('.');

    if (has_scheme || looks_like_bare_domain)
        && let Ok(u) = normalize_url(trimmed)
    {
        return UserQueryKind::WebUrl(u);
    }

    UserQueryKind::KeywordSearch(trimmed.to_owned())
}

/// Rewrites `target`'s scheme, host, and port to `mirror_base` while keeping path and query intact.
#[must_use]
pub fn rewrite_domain_override(target: &Url, mirror_base: &Url) -> Url {
    let mut out = target.clone();
    let _ = out.set_scheme(mirror_base.scheme());
    if let Some(host) = mirror_base.host_str() {
        let _ = out.set_host(Some(host));
    }
    let _ = out.set_port(mirror_base.port());
    out
}

/// Returns `true` when `a` and `b` match within a normalized Levenshtein edit distance ratio
/// (`<= max_ratio`, e.g. `0.20` for 20% tolerance).
#[must_use]
pub fn within_edit_ratio(a: &str, b: &str, max_ratio: f32) -> bool {
    let na = normalize_title_for_compare(a);
    let nb = normalize_title_for_compare(b);
    if na.is_empty() || nb.is_empty() {
        return false;
    }
    if na == nb {
        return true;
    }
    let chars_a: Vec<char> = na.chars().collect();
    let chars_b: Vec<char> = nb.chars().collect();
    let max_len = chars_a.len().max(chars_b.len()).max(1);
    let dist = levenshtein_chars(&chars_a, &chars_b);
    let dist_f = f32::from(u16::try_from(dist).unwrap_or(u16::MAX));
    let max_f = f32::from(u16::try_from(max_len).unwrap_or(u16::MAX));
    (dist_f / max_f) <= max_ratio
}

/// Executes two-phase exact resolution against a saved source:
/// - **Phase 1**: Searches by `title_hint` (if provided) and prefers exact relative path match,
///   then closest title match within 20% Levenshtein distance.
/// - **Phase 2**: Falls back to direct seed resolution against the rewritten mirror URL,
///   using rule pack / CMS / Madara AJAX / generic extraction.
///
/// # Errors
///
/// Returns [`ScrapeError`] if both phases fail to resolve a manga or chapter.
#[expect(
    clippy::too_many_arguments,
    reason = "orchestrates saved source mirror override and two-phase resolution"
)]
pub async fn resolve_exact_from_source(
    http: &HttpClient,
    kind: CmsKind,
    mirror_base: &Url,
    parser_ref: Option<&str>,
    search_template: Option<&str>,
    target_url: &Url,
    title_hint: Option<&str>,
    custom_rules: &[CustomRuleSpec],
) -> Result<Resolved, ScrapeError> {
    let overridden_target = rewrite_domain_override(target_url, mirror_base);
    let target_rel = relative_path_key(&overridden_target);

    // Phase 1: Title search + path/Levenshtein match
    if let Some(title) = title_hint.map(str::trim).filter(|t| !t.is_empty())
        && let Ok(catalog) = browse_or_search_cms(
            http,
            kind,
            mirror_base,
            parser_ref,
            search_template,
            Some(title),
            1,
            custom_rules,
        )
        .await
    {
        let exact_path = catalog
            .entries
            .iter()
            .find(|e| relative_path_key(&e.url) == target_rel);
        let fuzzy_title = catalog
            .entries
            .iter()
            .find(|e| within_edit_ratio(&e.title, title, 0.20));
        if let Some(matched) = exact_path.or(fuzzy_title)
            && let Ok(res) = resolve_seed_url(
                http,
                kind,
                parser_ref,
                &rewrite_domain_override(&matched.url, mirror_base),
                custom_rules,
            )
            .await
        {
            return Ok(res);
        }
    }

    // Phase 2: Direct seed resolution on mirror URL
    resolve_seed_url(http, kind, parser_ref, &overridden_target, custom_rules).await
}

async fn resolve_seed_url(
    http: &HttpClient,
    kind: CmsKind,
    parser_ref: Option<&str>,
    url: &Url,
    custom_rules: &[CustomRuleSpec],
) -> Result<Resolved, ScrapeError> {
    if kind == CmsKind::CustomRule
        && let Some(rule_id) = parser_ref
        && let Some(spec) = custom_rules.iter().find(|r| r.id == rule_id)
        && let Ok(manga) = execute_rule_manga(http, url, spec).await
        && !manga.chapters.is_empty()
    {
        return Ok(Resolved::Manga(Box::new(manga)));
    }

    let page = http.fetch_html(url).await?;
    match GenericSource::resolve_html(http, &page.html, &page.final_url).await {
        Ok(mut resolved) => {
            if let Resolved::Manga(ref mut manga) = resolved
                && manga.chapters.is_empty()
                && kind == CmsKind::Madara
            {
                manga.chapters =
                    fetch_madara_chapters_ajax(http, &page.final_url, &page.html).await;
            }
            Ok(resolved)
        }
        Err(err) => {
            if kind == CmsKind::Madara {
                let ajax_chaps =
                    fetch_madara_chapters_ajax(http, &page.final_url, &page.html).await;
                if !ajax_chaps.is_empty() {
                    let mut manga = GenericSource::parse_manga_html(&page.html, &page.final_url)
                        .unwrap_or_else(|_| {
                            let host = page.final_url.host_str().unwrap_or("unknown").to_owned();
                            crate::model::Manga::minimal(
                                page.final_url.clone(),
                                crate::model::SiteId::generic(host),
                                page.final_url.path().to_owned(),
                                None,
                                ajax_chaps.clone(),
                            )
                        });
                    manga.chapters = ajax_chaps;
                    return Ok(Resolved::Manga(Box::new(manga)));
                }
            }
            Err(err)
        }
    }
}

fn relative_path_key(url: &Url) -> String {
    let mut p = url.path().trim_end_matches('/').to_ascii_lowercase();
    if let Some(q) = url.query() {
        p.push('?');
        p.push_str(&q.to_ascii_lowercase());
    }
    p
}

fn normalize_title_for_compare(raw: &str) -> String {
    raw.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase()
}

fn levenshtein_chars(a: &[char], b: &[char]) -> usize {
    let m = a.len();
    let n = b.len();
    if m == 0 {
        return n;
    }
    if n == 0 {
        return m;
    }
    let mut prev: Vec<usize> = (0..=n).collect();
    let mut curr = vec![0_usize; n + 1];
    for (i, &ca) in a.iter().enumerate() {
        curr[0] = i + 1;
        for (j, &cb) in b.iter().enumerate() {
            let cost = usize::from(ca != cb);
            curr[j + 1] = (curr[j] + 1).min(prev[j + 1] + 1).min(prev[j] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    prev[n]
}
