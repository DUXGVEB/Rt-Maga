//! Strategy A: Upgraded native HTML comic extraction supporting `<img>`, `<picture><source>`,
//! `<amp-img>`, inline CSS `background-image`, lazy-loading attributes, and confidence-aware
//! page-count validation.

use std::collections::{BTreeMap, HashSet};
use std::sync::LazyLock;

use regex::Regex;
use scraper::{ElementRef, Html, Selector};
use url::Url;

use crate::model::{Chapter, PageImage};
use crate::parse::chapters::{extract_chapter_number, format_chapter_num};
use crate::parse::dom::{
    is_declared_too_small, is_ignored_image_url, is_inside_chrome, url_pattern_key,
};
use crate::parse::meta::extract_title;
use crate::parse::nav::{extract_manga_url_from_chapter, extract_prev_next};
use crate::resolver::{
    CandidateOrigin, RawImageCandidate, extract_css_background_urls, parse_srcset_ranked,
    resolve_image_url, resolve_page_images,
};

const MIN_PAGE_DIMENSION_PX: u32 = 160;

/// Ordered lazy-load & standard image attributes checked from highest priority to lowest.
const ADAPTIVE_IMAGE_ATTRS: &[&str] = &[
    "data-original",
    "data-src",
    "data-lazy-src",
    "data-pagespeed-lazy-src",
    "data-echo",
    "data-cfsrc",
    "data-actualsrc",
    "data-lazy",
    "data-img",
    "data-image",
    "data-full-src",
    "data-cdn",
    "data-url",
    "data-sv1",
    "data-sv2",
    "data-fb",
    "src",
];

static MEDIA_EL_SEL: LazyLock<Selector> = LazyLock::new(|| {
    Selector::parse("img, amp-img, [style*='background-image'], [style*='background:'], [data-bg]")
        .expect("valid media selector")
});

static SOURCE_SEL: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("source").expect("valid source selector"));

static POSITIVE_CONTAINER_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(chapter|reading|reader|page[-_]?chapter|page[-_]?break|content|viewer|manga[-_]?page)")
        .expect("valid positive container regex")
});

static NEGATIVE_CONTAINER_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)(ads|banner|related|recommend|comment|sidebar|footer|header|nav|popup|modal|avatar)",
    )
    .expect("valid negative container regex")
});

/// Minimum chapter page requirement adjusted for page-type confidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum HtmlMinimumPolicy {
    /// Require at least 3 pages (safest for ambiguous/generic pages).
    StandardThree,
    /// Allow 1-2 page oneshots or single long-strip chapters when chapter URL/DOM context is confident.
    ConfidentChapterOneOrMore,
}

impl HtmlMinimumPolicy {
    const fn min_pages(self) -> usize {
        match self {
            Self::StandardThree => 3,
            Self::ConfidentChapterOneOrMore => 1,
        }
    }
}

/// Extracted HTML chapter payload plus diagnostic candidate metrics.
#[derive(Debug, Clone)]
pub struct HtmlExtractionOutput {
    /// Extracted [`Chapter`] if minimum page policy passed.
    pub chapter: Option<Chapter>,
    /// Extracted ordered [`PageImage`] list (may be below minimum policy).
    pub pages: Vec<PageImage>,
    /// Raw candidate count before ad/UI filtering and deduplication.
    pub raw_candidates: usize,
    /// Number of `blob:` URLs detected during scan.
    pub blob_urls_seen: usize,
}

#[derive(Debug)]
struct ScoredDomNode {
    candidate: RawImageCandidate,
    primary_url: Url,
    pattern: String,
    dom_order: usize,
}

/// Collects image candidates from `<img>`, `<picture><source>`, `<amp-img>`, lazy attributes,
/// and inline CSS `background-image` declarations, then resolves and deduplicates them.
#[must_use]
pub fn extract_chapter_html_adaptive(
    doc: &Html,
    page_url: &Url,
    base: &Url,
    min_policy: HtmlMinimumPolicy,
) -> HtmlExtractionOutput {
    let mut containers: BTreeMap<_, Vec<ScoredDomNode>> = BTreeMap::new();
    let mut container_bonus: BTreeMap<_, i64> = BTreeMap::new();
    let mut all_raw_candidates = 0_usize;
    let mut blob_seen = 0_usize;

    for (dom_order, el) in doc.select(&MEDIA_EL_SEL).enumerate() {
        if is_inside_chrome(el) || is_declared_too_small(el, MIN_PAGE_DIMENSION_PX) {
            continue;
        }
        let Some((cand, resolved_primary)) =
            extract_element_candidate(el, page_url, base, &mut blob_seen)
        else {
            continue;
        };
        all_raw_candidates += 1;
        let pattern = url_pattern_key(&resolved_primary);

        for ancestor in el.ancestors().filter_map(ElementRef::wrap).take(3) {
            if matches!(ancestor.value().name(), "body" | "html") {
                continue;
            }
            let id = ancestor.id();
            container_bonus
                .entry(id)
                .or_insert_with(|| element_semantic_bonus(ancestor));
            containers.entry(id).or_default().push(ScoredDomNode {
                candidate: cand.clone(),
                primary_url: resolved_primary.clone(),
                pattern: pattern.clone(),
                dom_order,
            });
        }
    }

    let best_candidates = select_winning_container(containers, &container_bonus);
    let resolved = resolve_page_images(best_candidates, base);
    let blob_total = blob_seen + resolved.blob_urls_seen;
    let raw_total = all_raw_candidates.max(resolved.raw_candidates);

    let chapter = if resolved.pages.len() >= min_policy.min_pages() {
        let nav = extract_prev_next(doc, page_url, base);
        let manga_url = extract_manga_url_from_chapter(doc, page_url, base);
        let name = extract_title(doc).unwrap_or_else(|| {
            extract_chapter_number(page_url.path()).map_or_else(
                || "Chapter".to_owned(),
                |n| format!("Chapter {}", format_chapter_num(n)),
            )
        });
        Some(Chapter {
            url: page_url.clone(),
            name,
            images: resolved.pages.clone(),
            prev: nav.prev,
            next: nav.next,
            manga_url,
        })
    } else {
        None
    };

    HtmlExtractionOutput {
        chapter,
        pages: resolved.pages,
        raw_candidates: raw_total,
        blob_urls_seen: blob_total,
    }
}

fn extract_element_candidate(
    el: ElementRef<'_>,
    page_url: &Url,
    base: &Url,
    blob_seen: &mut usize,
) -> Option<(RawImageCandidate, Url)> {
    let tag = el.value().name();
    let mut urls: Vec<(Url, Option<Url>)> = Vec::new();
    let mut origin = CandidateOrigin::HtmlImg;

    if tag == "img" || tag == "amp-img" {
        if let Some(parent) = el.parent().and_then(ElementRef::wrap)
            && parent.value().name() == "picture"
        {
            for src_el in parent.select(&SOURCE_SEL) {
                if let Some(srcset) = src_el
                    .value()
                    .attr("srcset")
                    .or_else(|| src_el.value().attr("data-srcset"))
                {
                    for u in parse_srcset_ranked(srcset, base) {
                        origin = CandidateOrigin::HtmlPictureSource;
                        push_unique(&mut urls, (u, None));
                    }
                }
            }
        }

        for attr in ADAPTIVE_IMAGE_ATTRS {
            if let Some(raw_val) = el.value().attr(attr) {
                if raw_val.trim().starts_with("blob:") {
                    *blob_seen += 1;
                    continue;
                }
                if let Some(pair) = resolve_image_url(base, raw_val) {
                    push_unique(&mut urls, pair);
                }
            }
        }

        if let Some(srcset) = el
            .value()
            .attr("srcset")
            .or_else(|| el.value().attr("data-srcset"))
        {
            for u in parse_srcset_ranked(srcset, base) {
                push_unique(&mut urls, (u, None));
            }
        }
    } else {
        origin = CandidateOrigin::CssBackground;
        if let Some(bg) = el.value().attr("data-bg")
            && let Some(pair) = resolve_image_url(base, bg)
        {
            push_unique(&mut urls, pair);
        }
        if let Some(style) = el.value().attr("style") {
            for u in extract_css_background_urls(style, base) {
                push_unique(&mut urls, (u, None));
            }
        }
    }

    let mut flat: Vec<Url> = Vec::new();
    for (u, fb) in urls {
        if !is_ignored_image_url(&u) && !flat.contains(&u) {
            flat.push(u);
        }
        if let Some(alt) = fb
            && !is_ignored_image_url(&alt)
            && !flat.contains(&alt)
        {
            flat.push(alt);
        }
    }

    let (primary, rest) = flat.split_first()?;
    let fallbacks = rest.iter().map(|u| u.as_str().to_owned()).collect();
    Some((
        RawImageCandidate {
            raw_url: primary.as_str().to_owned(),
            fallbacks,
            referer: page_url.clone(),
            origin,
        },
        primary.clone(),
    ))
}

fn push_unique(list: &mut Vec<(Url, Option<Url>)>, item: (Url, Option<Url>)) {
    if !list.iter().any(|(existing, _)| existing == &item.0) {
        list.push(item);
    }
}

fn select_winning_container<K: Ord>(
    containers: BTreeMap<K, Vec<ScoredDomNode>>,
    container_bonus: &BTreeMap<K, i64>,
) -> Vec<RawImageCandidate> {
    let mut best_score = i64::MIN;
    let mut best_nodes: Vec<ScoredDomNode> = Vec::new();

    for (node_id, mut nodes) in containers {
        let bonus = container_bonus.get(&node_id).copied().unwrap_or(0);
        let score = score_container(&nodes, bonus);
        if score > best_score {
            best_score = score;
            nodes.sort_by_key(|n| n.dom_order);
            best_nodes = nodes;
        }
    }

    best_nodes.into_iter().map(|n| n.candidate).collect()
}

fn score_container(nodes: &[ScoredDomNode], semantic_bonus: i64) -> i64 {
    let mut seen = HashSet::new();
    let unique: Vec<&ScoredDomNode> = nodes
        .iter()
        .filter(|n| seen.insert(n.primary_url.as_str()))
        .collect();

    let count = i64::try_from(unique.len()).unwrap_or(i64::MAX);
    let mut max_run = 0_i64;
    let mut current_run = 0_i64;
    let mut prev: Option<&str> = None;
    for node in &unique {
        if prev == Some(node.pattern.as_str()) {
            current_run += 1;
        } else {
            current_run = 1;
            prev = Some(node.pattern.as_str());
        }
        max_run = max_run.max(current_run);
    }
    count + (2 * max_run) + semantic_bonus
}

fn element_semantic_bonus(el: ElementRef<'_>) -> i64 {
    let mut attr_buf = String::new();
    if let Some(id) = el.value().attr("id") {
        attr_buf.push_str(id);
        attr_buf.push(' ');
    }
    if let Some(class) = el.value().attr("class") {
        attr_buf.push_str(class);
    }
    let mut bonus = 0;
    if POSITIVE_CONTAINER_RE.is_match(&attr_buf) {
        bonus += 10;
    }
    if NEGATIVE_CONTAINER_RE.is_match(&attr_buf) {
        bonus -= 20;
    }
    bonus
}
