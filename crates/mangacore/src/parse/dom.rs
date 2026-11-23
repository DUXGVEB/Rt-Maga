//! Pure DOM utilities: URL resolution, image attribute priority, text cleanup, and URL pattern keys.

use std::sync::LazyLock;

use regex::Regex;
use scraper::{ElementRef, Html, Selector};
use url::Url;

/// Ordered image source attributes checked from highest priority (lazy-load real URL) to lowest.
const IMAGE_ATTRS: &[&str] = &[
    "data-original",
    "data-src",
    "data-lazy-src",
    "data-cdn",
    "data-url",
    "data-sv1",
    "data-sv2",
    "data-fb",
    "src",
];

/// Generic ad/tracker subdomain prefixes filtered out from chapter image candidates.
const AD_HOST_FRAGMENTS: &[&str] = &[
    "adservice.",
    "ads.",
    "adserver.",
    "popads.",
    "tracker.",
    "analytics.",
];

/// Directory or filename fragments strongly indicating non-manga UI or promotional images.
/// Checked against the filename or full directory segments so manga title slugs like
/// `/series/avatar-of-stars/ch-1/01.jpg` or `/manga/iconic-hero/01.jpg` are never falsely rejected.
const IGNORED_DIR_SEGMENTS: &[&str] = &[
    "ads", "ad", "banners", "avatars", "icons", "emojis", "stickers",
];

const IGNORED_FILENAME_FRAGMENTS: &[&str] = &[
    "favicon",
    "logo.",
    "logo-",
    "logo_",
    "avatar.",
    "avatar-",
    "avatar_",
    "icon.",
    "icon-",
    "icon_",
    "emoji",
    "sticker",
    "banner.",
    "banner-",
    "banner_",
    "loading.",
    "loader.",
    "placeholder",
    "spinner",
    "blank.",
    "pixel.",
    "1x1.",
];

static NUMBER_SEGMENT_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\d+(?:[.-]\d+)*").expect("static regex is valid"));

static BASE_HREF_SEL: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("base[href]").expect("static selector is valid"));

/// Resolves a `<base href="...">` element if present, otherwise returns `page_url`.
pub fn effective_base_url(doc: &Html, page_url: &Url) -> Url {
    doc.select(&BASE_HREF_SEL)
        .next()
        .and_then(|el| el.value().attr("href"))
        .and_then(|href| page_url.join(href.trim()).ok())
        .unwrap_or_else(|| page_url.clone())
}

/// Resolves a potentially relative, protocol-relative, or whitespace-padded URL against `base`.
pub fn abs_url(base: &Url, raw: &str) -> Option<Url> {
    let trimmed = raw.trim();
    if trimmed.is_empty()
        || trimmed.starts_with("javascript:")
        || trimmed.starts_with("data:")
        || trimmed.starts_with("mailto:")
        || trimmed.starts_with('#')
    {
        return None;
    }
    let mut resolved = if trimmed.starts_with("//") {
        Url::parse(&format!("{}:{trimmed}", base.scheme())).ok()?
    } else {
        base.join(trimmed).ok()?
    };
    if resolved.scheme() == "http" || resolved.scheme() == "https" {
        resolved.set_fragment(None);
        Some(resolved)
    } else {
        None
    }
}

/// Extracts an ordered, deduplicated list of image URLs from an `<img>` or `<source>` element.
///
/// The first element is the preferred primary URL; subsequent entries are fallback mirrors.
pub fn image_candidates(el: ElementRef<'_>, base: &Url) -> Vec<Url> {
    let mut urls = Vec::new();

    for attr in IMAGE_ATTRS {
        if let Some(val) = el.value().attr(attr)
            && let Some(url) = abs_url(base, val)
            && !urls.contains(&url)
        {
            urls.push(url);
        }
    }

    if let Some(srcset) = el
        .value()
        .attr("srcset")
        .or_else(|| el.value().attr("data-srcset"))
        && let Some(best) = parse_srcset_largest(srcset)
        && let Some(url) = abs_url(base, best)
        && !urls.contains(&url)
    {
        urls.push(url);
    }

    urls
}

/// Picks the URL with the largest width/density descriptor from an HTML `srcset` attribute.
pub fn parse_srcset_largest(srcset: &str) -> Option<&str> {
    let mut best: Option<(&str, f32)> = None;
    for part in srcset.split(',') {
        let mut tokens = part.split_whitespace();
        let Some(url) = tokens.next() else {
            continue;
        };
        let weight = tokens
            .next()
            .and_then(|desc| {
                desc.strip_suffix('w')
                    .or_else(|| desc.strip_suffix('x'))
                    .and_then(|n| n.parse::<f32>().ok())
            })
            .unwrap_or(1.0);
        if best.is_none_or(|(_, w)| weight > w) {
            best = Some((url, weight));
        }
    }
    best.map(|(url, _)| url)
}

/// Returns `true` if the image URL clearly belongs to ads, icons, SVGs, GIFs, or placeholders.
pub fn is_ignored_image_url(url: &Url) -> bool {
    let ext = std::path::Path::new(url.path())
        .extension()
        .and_then(std::ffi::OsStr::to_str)
        .unwrap_or("");
    if ext.eq_ignore_ascii_case("svg")
        || ext.eq_ignore_ascii_case("gif")
        || ext.eq_ignore_ascii_case("ico")
    {
        return true;
    }
    if let Some(host) = url.host_str() {
        let lower_host = host.to_ascii_lowercase();
        if AD_HOST_FRAGMENTS
            .iter()
            .any(|frag| lower_host.contains(frag))
        {
            return true;
        }
    }
    let segments: Vec<String> = url
        .path_segments()
        .map(|it| {
            it.filter(|s| !s.is_empty())
                .map(str::to_ascii_lowercase)
                .collect()
        })
        .unwrap_or_default();

    let Some((filename, dirs)) = segments.split_last() else {
        return false;
    };

    if dirs
        .iter()
        .any(|dir| IGNORED_DIR_SEGMENTS.contains(&dir.as_str()))
    {
        return true;
    }

    IGNORED_FILENAME_FRAGMENTS
        .iter()
        .any(|frag| filename.contains(frag))
}

/// Returns `true` if the `<img>` declares an explicit width or height strictly smaller than `min_px`.
pub fn is_declared_too_small(el: ElementRef<'_>, min_px: u32) -> bool {
    for attr in ["width", "height"] {
        if let Some(raw) = el.value().attr(attr)
            && let Ok(px) = raw.trim().trim_end_matches("px").parse::<u32>()
            && px > 0
            && px < min_px
        {
            return true;
        }
    }
    false
}

/// Returns `true` if `el` or any of its ancestors is a layout chrome tag (`header`, `nav`,
/// `footer`, `aside`).
pub fn is_inside_chrome(el: ElementRef<'_>) -> bool {
    for anc in el.ancestors().filter_map(ElementRef::wrap) {
        let name = anc.value().name();
        if matches!(name, "header" | "nav" | "footer" | "aside") {
            return true;
        }
    }
    false
}

/// Collapses runs of whitespace into single spaces and trims both ends.
pub fn clean_text(raw: &str) -> String {
    raw.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Builds a structural URL pattern key by keeping the host and replacing digit runs in the path
/// with `{n}` so sibling chapter or page URLs group together.
pub fn url_pattern_key(url: &Url) -> String {
    let host = url.host_str().unwrap_or("");
    let normalized_path = NUMBER_SEGMENT_RE.replace_all(url.path(), "{n}");
    format!("{host}{normalized_path}")
}

/// Strips trailing site name suffixes such as `" - SiteName"` or `" | Example"` from `<title>`.
pub fn strip_site_suffix(title: &str) -> String {
    let cleaned = clean_text(title);
    for sep in [" | ", " - ", " — ", " :: "] {
        if let Some((head, _)) = cleaned.split_once(sep) {
            let trimmed = head.trim();
            if trimmed.chars().count() >= 2 {
                return trimmed.to_owned();
            }
        }
    }
    cleaned
}
