//! Strategy C: Structured embedded data & JSON/JSON-LD/SPA hydration extraction.

use std::sync::LazyLock;

use regex::Regex;
use scraper::{Html, Selector};
use serde_json::Value;
use url::Url;

use crate::model::{Chapter, ChapterRef, Manga, SiteId};
use crate::parse::chapters::{extract_chapter_number, format_chapter_num};
use crate::parse::dom::{abs_url, clean_text};
use crate::parse::meta::extract_title;
use crate::parse::nav::{extract_manga_url_from_chapter, extract_prev_next};
use crate::resolver::{CandidateOrigin, RawImageCandidate, resolve_page_images};

static SCRIPT_SEL: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("script").expect("valid script selector"));

static TS_READER_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)ts_reader\.run\s*\(").expect("valid ts_reader regex"));

static MADARA_PRELOAD_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)var\s+chapter_preloaded_images\s*=").expect("valid madara regex")
});

static EMBEDDED_ARRAY_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r#"(?i)(?:"?(?:chapterImages|chapter_images|pageList|page_list|imgList|img_list|imageUrls|__PAGES__|__CHAPTER_IMAGES__|images|pages)"?\s*[:=]\s*)\["#,
    )
    .expect("valid embedded array regex")
});

/// Structured chapter extraction result returned by Strategy C.
#[derive(Debug, Clone)]
pub struct StructuredChapterExtraction {
    /// Constructed [`Chapter`] with ordered pages and resolved fallbacks/navigation.
    pub chapter: Chapter,
    /// Raw candidate URLs inspected before deduplication and ad/UI filtering.
    pub raw_candidates: usize,
    /// Extraction confidence (`0.0..=1.0`).
    pub confidence: f32,
    /// Structured payload family matched (`"ts_reader"`, `"next_data"`, `"json_ld"`, etc.).
    pub source_tag: &'static str,
}

/// Checks whether a fetched API/data response uses a binary, protobuf, or non-JSON format that
/// cannot be interpreted by JSON/HTML parsers.
#[must_use]
pub fn is_unsupported_api_payload(content_type: Option<&str>, body: &str) -> bool {
    if let Some(ct) = content_type {
        let lower = ct.to_ascii_lowercase();
        if lower.contains("protobuf")
            || lower.contains("grpc")
            || lower.contains("msgpack")
            || lower == "application/octet-stream"
        {
            return true;
        }
        if lower.contains("json") && !body.trim_start().starts_with(['{', '[']) {
            return true;
        }
    }
    let nul_or_ctrl = body
        .bytes()
        .take(256)
        .filter(|b| *b < 0x09 || (*b > 0x0D && *b < 0x20))
        .count();
    nul_or_ctrl >= 4
}

/// Attempts Strategy C chapter extraction from embedded JSON (`ts_reader.run`, Madara
/// `chapter_preloaded_images`, `#__NEXT_DATA__`, `application/ld+json`, or named JSON arrays).
#[must_use]
pub fn extract_structured_chapter(
    doc: &Html,
    html: &str,
    page_url: &Url,
    base: &Url,
) -> Option<StructuredChapterExtraction> {
    if let Some(res) = try_ts_reader(doc, html, page_url, base) {
        return Some(res);
    }
    if let Some(res) = try_madara_preloaded(doc, html, page_url, base) {
        return Some(res);
    }
    if let Some(res) = try_next_data_chapter(doc, page_url, base) {
        return Some(res);
    }
    if let Some(res) = try_json_ld_chapter(doc, page_url, base) {
        return Some(res);
    }
    try_embedded_script_arrays(doc, page_url, base)
}

/// Attempts Strategy C series detail & metadata extraction from `application/ld+json` or
/// `#__NEXT_DATA__` when plain HTML selectors lacked structured chapter lists.
#[must_use]
pub fn extract_structured_manga(doc: &Html, page_url: &Url, base: &Url) -> Option<Manga> {
    for script in doc.select(&SCRIPT_SEL) {
        let is_next = script.value().attr("id") == Some("__NEXT_DATA__");
        let is_ld = script.value().attr("type") == Some("application/ld+json");
        if !is_next && !is_ld {
            continue;
        }
        let body = script.text().collect::<String>();
        let Ok(val) = serde_json::from_str::<Value>(&body) else {
            continue;
        };
        if is_next && let Some(manga) = parse_next_data_manga(&val, page_url, base) {
            return Some(manga);
        }
        if is_ld && let Some(manga) = parse_json_ld_manga(doc, &val, page_url, base) {
            return Some(manga);
        }
    }
    None
}

fn try_ts_reader(
    doc: &Html,
    html: &str,
    page_url: &Url,
    base: &Url,
) -> Option<StructuredChapterExtraction> {
    let m = TS_READER_RE.find(html)?;
    let obj_slice = slice_balanced_json(&html[m.end()..], b'{', b'}')?;
    let val: Value = serde_json::from_str(obj_slice).ok()?;

    let sources = val.get("sources").and_then(Value::as_array)?;
    let mut primary_urls: Vec<String> = Vec::new();
    let mut per_page_fallbacks: Vec<Vec<String>> = Vec::new();

    for src in sources {
        let Some(imgs) = src.get("images").and_then(Value::as_array) else {
            continue;
        };
        let urls: Vec<String> = imgs
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect();
        if urls.is_empty() {
            continue;
        }
        if primary_urls.is_empty() {
            per_page_fallbacks.resize_with(urls.len(), Vec::new);
            primary_urls = urls;
        } else {
            for (idx, u) in urls.into_iter().enumerate() {
                if let Some(slot) = per_page_fallbacks.get_mut(idx) {
                    slot.push(u);
                }
            }
        }
    }

    let candidates: Vec<RawImageCandidate> = primary_urls
        .into_iter()
        .enumerate()
        .map(|(i, raw_url)| RawImageCandidate {
            raw_url,
            fallbacks: per_page_fallbacks.get(i).cloned().unwrap_or_default(),
            referer: page_url.clone(),
            origin: CandidateOrigin::StructuredJson,
        })
        .collect();

    let resolved = resolve_page_images(candidates, base);
    if resolved.pages.is_empty() {
        return None;
    }

    let mut nav = extract_prev_next(doc, page_url, base);
    if nav.prev.is_none()
        && let Some(prev_raw) = val.get("prevUrl").and_then(Value::as_str)
    {
        nav.prev = abs_url(base, prev_raw);
    }
    if nav.next.is_none()
        && let Some(next_raw) = val.get("nextUrl").and_then(Value::as_str)
    {
        nav.next = abs_url(base, next_raw);
    }

    let name = derive_chapter_title(doc, page_url);
    Some(StructuredChapterExtraction {
        chapter: Chapter {
            url: page_url.clone(),
            name,
            images: resolved.pages,
            prev: nav.prev,
            next: nav.next,
            manga_url: extract_manga_url_from_chapter(doc, page_url, base),
        },
        raw_candidates: resolved.raw_candidates,
        confidence: 0.94,
        source_tag: "ts_reader",
    })
}

fn try_madara_preloaded(
    doc: &Html,
    html: &str,
    page_url: &Url,
    base: &Url,
) -> Option<StructuredChapterExtraction> {
    let m = MADARA_PRELOAD_RE.find(html)?;
    let arr_slice = slice_balanced_json(&html[m.end()..], b'[', b']')?;
    let val: Value = serde_json::from_str(arr_slice).ok()?;
    let candidates = collect_image_candidates_from_value(&val, page_url, 0)?;
    build_structured_chapter(doc, page_url, base, candidates, 0.92, "madara_preloaded")
}

fn try_next_data_chapter(
    doc: &Html,
    page_url: &Url,
    base: &Url,
) -> Option<StructuredChapterExtraction> {
    for script in doc.select(&SCRIPT_SEL) {
        if script.value().attr("id") != Some("__NEXT_DATA__") {
            continue;
        }
        let body = script.text().collect::<String>();
        let Ok(val) = serde_json::from_str::<Value>(&body) else {
            continue;
        };
        let search_root = val.pointer("/props/pageProps").unwrap_or(&val);
        if let Some(candidates) = find_best_image_array(search_root, page_url, 0)
            && let Some(ext) =
                build_structured_chapter(doc, page_url, base, candidates, 0.91, "next_data")
        {
            return Some(ext);
        }
    }
    None
}

fn try_json_ld_chapter(
    doc: &Html,
    page_url: &Url,
    base: &Url,
) -> Option<StructuredChapterExtraction> {
    for script in doc.select(&SCRIPT_SEL) {
        if script.value().attr("type") != Some("application/ld+json") {
            continue;
        }
        let body = script.text().collect::<String>();
        let Ok(val) = serde_json::from_str::<Value>(&body) else {
            continue;
        };
        let is_comic_type = val.get("@type").and_then(Value::as_str).is_some_and(|t| {
            matches!(
                t.to_ascii_lowercase().as_str(),
                "comicissue" | "imagegallery" | "chapter" | "creativework"
            )
        });
        if !is_comic_type {
            continue;
        }
        if let Some(media) = val
            .get("associatedMedia")
            .or_else(|| val.get("image"))
            .and_then(|v| collect_image_candidates_from_value(v, page_url, 0))
            && let Some(ext) = build_structured_chapter(doc, page_url, base, media, 0.88, "json_ld")
        {
            return Some(ext);
        }
    }
    None
}

fn try_embedded_script_arrays(
    doc: &Html,
    page_url: &Url,
    base: &Url,
) -> Option<StructuredChapterExtraction> {
    for script in doc.select(&SCRIPT_SEL) {
        let body = script.text().collect::<String>();
        if body.len() < 16 {
            continue;
        }
        for caps in EMBEDDED_ARRAY_RE.find_iter(&body) {
            let bracket_pos = caps.end().saturating_sub(1);
            let Some(arr_slice) = slice_balanced_json(&body[bracket_pos..], b'[', b']') else {
                continue;
            };
            let Ok(val) = serde_json::from_str::<Value>(arr_slice) else {
                continue;
            };
            if let Some(candidates) = collect_image_candidates_from_value(&val, page_url, 0)
                && let Some(ext) =
                    build_structured_chapter(doc, page_url, base, candidates, 0.87, "embedded_json")
            {
                return Some(ext);
            }
        }
    }
    None
}

fn build_structured_chapter(
    doc: &Html,
    page_url: &Url,
    base: &Url,
    candidates: Vec<RawImageCandidate>,
    confidence: f32,
    source_tag: &'static str,
) -> Option<StructuredChapterExtraction> {
    let resolved = resolve_page_images(candidates, base);
    if resolved.pages.is_empty() {
        return None;
    }
    let nav = extract_prev_next(doc, page_url, base);
    let manga_url = extract_manga_url_from_chapter(doc, page_url, base);
    let name = derive_chapter_title(doc, page_url);

    Some(StructuredChapterExtraction {
        chapter: Chapter {
            url: page_url.clone(),
            name,
            images: resolved.pages,
            prev: nav.prev,
            next: nav.next,
            manga_url,
        },
        raw_candidates: resolved.raw_candidates,
        confidence,
        source_tag,
    })
}

fn derive_chapter_title(doc: &Html, page_url: &Url) -> String {
    extract_title(doc).unwrap_or_else(|| {
        extract_chapter_number(page_url.path()).map_or_else(
            || "Chapter".to_owned(),
            |n| format!("Chapter {}", format_chapter_num(n)),
        )
    })
}

fn find_best_image_array(val: &Value, page_url: &Url, depth: u8) -> Option<Vec<RawImageCandidate>> {
    if depth > 10 {
        return None;
    }
    let mut best: Option<Vec<RawImageCandidate>> = None;
    match val {
        Value::Array(_) => {
            if let Some(cands) = collect_image_candidates_from_value(val, page_url, depth)
                && best.as_ref().is_none_or(|b| cands.len() > b.len())
            {
                best = Some(cands);
            }
        }
        Value::Object(map) => {
            for (k, v) in map {
                let lower = k.to_ascii_lowercase();
                if matches!(
                    lower.as_str(),
                    "images" | "pages" | "pagelist" | "chapterimages" | "imglist" | "imageurls"
                ) && let Some(cands) =
                    collect_image_candidates_from_value(v, page_url, depth + 1)
                    && best.as_ref().is_none_or(|b| cands.len() > b.len())
                {
                    best = Some(cands);
                }
                if let Some(nested) = find_best_image_array(v, page_url, depth + 1)
                    && best.as_ref().is_none_or(|b| nested.len() > b.len())
                {
                    best = Some(nested);
                }
            }
        }
        _ => {}
    }
    best
}

fn collect_image_candidates_from_value(
    val: &Value,
    page_url: &Url,
    depth: u8,
) -> Option<Vec<RawImageCandidate>> {
    if depth > 12 {
        return None;
    }
    let arr = val.as_array()?;
    if arr.is_empty() {
        return None;
    }

    let mut out = Vec::with_capacity(arr.len());
    for item in arr {
        match item {
            Value::String(s) if looks_like_candidate_url_or_path(s) => {
                out.push(RawImageCandidate {
                    raw_url: s.clone(),
                    fallbacks: Vec::new(),
                    referer: page_url.clone(),
                    origin: CandidateOrigin::StructuredJson,
                });
            }
            Value::Object(map) => {
                let primary = [
                    "url",
                    "src",
                    "image",
                    "imageUrl",
                    "contentUrl",
                    "primaryUrl",
                    "path",
                    "file",
                ]
                .iter()
                .find_map(|key| map.get(*key).and_then(Value::as_str));
                if let Some(p) = primary
                    && looks_like_candidate_url_or_path(p)
                {
                    let mut fallbacks = Vec::new();
                    for fb_key in ["fallback", "fallbackUrl", "backup", "cdn"] {
                        if let Some(fb) = map.get(fb_key).and_then(Value::as_str)
                            && looks_like_candidate_url_or_path(fb)
                        {
                            fallbacks.push(fb.to_owned());
                        }
                    }
                    out.push(RawImageCandidate {
                        raw_url: p.to_owned(),
                        fallbacks,
                        referer: page_url.clone(),
                        origin: CandidateOrigin::StructuredJson,
                    });
                }
            }
            _ => {}
        }
    }

    if out.is_empty() { None } else { Some(out) }
}

fn looks_like_candidate_url_or_path(raw: &str) -> bool {
    let t = raw.trim();
    let is_js_ext = std::path::Path::new(t)
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("js"));
    if t.is_empty() || t.starts_with("data:") || t.starts_with("javascript:") || is_js_ext {
        return false;
    }
    t.starts_with("http://")
        || t.starts_with("https://")
        || t.starts_with("//")
        || t.starts_with('/')
}

fn parse_next_data_manga(val: &Value, page_url: &Url, base: &Url) -> Option<Manga> {
    let root = val.pointer("/props/pageProps").unwrap_or(val);
    let chapters = find_chapters_array(root, base, 0)?;
    if chapters.is_empty() {
        return None;
    }
    let title = find_first_string_key(root, &["title", "name", "seriesTitle"], 0)
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| page_url.host_str().unwrap_or("Manga").to_owned());
    let cover = find_first_string_key(root, &["cover", "coverUrl", "thumbnail", "poster"], 0)
        .and_then(|c| abs_url(base, &c));
    let description = find_first_string_key(root, &["description", "synopsis", "summary"], 0);
    let host = page_url.host_str().unwrap_or("unknown").to_owned();
    let mut manga = Manga::minimal(
        page_url.clone(),
        SiteId::generic(host),
        title,
        cover,
        chapters,
    );
    manga.description = description;
    Some(manga)
}

fn parse_json_ld_manga(doc: &Html, val: &Value, page_url: &Url, base: &Url) -> Option<Manga> {
    let is_series = val.get("@type").and_then(Value::as_str).is_some_and(|t| {
        matches!(
            t.to_ascii_lowercase().as_str(),
            "comicseries" | "book" | "series"
        )
    });
    if !is_series {
        return None;
    }
    let chapters = crate::parse::chapters::extract_chapters(doc, base);
    if chapters.is_empty() {
        return None;
    }
    let title = val
        .get("name")
        .and_then(Value::as_str)
        .map(clean_text)
        .filter(|s| !s.is_empty())
        .or_else(|| extract_title(doc))?;
    let cover = val
        .get("image")
        .and_then(Value::as_str)
        .and_then(|u| abs_url(base, u));
    let host = page_url.host_str().unwrap_or("unknown").to_owned();
    let mut manga = Manga::minimal(
        page_url.clone(),
        SiteId::generic(host),
        title,
        cover,
        chapters,
    );
    manga.description = val
        .get("description")
        .and_then(Value::as_str)
        .map(clean_text);
    Some(manga)
}

fn find_chapters_array(val: &Value, base: &Url, depth: u8) -> Option<Vec<ChapterRef>> {
    if depth > 8 {
        return None;
    }
    let Value::Object(map) = val else {
        return None;
    };
    for (k, v) in map {
        if matches!(
            k.to_ascii_lowercase().as_str(),
            "chapters" | "chapterlist" | "episodes"
        ) && let Some(arr) = v.as_array()
        {
            let mut refs = Vec::new();
            for item in arr {
                let Some(obj) = item.as_object() else {
                    continue;
                };
                let url_raw = obj
                    .get("url")
                    .or_else(|| obj.get("href"))
                    .or_else(|| obj.get("link"))
                    .and_then(Value::as_str);
                let Some(url) = url_raw.and_then(|u| abs_url(base, u)) else {
                    continue;
                };
                let name = obj
                    .get("name")
                    .or_else(|| obj.get("title"))
                    .and_then(Value::as_str)
                    .map(clean_text)
                    .filter(|s| !s.is_empty())
                    .unwrap_or_else(|| "Chapter".to_owned());
                let number =
                    extract_chapter_number(&name).or_else(|| extract_chapter_number(url.path()));
                refs.push(ChapterRef::simple(url, name, number));
            }
            if !refs.is_empty() {
                refs.sort_by(|a, b| match (a.number, b.number) {
                    (Some(na), Some(nb)) => na.total_cmp(&nb),
                    _ => std::cmp::Ordering::Equal,
                });
                return Some(refs);
            }
        }
        if let Some(nested) = find_chapters_array(v, base, depth + 1) {
            return Some(nested);
        }
    }
    None
}

fn find_first_string_key(val: &Value, keys: &[&str], depth: u8) -> Option<String> {
    if depth > 6 {
        return None;
    }
    let Value::Object(map) = val else {
        return None;
    };
    for key in keys {
        if let Some(s) = map.get(*key).and_then(Value::as_str) {
            let clean = clean_text(s);
            if !clean.is_empty() {
                return Some(clean);
            }
        }
    }
    for v in map.values() {
        if let Some(found) = find_first_string_key(v, keys, depth + 1) {
            return Some(found);
        }
    }
    None
}

/// Slices a balanced JSON object (`{...}`) or array (`[...]`) starting from the first `open`
/// byte after leading whitespace while respecting string literals and backslash escapes.
fn slice_balanced_json(input: &str, open: u8, close: u8) -> Option<&str> {
    let trimmed = input.trim_start();
    let bytes = trimmed.as_bytes();
    if bytes.first().copied() != Some(open) {
        return None;
    }
    let mut depth = 0_usize;
    let mut in_string = false;
    let mut escape = false;

    for (idx, &b) in bytes.iter().enumerate() {
        if escape {
            escape = false;
            continue;
        }
        if in_string {
            if b == b'\\' {
                escape = true;
            } else if b == b'"' {
                in_string = false;
            }
            continue;
        }
        if b == b'"' {
            in_string = true;
        } else if b == open {
            depth += 1;
        } else if b == close {
            depth = depth.saturating_sub(1);
            if depth == 0 {
                return trimmed.get(..=idx);
            }
        }
    }
    None
}
