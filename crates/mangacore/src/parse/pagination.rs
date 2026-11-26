//! Universal HTML pagination extractor and `{page}` URL template synthesizer.

use std::sync::LazyLock;

use regex::Regex;
use scraper::{Html, Selector};
use url::Url;

use crate::model::PaginationInfo;
use crate::parse::dom::{abs_url, clean_text};

static REL_NEXT_SEL: LazyLock<Selector> = LazyLock::new(|| {
    Selector::parse(r#"link[rel="next"][href], a[rel="next"][href]"#).expect("valid selector")
});
static REL_PREV_SEL: LazyLock<Selector> = LazyLock::new(|| {
    Selector::parse(r#"link[rel="prev"][href], a[rel="prev"][href]"#).expect("valid selector")
});
static ACTIVE_PAGE_SEL: LazyLock<Selector> = LazyLock::new(|| {
    Selector::parse(
        ".pagination .active, .pagination .current, .page-numbers.current, \
         .page_redirect .active, .wp-pagenavi .current, [aria-current='page']",
    )
    .expect("valid selector")
});
static PAGINATION_LINKS_SEL: LazyLock<Selector> = LazyLock::new(|| {
    Selector::parse(
        ".pagination a[href], .page_redirect a[href], .wp-pagenavi a[href], \
         a.page-numbers[href], .nav-links a[href], .pager a[href], ul.pager a[href], \
         nav[aria-label*='page' i] a[href], nav[aria-label*='pagination' i] a[href], \
         .paging a[href], .pagenav a[href], a[href*='/trang-'], a[href*='/page/'], \
         a[href*='?page='], a[href*='&page='], a[href*='?paged='], a[href*='&paged=']",
    )
    .expect("valid selector")
});

static PATH_TRANG_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(/trang-)(\d+)((?:\.html?)?(?:/|$))").expect("valid trang regex")
});
static PATH_PAGE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(/(?:page|p)/)(\d+)((?:\.html?)?(?:/|$))").expect("valid page path regex")
});
static DIGITS_ONLY_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\d{1,5}$").expect("valid digits regex"));

const PAGE_QUERY_KEYS: [&str; 4] = ["page", "paged", "trang", "p"];

/// Extracts page number from a URL's path (`/trang-2.html`, `/page/2/`) or query (`?page=2`).
#[must_use]
pub fn extract_page_number_from_url(url: &Url) -> Option<u32> {
    let path = url.path();
    if let Some(caps) = PATH_TRANG_RE.captures(path)
        && let Some(p) = caps.get(2).and_then(|m| m.as_str().parse::<u32>().ok())
    {
        return Some(p.max(1));
    }
    if let Some(caps) = PATH_PAGE_RE.captures(path)
        && let Some(p) = caps.get(2).and_then(|m| m.as_str().parse::<u32>().ok())
    {
        return Some(p.max(1));
    }
    for (k, v) in url.query_pairs() {
        if PAGE_QUERY_KEYS.iter().any(|pk| k.eq_ignore_ascii_case(pk))
            && let Ok(p) = v.trim().parse::<u32>()
        {
            return Some(p.max(1));
        }
    }
    None
}

/// Converts a concrete numbered pagination URL into a reusable `{page}` URL template.
#[must_use]
pub fn template_from_numbered_url(url: &Url) -> Option<String> {
    let path = url.path();
    if PATH_TRANG_RE.is_match(path) {
        let replaced_path = PATH_TRANG_RE
            .replace(path, "${1}__RTMAGA_PAGE__${3}")
            .into_owned();
        let mut copy = url.clone();
        copy.set_path(&replaced_path);
        return Some(copy.as_str().replace("__RTMAGA_PAGE__", "{page}"));
    }
    if PATH_PAGE_RE.is_match(path) {
        let replaced_path = PATH_PAGE_RE
            .replace(path, "${1}__RTMAGA_PAGE__${3}")
            .into_owned();
        let mut copy = url.clone();
        copy.set_path(&replaced_path);
        return Some(copy.as_str().replace("__RTMAGA_PAGE__", "{page}"));
    }

    let mut matched_key: Option<String> = None;
    let mut kept_pairs: Vec<(String, String)> = Vec::new();
    for (k, v) in url.query_pairs() {
        if matched_key.is_none()
            && PAGE_QUERY_KEYS.iter().any(|pk| k.eq_ignore_ascii_case(pk))
            && v.trim().parse::<u32>().is_ok()
        {
            matched_key = Some(k.into_owned());
        } else {
            kept_pairs.push((k.into_owned(), v.into_owned()));
        }
    }
    if let Some(key) = matched_key {
        let mut copy = url.clone();
        {
            let mut qp = copy.query_pairs_mut();
            qp.clear();
            for (k, v) in &kept_pairs {
                qp.append_pair(k, v);
            }
            qp.append_pair(&key, "__RTMAGA_PAGE__");
        }
        return Some(copy.as_str().replace("__RTMAGA_PAGE__", "{page}"));
    }
    None
}

/// Substitutes `page` into a `{page}` URL template produced by [`template_from_numbered_url`].
#[must_use]
pub fn apply_page_to_template(template: &str, page: u32) -> Option<Url> {
    if !template.contains("{page}") {
        return None;
    }
    let concrete = template.replace("{page}", &page.max(1).to_string());
    Url::parse(&concrete).ok()
}

/// Extracts [`PaginationInfo`] (current page, total pages, prev/next URLs, and `{page}` jump template)
/// from arbitrary manga catalog or search HTML.
#[must_use]
#[expect(
    clippy::too_many_lines,
    clippy::fn_params_excessive_bools,
    reason = "single-pass HTML pagination extractor with has_entries guard"
)]
pub fn extract_universal_pagination(
    doc: &Html,
    base: &Url,
    page_url: &Url,
    has_entries: bool,
) -> PaginationInfo {
    let mut current_page = extract_page_number_from_url(page_url).unwrap_or(1);
    if current_page == 1 {
        for el in doc.select(&ACTIVE_PAGE_SEL) {
            let txt = clean_text(&el.text().collect::<String>());
            if DIGITS_ONLY_RE.is_match(&txt)
                && let Ok(p) = txt.parse::<u32>()
                && p >= 1
            {
                current_page = p;
                break;
            }
        }
    }

    let mut max_page = current_page;
    let mut prev_url = doc
        .select(&REL_PREV_SEL)
        .filter_map(|el| el.value().attr("href").and_then(|h| abs_url(base, h)))
        .find(|u| u != page_url);
    let mut next_url = doc
        .select(&REL_NEXT_SEL)
        .filter_map(|el| el.value().attr("href").and_then(|h| abs_url(base, h)))
        .find(|u| u != page_url);
    let mut template = template_from_numbered_url(page_url);

    let page_host = page_url.host_str().map(str::to_ascii_lowercase);

    for a in doc.select(&PAGINATION_LINKS_SEL) {
        let Some(href) = a.value().attr("href") else {
            continue;
        };
        let trimmed_href = href.trim();
        if trimmed_href.is_empty()
            || trimmed_href.starts_with('#')
            || trimmed_href.starts_with("javascript:")
        {
            continue;
        }
        let Some(resolved) = abs_url(base, trimmed_href) else {
            continue;
        };
        if let (Some(ph), Some(rh)) = (page_host.as_deref(), resolved.host_str())
            && !rh.eq_ignore_ascii_case(ph)
        {
            continue;
        }

        let label = clean_text(&a.text().collect::<String>()).to_ascii_lowercase();
        let rel = a.value().attr("rel").unwrap_or("").to_ascii_lowercase();
        let cls = a.value().attr("class").unwrap_or("").to_ascii_lowercase();
        let title_attr = a.value().attr("title").unwrap_or("").to_ascii_lowercase();

        let num_from_url = extract_page_number_from_url(&resolved);
        let num_from_label = DIGITS_ONLY_RE
            .is_match(&label)
            .then(|| label.parse::<u32>().ok())
            .flatten();
        let detected_page = num_from_url.or(num_from_label);

        if template.is_none() && num_from_url.is_some() {
            template = template_from_numbered_url(&resolved);
        }

        if let Some(p) = detected_page {
            if p > max_page && p <= 50_000 {
                max_page = p;
            }
            if prev_url.is_none() && current_page > 1 && p == current_page - 1 {
                prev_url = Some(resolved.clone());
            }
            if next_url.is_none() && p == current_page + 1 {
                next_url = Some(resolved.clone());
            }
        }

        if next_url.is_none()
            && resolved != *page_url
            && (rel.contains("next")
                || cls.contains("next")
                || title_attr.contains("next")
                || title_attr.contains("trang sau")
                || matches!(
                    label.as_str(),
                    "next" | "next ›" | "next »" | "›" | ">" | "trang sau" | "sau" | "tiếp"
                ))
        {
            next_url = Some(resolved.clone());
            if template.is_none() {
                template = template_from_numbered_url(&resolved);
            }
        } else if prev_url.is_none()
            && current_page > 1
            && resolved != *page_url
            && (rel.contains("prev")
                || cls.contains("prev")
                || title_attr.contains("prev")
                || title_attr.contains("trang trước")
                || matches!(
                    label.as_str(),
                    "prev" | "previous" | "‹ prev" | "« prev" | "‹" | "<" | "trang trước" | "trước"
                ))
        {
            prev_url = Some(resolved);
        }
    }

    if let Some(ref tmpl) = template {
        if prev_url.is_none() && current_page > 1 {
            prev_url = apply_page_to_template(tmpl, current_page - 1);
        }
        if next_url.is_none() && has_entries && max_page > current_page {
            next_url = apply_page_to_template(tmpl, current_page + 1);
        }
    }

    if let Some(ref next) = next_url
        && let Some(np) = extract_page_number_from_url(next)
        && np > max_page
    {
        max_page = np;
    }

    PaginationInfo {
        current_page,
        total_pages: (max_page > 1).then_some(max_page),
        prev_url,
        next_url,
        page_url_template: template,
    }
}
