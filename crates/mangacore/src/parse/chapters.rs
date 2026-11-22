//! Generic heuristic for discovering and ordering chapter links on a manga page.

use std::collections::{BTreeMap, HashSet};
use std::sync::LazyLock;

use regex::Regex;
use scraper::{Html, Selector};
use url::Url;

use crate::model::ChapterRef;
use crate::parse::dom::{abs_url, clean_text, is_inside_chrome, url_pattern_key};

static LINK_SEL: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("a[href]").expect("static selector is valid"));

static CHAPTER_NUM_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)(?:chapter|chap|chương|chuong|ch\.?|episode|ep\.?|tập|tap)[\s\-_.:/#]*(\d+(?:[.,\-]\d+)?)",
    )
    .expect("static regex is valid")
});

/// Parses a chapter number (including fractional chapters like `12.5` or `12-5`) from text or URL.
pub fn extract_chapter_number(input: &str) -> Option<f32> {
    let caps = CHAPTER_NUM_RE.captures(input)?;
    let raw = caps.get(1)?.as_str().replace([',', '-'], ".");
    raw.parse::<f32>().ok()
}

/// Extracts the dominant chapter list from an arbitrary manga HTML page, ordered ascending.
pub fn extract_chapters(doc: &Html, base: &Url) -> Vec<ChapterRef> {
    let page_host = base.host_str().unwrap_or("").to_ascii_lowercase();
    let mut groups: BTreeMap<String, Vec<(usize, ChapterRef)>> = BTreeMap::new();

    for (dom_idx, link) in doc.select(&LINK_SEL).enumerate() {
        if is_inside_chrome(link) {
            continue;
        }
        let Some(href) = link.value().attr("href") else {
            continue;
        };
        let Some(url) = abs_url(base, href) else {
            continue;
        };
        if url.host_str().unwrap_or("").to_ascii_lowercase() != page_host {
            continue;
        }
        if url.path() == base.path() {
            continue;
        }

        let text = clean_text(&link.text().collect::<String>());
        let number = extract_chapter_number(&text).or_else(|| extract_chapter_number(url.path()));
        if number.is_none() {
            continue;
        }

        let name = if text.is_empty() {
            number.map_or_else(
                || url.path().to_owned(),
                |n| format!("Chương {}", format_chapter_num(n)),
            )
        } else {
            text
        };

        let pattern = url_pattern_key(&url);
        groups
            .entry(pattern)
            .or_default()
            .push((dom_idx, ChapterRef { url, name, number }));
    }

    let Some(mut best_group) = groups.into_values().max_by_key(Vec::len) else {
        return Vec::new();
    };

    // Deduplicate while preserving the first occurrence.
    let mut seen = HashSet::new();
    best_group.retain(|(_, ch)| seen.insert(ch.url.as_str().to_owned()));

    // Sort ascending by chapter number; break ties by DOM order.
    best_group.sort_by(|(idx_a, a), (idx_b, b)| match (a.number, b.number) {
        (Some(na), Some(nb)) => na.total_cmp(&nb).then_with(|| idx_a.cmp(idx_b)),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => idx_a.cmp(idx_b),
    });

    best_group.into_iter().map(|(_, ch)| ch).collect()
}

/// Formats a chapter number without redundant `.0` for integer values.
pub fn format_chapter_num(n: f32) -> String {
    if (n.fract()).abs() < f32::EPSILON {
        format!("{n:.0}")
    } else {
        format!("{n}")
    }
}
