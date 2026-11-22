//! Generic heuristic for extracting ordered manga page images from a chapter HTML page.

use std::collections::{BTreeMap, HashSet};
use std::sync::LazyLock;

use regex::Regex;
use scraper::{ElementRef, Html, Selector};
use url::Url;

use crate::model::PageImage;
use crate::parse::dom::{
    abs_url, image_candidates, is_declared_too_small, is_ignored_image_url, is_inside_chrome,
    url_pattern_key,
};

/// Minimum declared dimension (in pixels) for an image to be considered a manga page.
const MIN_PAGE_DIMENSION_PX: u32 = 200;

/// Minimum number of images required before falling back to `<script>` extraction.
const MIN_DOM_IMAGES_BEFORE_SCRIPT_FALLBACK: usize = 3;

static IMG_SEL: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("img").expect("static selector is valid"));

static SCRIPT_SEL: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("script").expect("static selector is valid"));

static POSITIVE_CONTAINER_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(chapter|reading|reader|page[-_]?chapter|content|viewer|manga[-_]?page)")
        .expect("static regex is valid")
});

static NEGATIVE_CONTAINER_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(ads|banner|related|recommend|comment|sidebar|footer|header|nav|popup|modal)")
        .expect("static regex is valid")
});

static SCRIPT_IMAGE_URL_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"["'](https?:\\?/\\?/[^"'\s<>]+\.(?:jpe?g|png|webp|avif)(?:\?[^"'\s<>]*)?)["']"#)
        .expect("static regex is valid")
});

#[derive(Debug)]
struct CandidateImg {
    primary: Url,
    fallbacks: Vec<Url>,
    pattern: String,
    dom_order: usize,
}

/// Extracts ordered manga page images using DOM container scoring, with a `<script>` fallback.
pub fn extract_chapter_images(doc: &Html, page_url: &Url, base: &Url) -> Vec<PageImage> {
    let dom_images = extract_from_dom(doc, page_url, base);
    if dom_images.len() >= MIN_DOM_IMAGES_BEFORE_SCRIPT_FALLBACK {
        return dom_images;
    }

    let script_images = extract_from_scripts(doc, page_url, base);
    if script_images.len() > dom_images.len() {
        script_images
    } else {
        dom_images
    }
}

fn extract_from_dom(doc: &Html, page_url: &Url, base: &Url) -> Vec<PageImage> {
    // Group valid images by nearby ancestor containers (parent and grandparent) so wrappers like
    // `<div class="page-chapter"><img /></div>` aggregate cleanly under their list container.
    let mut containers: BTreeMap<_, Vec<CandidateImg>> = BTreeMap::new();
    let mut container_bonus: BTreeMap<_, i64> = BTreeMap::new();

    for (dom_order, img) in doc.select(&IMG_SEL).enumerate() {
        if is_inside_chrome(img) || is_declared_too_small(img, MIN_PAGE_DIMENSION_PX) {
            continue;
        }
        let candidates: Vec<Url> = image_candidates(img, base)
            .into_iter()
            .filter(|u| !is_ignored_image_url(u))
            .collect();
        let Some((primary, rest)) = candidates.split_first() else {
            continue;
        };

        let pattern = url_pattern_key(primary);

        // Register under up to 3 ancestor levels above the <img>.
        for ancestor in img.ancestors().filter_map(ElementRef::wrap).take(3) {
            if matches!(ancestor.value().name(), "body" | "html") {
                continue;
            }
            let id = ancestor.id();
            container_bonus
                .entry(id)
                .or_insert_with(|| element_semantic_bonus(ancestor));
            containers.entry(id).or_default().push(CandidateImg {
                primary: primary.clone(),
                fallbacks: rest.to_vec(),
                pattern: pattern.clone(),
                dom_order,
            });
        }
    }

    let mut best_score = i64::MIN;
    let mut best_images: Vec<CandidateImg> = Vec::new();

    for (node_id, mut imgs) in containers {
        let bonus = container_bonus.get(&node_id).copied().unwrap_or(0);
        let score = score_container(&imgs, bonus);
        if score > best_score {
            best_score = score;
            imgs.sort_by_key(|c| c.dom_order);
            best_images = imgs;
        }
    }

    let mut seen = HashSet::new();
    best_images
        .into_iter()
        .filter(|c| seen.insert(c.primary.as_str().to_owned()))
        .map(|c| PageImage {
            url: c.primary,
            fallbacks: c.fallbacks,
            referer: page_url.clone(),
        })
        .collect()
}

fn score_container(imgs: &[CandidateImg], semantic_bonus: i64) -> i64 {
    let mut seen = HashSet::new();
    let unique: Vec<&CandidateImg> = imgs
        .iter()
        .filter(|c| seen.insert(c.primary.as_str()))
        .collect();

    let count = i64::try_from(unique.len()).unwrap_or(i64::MAX);
    let longest_run = longest_same_pattern_run(&unique);
    count + (2 * longest_run) + semantic_bonus
}

fn longest_same_pattern_run(imgs: &[&CandidateImg]) -> i64 {
    let mut max_run: i64 = 0;
    let mut current_run: i64 = 0;
    let mut prev_pattern: Option<&str> = None;

    for img in imgs {
        if prev_pattern == Some(img.pattern.as_str()) {
            current_run += 1;
        } else {
            current_run = 1;
            prev_pattern = Some(img.pattern.as_str());
        }
        if current_run > max_run {
            max_run = current_run;
        }
    }

    max_run
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

fn extract_from_scripts(doc: &Html, page_url: &Url, base: &Url) -> Vec<PageImage> {
    let mut groups: BTreeMap<String, Vec<Url>> = BTreeMap::new();

    for script in doc.select(&SCRIPT_SEL) {
        let body = script.text().collect::<String>();
        for caps in SCRIPT_IMAGE_URL_RE.captures_iter(&body) {
            let raw = caps[1].replace("\\/", "/");
            if let Some(url) = abs_url(base, &raw)
                && !is_ignored_image_url(&url)
            {
                let key = url_pattern_key(&url);
                groups.entry(key).or_default().push(url);
            }
        }
    }

    let Some(best_group) = groups.into_values().max_by_key(Vec::len) else {
        return Vec::new();
    };

    let mut seen = HashSet::new();
    best_group
        .into_iter()
        .filter(|u| seen.insert(u.as_str().to_owned()))
        .map(|url| PageImage {
            url,
            fallbacks: Vec::new(),
            referer: page_url.clone(),
        })
        .collect()
}
