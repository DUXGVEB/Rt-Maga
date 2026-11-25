//! Structure-first extraction for single-work image galleries and doujinshi pages.
//!
//! Many gallery-style comic websites display a work overview page containing a thumbnail grid
//! or embedded gallery metadata JSON rather than a traditional multi-chapter list. This module
//! detects those structures cleanly and synthesizes a readable chapter with ordered full-size
//! page image candidates and extension fallbacks.

use std::collections::HashSet;
use std::sync::LazyLock;

use regex::Regex;
use scraper::{Html, Selector};
use serde_json::Value;
use url::Url;

use crate::model::{Chapter, ChapterRef, Manga, PageImage, SiteId};
use crate::parse::dom::{abs_url, clean_text, image_candidates, is_ignored_image_url};
use crate::parse::meta::{
    extract_cover, extract_creators, extract_description, extract_status, extract_tags_and_genres,
    extract_title,
};

static GALLERY_THUMB_SEL: LazyLock<Selector> = LazyLock::new(|| {
    Selector::parse(
        "#thumbnail-container a[href], .gallerythumb a[href], .preview_item a[href], .thumb-container a[href], .thumbs a[href], .gallery_thumbs a[href]",
    )
    .expect("static selector is valid")
});
static IMG_SEL: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("img").expect("static selector is valid"));
static SCRIPT_SEL: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("script").expect("static selector is valid"));

static EMBEDDED_GALLERY_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(?:window\._gallery|var\s+gallery|__GALLERY__)\s*=\s*(?:JSON\.parse\()?")
        .expect("static regex is valid")
});
static THUMB_SUFFIX_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(.+?/)(\d+)t\.(jpg|jpeg|png|webp|avif)(?:\?.*)?$").expect("static regex is valid")
});

/// Extracted single-work gallery payload containing synthesized chapter metadata and pages.
#[derive(Debug, Clone)]
pub struct ExtractedGallery {
    /// Synthesized chapter representing all pages in this gallery.
    pub chapter: Chapter,
    /// Optional thumbnail grid cover fallback if page metadata omitted an explicit cover.
    pub fallback_cover: Option<Url>,
}

/// Attempts to parse `html` as a single-work gallery overview page and build a [`Manga`] with
/// enriched creators/tags and a single `"Full Gallery ({N} pages)"` chapter reference.
#[must_use]
pub fn parse_gallery_manga(html: &str, page_url: &Url, base: &Url) -> Option<Manga> {
    let doc = Html::parse_document(html);
    let gallery = extract_gallery_from_doc(&doc, html, page_url, base)?;
    let page_count = gallery.chapter.images.len();
    if page_count < 3 {
        return None;
    }

    let host = page_url.host_str().unwrap_or("unknown").to_owned();
    let title = extract_title(&doc).unwrap_or_else(|| host.clone());
    let cover = extract_cover(&doc, base).or(gallery.fallback_cover);
    let description = extract_description(&doc);
    let (genres, tags) = extract_tags_and_genres(&doc);
    let (authors, artists) = extract_creators(&doc);
    let status = extract_status(&doc);

    let mut chapter_url = page_url.clone();
    chapter_url.set_fragment(Some("rtmaga-gallery"));

    let chapter_ref = ChapterRef::simple(
        chapter_url,
        format!("Full Gallery ({page_count} pages)"),
        Some(1.0),
    );

    let mut manga = Manga::minimal(
        page_url.clone(),
        SiteId::generic(host),
        title,
        cover,
        vec![chapter_ref],
    );
    manga.description = description;
    manga.genres = genres;
    manga.tags = tags;
    manga.authors = authors;
    manga.artists = artists;
    manga.status = status;
    Some(manga)
}

/// Attempts to parse `html` as a gallery page or gallery reader page and produce its [`Chapter`].
#[must_use]
pub fn parse_gallery_chapter(html: &str, page_url: &Url, base: &Url) -> Option<Chapter> {
    let doc = Html::parse_document(html);
    extract_gallery_from_doc(&doc, html, page_url, base).map(|g| g.chapter)
}

/// Returns `true` when `doc` / `html` visibly represents a gallery thumbnail grid or embedded
/// gallery JSON descriptor.
#[must_use]
pub fn looks_like_gallery_page(doc: &Html, html: &str) -> bool {
    if EMBEDDED_GALLERY_RE.is_match(html) {
        return true;
    }
    doc.select(&GALLERY_THUMB_SEL).take(4).count() >= 4
}

fn extract_gallery_from_doc(
    doc: &Html,
    html: &str,
    page_url: &Url,
    base: &Url,
) -> Option<ExtractedGallery> {
    if let Some(res) = extract_from_embedded_gallery_json(doc, html, page_url, base) {
        return Some(res);
    }
    extract_from_thumbnail_grid(doc, page_url, base)
}

fn extract_from_embedded_gallery_json(
    doc: &Html,
    html: &str,
    page_url: &Url,
    base: &Url,
) -> Option<ExtractedGallery> {
    let mut raw_candidates: Vec<&str> = Vec::new();
    for m in EMBEDDED_GALLERY_RE.find_iter(html) {
        raw_candidates.push(&html[m.end()..]);
    }
    for slice in raw_candidates {
        let trimmed = slice.trim_start();
        let maybe_json = if trimmed.starts_with('"') || trimmed.starts_with('\'') {
            unescape_js_string_literal(trimmed)
        } else {
            slice_balanced_braces(trimmed).map(str::to_owned)
        };
        let Some(json_str) = maybe_json else {
            continue;
        };
        let Ok(val) = serde_json::from_str::<Value>(&json_str) else {
            continue;
        };
        if let Some(pages) = build_pages_from_gallery_value(doc, &val, page_url, base) {
            return Some(pages);
        }
    }

    // Also scan <script type="application/json"> payloads carrying `media_id` + `images.pages`.
    for script in doc.select(&SCRIPT_SEL) {
        let body = script.text().collect::<String>();
        if !body.contains("media_id") && !body.contains("pages") {
            continue;
        }
        let Ok(val) = serde_json::from_str::<Value>(body.trim()) else {
            continue;
        };
        if let Some(pages) = build_pages_from_gallery_value(doc, &val, page_url, base) {
            return Some(pages);
        }
    }

    None
}

fn build_pages_from_gallery_value(
    doc: &Html,
    val: &Value,
    page_url: &Url,
    base: &Url,
) -> Option<ExtractedGallery> {
    let pages_arr = val
        .pointer("/images/pages")
        .or_else(|| val.get("pages"))
        .and_then(Value::as_array)?;
    if pages_arr.len() < 2 {
        return None;
    }

    // Attempt to discover the image host/prefix from DOM thumbnails or explicit JSON fields.
    let prefix_url = find_gallery_image_prefix(doc, val, base);
    let media_id = val
        .get("media_id")
        .and_then(|v| {
            v.as_str()
                .map(str::to_owned)
                .or_else(|| v.as_u64().map(|n| n.to_string()))
        })
        .unwrap_or_default();

    let mut out = Vec::with_capacity(pages_arr.len());
    for (idx, item) in pages_arr.iter().enumerate() {
        let page_num = idx + 1;
        if let Some(direct_str) = item
            .as_str()
            .or_else(|| item.get("url").and_then(Value::as_str))
            .or_else(|| item.get("src").and_then(Value::as_str))
            .or_else(|| item.get("image").and_then(Value::as_str))
            && let Some(u) = abs_url(base, direct_str)
            && !is_ignored_image_url(&u)
        {
            out.push(PageImage {
                url: u,
                fallbacks: Vec::new(),
                referer: page_url.clone(),
            });
            continue;
        }

        let ext = item
            .get("t")
            .and_then(Value::as_str)
            .map_or("jpg", map_gallery_ext_code);

        if let Some((scheme_host, path_dir)) = prefix_url.as_ref() {
            let primary_str = format!("{scheme_host}{path_dir}{page_num}.{ext}");
            if let Ok(primary) = Url::parse(&primary_str) {
                let mut fallbacks = Vec::new();
                for alt_ext in ["jpg", "png", "webp"] {
                    if alt_ext != ext
                        && let Ok(fb) =
                            Url::parse(&format!("{scheme_host}{path_dir}{page_num}.{alt_ext}"))
                    {
                        fallbacks.push(fb);
                    }
                }
                out.push(PageImage {
                    url: primary,
                    fallbacks,
                    referer: page_url.clone(),
                });
            }
        } else if !media_id.is_empty() {
            // Try origin relative `/galleries/<media_id>/<n>.<ext>` if no separate CDN prefix exists.
            let origin = format!("{}://{}", base.scheme(), base.host_str().unwrap_or(""));
            if let Ok(primary) =
                Url::parse(&format!("{origin}/galleries/{media_id}/{page_num}.{ext}"))
            {
                out.push(PageImage {
                    url: primary,
                    fallbacks: Vec::new(),
                    referer: page_url.clone(),
                });
            }
        }
    }

    if out.len() < 2 {
        return None;
    }

    let name = extract_title(doc).unwrap_or_else(|| format!("Full Gallery ({} pages)", out.len()));
    let fallback_cover = out.first().map(|p| p.url.clone());

    Some(ExtractedGallery {
        chapter: Chapter {
            url: page_url.clone(),
            name,
            images: out,
            prev: None,
            next: None,
            manga_url: Some(page_url.clone()),
        },
        fallback_cover,
    })
}

fn find_gallery_image_prefix(doc: &Html, val: &Value, base: &Url) -> Option<(String, String)> {
    if let Some(cdn) = val
        .get("image_base")
        .or_else(|| val.get("cdn_url"))
        .and_then(Value::as_str)
        && let Some(u) = abs_url(base, cdn)
    {
        let origin = format!("{}://{}", u.scheme(), u.host_str()?);
        let dir = if u.path().ends_with('/') {
            u.path().to_owned()
        } else {
            format!("{}/", u.path())
        };
        return Some((origin, dir));
    }

    for anchor in doc.select(&GALLERY_THUMB_SEL) {
        let Some(img) = anchor.select(&IMG_SEL).next() else {
            continue;
        };
        for cand in image_candidates(img, base) {
            if let Some(caps) = THUMB_SUFFIX_RE.captures(cand.as_str()) {
                let prefix = caps.get(1)?.as_str();
                if let Ok(parsed) = Url::parse(prefix) {
                    let origin = format!("{}://{}", parsed.scheme(), parsed.host_str()?);
                    let mut path = parsed.path().to_owned();
                    if !path.ends_with('/') {
                        path.push('/');
                    }
                    // Upgrade `/1t.jpg` hosts when thumb subdomain uses `t.` -> `i.` isn't guessed,
                    // keep host verbatim unless CDN mirrors are present in attributes.
                    return Some((origin, path));
                }
            }
        }
    }
    None
}

fn extract_from_thumbnail_grid(doc: &Html, page_url: &Url, base: &Url) -> Option<ExtractedGallery> {
    let mut pages = Vec::new();
    let mut seen = HashSet::new();

    for anchor in doc.select(&GALLERY_THUMB_SEL) {
        let Some(href) = anchor.value().attr("href") else {
            continue;
        };
        if abs_url(base, href).is_none() {
            continue;
        }
        let Some(img) = anchor.select(&IMG_SEL).next() else {
            continue;
        };
        let candidates: Vec<Url> = image_candidates(img, base)
            .into_iter()
            .filter(|u| !is_ignored_image_url(u))
            .collect();
        let Some(thumb_url) = candidates.first().cloned() else {
            continue;
        };

        let (primary, fallbacks) = promote_thumbnail_to_fullsize(&thumb_url);
        if !seen.insert(primary.as_str().to_owned()) {
            continue;
        }

        let mut merged_fallbacks = fallbacks;
        for extra in candidates.into_iter().skip(1) {
            if extra != primary && !merged_fallbacks.contains(&extra) {
                merged_fallbacks.push(extra);
            }
        }
        if thumb_url != primary && !merged_fallbacks.contains(&thumb_url) {
            merged_fallbacks.push(thumb_url);
        }

        pages.push(PageImage {
            url: primary,
            fallbacks: merged_fallbacks,
            referer: page_url.clone(),
        });
    }

    if pages.len() < 3 {
        return None;
    }

    let title = extract_title(doc)
        .map(|s| clean_text(&s))
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| format!("Full Gallery ({} pages)", pages.len()));
    let fallback_cover = pages.first().map(|p| p.url.clone());

    Some(ExtractedGallery {
        chapter: Chapter {
            url: page_url.clone(),
            name: title,
            images: pages,
            prev: None,
            next: None,
            manga_url: Some(page_url.clone()),
        },
        fallback_cover,
    })
}

/// Promotes standard `<dir>/<n>t.<ext>` thumbnail URLs to `<dir>/<n>.<ext>` while placing
/// alternate extensions (`jpg`, `webp`, `png`) into `fallbacks` so image proxy never breaks.
#[must_use]
pub fn promote_thumbnail_to_fullsize(thumb: &Url) -> (Url, Vec<Url>) {
    let raw = thumb.as_str();
    if let Some(caps) = THUMB_SUFFIX_RE.captures(raw) {
        let prefix = &caps[1];
        let num = &caps[2];
        let ext = caps[3].to_ascii_lowercase();
        if let Ok(full) = Url::parse(&format!("{prefix}{num}.{ext}")) {
            let mut fallbacks = Vec::new();
            for alt in ["jpg", "webp", "png"] {
                if alt != ext
                    && let Ok(fb) = Url::parse(&format!("{prefix}{num}.{alt}"))
                {
                    fallbacks.push(fb);
                }
            }
            return (full, fallbacks);
        }
    }
    (thumb.clone(), Vec::new())
}

fn map_gallery_ext_code(code: &str) -> &'static str {
    if code.eq_ignore_ascii_case("p") {
        "png"
    } else if code.eq_ignore_ascii_case("w") {
        "webp"
    } else if code.eq_ignore_ascii_case("g") {
        "gif"
    } else if code.eq_ignore_ascii_case("a") {
        "avif"
    } else {
        "jpg"
    }
}

fn slice_balanced_braces(input: &str) -> Option<&str> {
    let bytes = input.as_bytes();
    if bytes.first().copied() != Some(b'{') {
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
        } else if b == b'{' {
            depth += 1;
        } else if b == b'}' {
            depth = depth.saturating_sub(1);
            if depth == 0 {
                return input.get(..=idx);
            }
        }
    }
    None
}

fn unescape_js_string_literal(input: &str) -> Option<String> {
    let quote = input.as_bytes().first().copied()?;
    if quote != b'"' && quote != b'\'' {
        return None;
    }
    let mut out = String::new();
    let mut chars = input[1..].chars();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            match chars.next()? {
                'u' => {
                    let hex: String = chars.by_ref().take(4).collect();
                    if hex.len() == 4
                        && let Ok(code) = u32::from_str_radix(&hex, 16)
                        && let Some(decoded) = char::from_u32(code)
                    {
                        out.push(decoded);
                    }
                }
                'n' => out.push('\n'),
                'r' => out.push('\r'),
                't' => out.push('\t'),
                other => out.push(other),
            }
        } else if ch == char::from(quote) {
            return Some(out);
        } else {
            out.push(ch);
        }
    }
    None
}
