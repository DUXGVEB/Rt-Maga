//! Metadata extraction (series title, cover image, creators, tags, status, and favicon hints)
//! from arbitrary manga and gallery HTML pages.

use std::collections::HashSet;
use std::sync::LazyLock;

use regex::Regex;
use scraper::{Html, Selector};
use url::Url;

use crate::parse::dom::{abs_url, clean_text, image_candidates, strip_site_suffix};

const MAX_META_LIST_ITEMS: usize = 64;

static OG_TITLE_SEL: LazyLock<Selector> = LazyLock::new(|| {
    Selector::parse(r#"meta[property="og:title"], meta[name="twitter:title"]"#)
        .expect("static selector is valid")
});
static H1_SEL: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("h1").expect("static selector is valid"));
static TITLE_SEL: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("title").expect("static selector is valid"));
static OG_IMAGE_SEL: LazyLock<Selector> = LazyLock::new(|| {
    Selector::parse(r#"meta[property="og:image"], meta[name="twitter:image"]"#)
        .expect("static selector is valid")
});
static COVER_IMG_SEL: LazyLock<Selector> = LazyLock::new(|| {
    Selector::parse(
        ".book_avatar img, .detail-info img, .cover img, .summary_image img, #cover img, .gallery-cover img, .thumb img, [itemprop='image']",
    )
    .expect("static selector is valid")
});
static DESC_BODY_SEL: LazyLock<Selector> = LazyLock::new(|| {
    Selector::parse(
        ".summary__content, .description-summary .summary__content, .manga-excerpt, .entry-content[itemprop='description'], #summary, [itemprop='description'], .story-detail-info .detail",
    )
    .expect("static selector is valid")
});
static DESC_META_SEL: LazyLock<Selector> = LazyLock::new(|| {
    Selector::parse("meta[property='og:description'], meta[name='description']")
        .expect("static selector is valid")
});
static GENRE_SEL: LazyLock<Selector> = LazyLock::new(|| {
    Selector::parse(
        ".genres-content a, .mgen a, .seriestugenre a, .post-content_item a[href*='genre'], a[href*='/genre/'], a[href*='/genres/'], a[href*='/the-loai/'], a[href*='/category/']",
    )
    .expect("static selector is valid")
});
static TAG_SEL: LazyLock<Selector> = LazyLock::new(|| {
    Selector::parse(
        ".tag-links a, a[href*='/tag/'], a[href*='/tags/'], a[rel='tag'], .tag-container .tags a .name, .tag-container a.tag .name, .tag-container a.tag, a[href*='/parody/'], a[href*='/character/']",
    )
    .expect("static selector is valid")
});
static AUTHOR_SEL: LazyLock<Selector> = LazyLock::new(|| {
    Selector::parse(
        ".author-content a, a[href*='/author/'], a[href*='/tac-gia/'], [itemprop='author']",
    )
    .expect("static selector is valid")
});
static ARTIST_SEL: LazyLock<Selector> = LazyLock::new(|| {
    Selector::parse(
        ".artist-content a, a[href*='/artist/'], a[href*='/group/'], a[href*='/circle/']",
    )
    .expect("static selector is valid")
});
static STATUS_SEL: LazyLock<Selector> = LazyLock::new(|| {
    Selector::parse(".post-status .summary-content, .tsinfo .imptdt, .detail-info .status")
        .expect("static selector is valid")
});
static ICON_LINK_SEL: LazyLock<Selector> = LazyLock::new(|| {
    Selector::parse(
        "link[rel='icon'], link[rel='shortcut icon'], link[rel='apple-touch-icon'], link[rel='apple-touch-icon-precomposed']",
    )
    .expect("static selector is valid")
});
static RATING_META_SEL: LazyLock<Selector> = LazyLock::new(|| {
    Selector::parse("meta[name='rating'], meta[property='rating']")
        .expect("static selector is valid")
});
static TRAILING_COUNT_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\s*\(\s*[\d,.]+\s*[KkMm]?\s*\)\s*$").expect("static regex is valid")
});

/// Extracts the most likely manga title using `h1`, `og:title`, then `<title>`.
pub fn extract_title(doc: &Html) -> Option<String> {
    if let Some(h1) = doc.select(&H1_SEL).next() {
        let text = clean_text(&h1.text().collect::<String>());
        if !text.is_empty() {
            return Some(text);
        }
    }

    if let Some(meta) = doc.select(&OG_TITLE_SEL).next()
        && let Some(content) = meta.value().attr("content")
    {
        let text = strip_site_suffix(content);
        if !text.is_empty() {
            return Some(text);
        }
    }

    if let Some(title) = doc.select(&TITLE_SEL).next() {
        let text = strip_site_suffix(&title.text().collect::<String>());
        if !text.is_empty() {
            return Some(text);
        }
    }

    None
}

/// Extracts the most likely series cover image URL from dedicated cover containers or `og:image`.
pub fn extract_cover(doc: &Html, base: &Url) -> Option<Url> {
    if let Some(el) = doc.select(&COVER_IMG_SEL).next() {
        if let Some(url) = image_candidates(el, base).into_iter().next() {
            return Some(url);
        }
        if let Some(content) = el.value().attr("content")
            && let Some(url) = abs_url(base, content)
        {
            return Some(url);
        }
    }

    for meta in doc.select(&OG_IMAGE_SEL) {
        if let Some(content) = meta.value().attr("content")
            && let Some(url) = abs_url(base, content)
        {
            return Some(url);
        }
    }

    None
}

/// Extracts the series synopsis from dedicated summary containers or `<meta>` description tags.
pub fn extract_description(doc: &Html) -> Option<String> {
    if let Some(body) = doc
        .select(&DESC_BODY_SEL)
        .map(|el| clean_text(&el.text().collect::<String>()))
        .find(|s| s.len() >= 12)
    {
        return Some(body);
    }
    doc.select(&DESC_META_SEL)
        .find_map(|m| m.value().attr("content").map(clean_text))
        .filter(|s| !s.is_empty())
}

/// Extracts deduplicated `(genres, tags)` from manga and gallery tag containers.
pub fn extract_tags_and_genres(doc: &Html) -> (Vec<String>, Vec<String>) {
    let mut seen = HashSet::new();
    let mut genres = Vec::new();
    for el in doc.select(&GENRE_SEL) {
        let raw = el.text().collect::<String>();
        if let Some(clean) = clean_tag_label(&raw) {
            let key = clean.to_ascii_lowercase();
            if seen.insert(key) {
                genres.push(clean);
                if genres.len() >= MAX_META_LIST_ITEMS {
                    break;
                }
            }
        }
    }

    let mut tags = Vec::new();
    for el in doc.select(&TAG_SEL) {
        let raw = el.text().collect::<String>();
        if let Some(clean) = clean_tag_label(&raw) {
            let key = clean.to_ascii_lowercase();
            if seen.insert(key) {
                tags.push(clean);
                if tags.len() >= MAX_META_LIST_ITEMS {
                    break;
                }
            }
        }
    }

    (genres, tags)
}

/// Extracts deduplicated `(authors, artists)` creator names from common manga/gallery selectors.
pub fn extract_creators(doc: &Html) -> (Vec<String>, Vec<String>) {
    let mut authors = Vec::new();
    let mut seen_authors = HashSet::new();
    for el in doc.select(&AUTHOR_SEL) {
        let raw = el.text().collect::<String>();
        if let Some(clean) = clean_tag_label(&raw)
            && seen_authors.insert(clean.to_ascii_lowercase())
        {
            authors.push(clean);
            if authors.len() >= 16 {
                break;
            }
        }
    }

    let mut artists = Vec::new();
    let mut seen_artists = HashSet::new();
    for el in doc.select(&ARTIST_SEL) {
        let raw = el.text().collect::<String>();
        if let Some(clean) = clean_tag_label(&raw)
            && seen_artists.insert(clean.to_ascii_lowercase())
        {
            artists.push(clean);
            if artists.len() >= 16 {
                break;
            }
        }
    }

    (authors, artists)
}

/// Extracts normalized publication status (`"ongoing"`, `"completed"`, `"hiatus"`) when present.
pub fn extract_status(doc: &Html) -> Option<String> {
    for el in doc.select(&STATUS_SEL) {
        let text = clean_text(&el.text().collect::<String>()).to_ascii_lowercase();
        if text.contains("completed") || text.contains("hoàn thành") || text.contains("full") {
            return Some("completed".to_owned());
        }
        if text.contains("ongoing")
            || text.contains("đang tiến hành")
            || text.contains("đang cập nhật")
        {
            return Some("ongoing".to_owned());
        }
        if text.contains("hiatus") || text.contains("tạm ngưng") {
            return Some("hiatus".to_owned());
        }
    }
    None
}

/// Extracts ordered icon URL candidates from HTML `<link rel="icon">` tags plus origin defaults.
pub fn extract_favicon_candidates(doc: &Html, base: &Url) -> Vec<Url> {
    let mut out = Vec::new();
    for el in doc.select(&ICON_LINK_SEL) {
        if let Some(href) = el.value().attr("href")
            && let Some(url) = abs_url(base, href)
            && !out.contains(&url)
        {
            out.push(url);
        }
    }
    if let Ok(origin) = Url::parse(&format!(
        "{}://{}{}",
        base.scheme(),
        base.host_str().unwrap_or(""),
        base.port().map_or_else(String::new, |p| format!(":{p}"))
    )) {
        for suffix in [
            "/favicon.ico",
            "/apple-touch-icon.png",
            "/apple-touch-icon-precomposed.png",
        ] {
            if let Ok(u) = origin.join(suffix)
                && !out.contains(&u)
            {
                out.push(u);
            }
        }
    }
    out
}

/// Detects adult / 18+ content solely from structure, metadata ratings, URL paths, and tags
/// without hardcoding any third-party adult site domain names.
#[must_use]
pub fn detect_adult_signals(url: &Url, html: &str, genres: &[String], tags: &[String]) -> bool {
    const ADULT_TOKENS: &[&str] = &[
        "18+",
        "r18",
        "r-18",
        "adult",
        "mature",
        "hentai",
        "doujinshi",
        "ecchi",
        "smut",
        "erotica",
        "uncensored",
        "adult webtoon",
        "adult-webtoon",
        "manhwa 18",
        "manhwa-18",
        "manhwa 18+",
        "manga 18+",
        "webtoon 18+",
        "truyện 18+",
        "truyen 18+",
        "người lớn",
        "nguoi lon",
        "nsfw",
        "sexual content",
        "yao",
        "yaoi",
        "yuri",
        "bara",
        "ahegao",
        "futanari",
    ];

    for label in genres.iter().chain(tags.iter()) {
        let low = label.trim().to_ascii_lowercase();
        if ADULT_TOKENS
            .iter()
            .any(|tok| low == *tok || low.contains(tok))
        {
            return true;
        }
    }

    let path_low = url.path().to_ascii_lowercase();
    if [
        "/hentai",
        "/doujinshi",
        "/18+",
        "/r18",
        "/adult",
        "/nsfw",
        "/manhwa-18",
    ]
    .iter()
    .any(|seg| path_low.contains(seg))
    {
        return true;
    }

    let lower_html = html.to_ascii_lowercase();
    if lower_html.contains("wpmanga-adault")
        || lower_html.contains("rta-5042-1996-1400-1577-rta")
        || lower_html.contains("age-verification")
        || lower_html.contains("over18")
        || lower_html.contains("nsfw-warning")
    {
        return true;
    }

    let doc = Html::parse_document(html);
    for meta in doc.select(&RATING_META_SEL) {
        if let Some(content) = meta.value().attr("content") {
            let c = content.trim().to_ascii_lowercase();
            if c.contains("adult") || c.contains("mature") || c.contains("18") || c.contains("rta")
            {
                return true;
            }
        }
    }

    false
}

fn clean_tag_label(raw: &str) -> Option<String> {
    let stripped = TRAILING_COUNT_RE.replace(raw.trim(), "");
    let cleaned = clean_text(&stripped);
    if cleaned.len() < 2 || cleaned.len() > 48 {
        return None;
    }
    if cleaned
        .chars()
        .all(|c| c.is_ascii_digit() || c == ',' || c == '.')
    {
        return None;
    }
    let low = cleaned.to_ascii_lowercase();
    if matches!(
        low.as_str(),
        "all" | "more" | "view all" | "updating" | "unknown" | "n/a" | "..."
    ) {
        return None;
    }
    Some(cleaned)
}
