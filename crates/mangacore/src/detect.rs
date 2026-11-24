//! Universal comic-source identification, access-gate detection, JS-shell detection, and
//! page-type confidence scoring.

use std::sync::LazyLock;

use regex::Regex;
use scraper::{ElementRef, Html, Selector};
use serde::{Deserialize, Serialize};
use url::Url;

use crate::parse::chapters::{extract_chapter_number, extract_chapters};
use crate::parse::dom::{abs_url, clean_text, effective_base_url, is_inside_chrome};
use crate::parse::images::extract_chapter_images;
use crate::parse::meta::extract_title;
use crate::parse::nav::extract_prev_next;
use crate::recognize::{UrlKind, classify_normalized_url};

static SCRIPT_SEL: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("script").expect("valid script selector"));
static PASSWORD_INPUT_SEL: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("input[type='password']").expect("valid password selector"));
static READER_CONTAINER_SEL: LazyLock<Selector> = LazyLock::new(|| {
    Selector::parse(
        ".reading-content, #readerarea, .chapter_content, .chapter-content, #viewer, .comic-viewer, #chapter-reader, [data-reader], .page-break",
    )
    .expect("valid reader container selector")
});
static IMG_SEL: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("img, picture source").expect("valid img selector"));
static CARD_LINK_SEL: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("a[href] img").expect("valid card link img selector"));
static META_TYPE_SEL: LazyLock<Selector> = LazyLock::new(|| {
    Selector::parse("meta[property='og:type'], link[rel='canonical']").expect("valid meta selector")
});
static TEXT_COUNT_SEL: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("article, p, main").expect("valid prose selector"));
static ROOT_SHELL_SEL: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("#__next, #__nuxt, #app, #root").expect("valid shell sel"));

static IMG_ATTR_HINTS_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(?:chapterImages|pageList|chapter_preloaded_images|ts_reader\.run|__NEXT_DATA__|__NUXT__)")
        .expect("valid state regex")
});

/// Specific authentication, anti-bot, payment, geo-block, or DRM gate detected on a fetched page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum AccessGateKind {
    /// Interactive Cloudflare, Turnstile, DDoS-Guard, or CAPTCHA challenge.
    Verification,
    /// Login form or authentication wall with no active session cookies.
    LoginRequired,
    /// Login redirect or auth rejection when session cookies were already present.
    SessionExpired,
    /// Coin/VIP/subscription paywall blocking access to this chapter or series.
    Paywall,
    /// Region or legal availability restriction (HTTP 451 / geo notice).
    GeoRestriction,
    /// Browser-only display protection / DRM (e.g. IMGX canvas taint, encrypted tiles).
    DrmProtected,
    /// Upstream policy/markup indicates automated extraction is disallowed.
    ExtractionNotPermitted,
}

/// Hint for default reader orientation/direction inferred from page metadata or markup.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ReadingDirectionHint {
    /// Right-to-left traditional manga pagination.
    Rtl,
    /// Left-to-right comic pagination.
    Ltr,
    /// Continuous vertical webtoon / long-strip scroll.
    Webtoon,
}

/// Explicitly indicates whether session cookies are currently recorded for the target domain.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum SessionPresence {
    /// One or more session cookies were previously synced/stored for this domain.
    WithCookies,
    /// No session cookies exist for this domain.
    NoCookies,
}

/// Document-level comic metadata hints extracted during detection.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PageMetadataHints {
    /// Series title candidate extracted from headings or structured metadata.
    pub series_title: Option<String>,
    /// Chapter label/title candidate when on a chapter page.
    pub chapter_title: Option<String>,
    /// Expected number of pages reported by embedded metadata/indicators, if available.
    pub expected_page_count: Option<usize>,
    /// Inferred reading direction hint.
    pub reading_direction: Option<ReadingDirectionHint>,
    /// Canonical URL declared via `<link rel="canonical">` if valid.
    pub canonical_url: Option<Url>,
}

/// Inputs passed into universal document analysis.
#[derive(Debug, Clone, Copy)]
pub struct PageInspectInput<'a> {
    /// Originally requested normalized URL.
    pub submitted_url: &'a Url,
    /// Final URL observed after HTTP redirects.
    pub final_url: &'a Url,
    /// Upstream HTTP status code.
    pub status: u16,
    /// Optional HTTP `Content-Type` media type.
    pub content_type: Option<&'a str>,
    /// Raw HTML or API body string.
    pub html: &'a str,
    /// Parsed HTML document tree.
    pub doc: &'a Html,
    /// Whether session cookies were present for `final_url`.
    pub session: SessionPresence,
}

/// Result of inspecting a fetched page to determine page kind, confidence, JS dependency, and gates.
#[derive(Debug, Clone)]
pub struct PageAnalysis {
    /// Best classified semantic page type.
    pub page_kind: UrlKind,
    /// Detection confidence score (`0.0..=1.0`).
    pub confidence: f32,
    /// Human-readable evidence strings listing inspected signals.
    pub signals: Vec<String>,
    /// `true` when page requires JS execution/hydration to render comic content.
    pub js_shell_detected: bool,
    /// Detected authentication, verification, paywall, geo, or DRM gate, if any.
    pub access_gate: Option<AccessGateKind>,
    /// `true` when the payload appears to be binary/protobuf or non-JSON API data.
    pub non_json_api_payload: bool,
    /// Extracted comic metadata hints.
    pub metadata: PageMetadataHints,
}

/// Analyzes URL patterns, redirects, metadata, DOM structure, scripts, and access gates to
/// classify any fetched page and attach confidence + diagnostic signals.
#[must_use]
pub fn analyze_html_page(ctx: PageInspectInput<'_>) -> PageAnalysis {
    let mut signals = Vec::new();
    let lower_html = ctx.html.to_ascii_lowercase();
    let base = effective_base_url(ctx.doc, ctx.final_url);

    let host = ctx
        .final_url
        .host_str()
        .unwrap_or("unknown")
        .to_ascii_lowercase();
    let (_, _, url_kind) = classify_normalized_url(ctx.final_url, &host);

    let redirected = ctx.submitted_url.path() != ctx.final_url.path()
        || ctx.submitted_url.host_str() != ctx.final_url.host_str();
    if redirected {
        signals.push(format!("redirected_to={}", ctx.final_url.path()));
    }

    if is_binary_or_non_json_content_type(ctx.content_type, ctx.html) {
        signals.push(format!(
            "unsupported_content_type={}",
            ctx.content_type.unwrap_or("unknown")
        ));
        return PageAnalysis {
            page_kind: UrlKind::ApiEndpoint,
            confidence: 0.75,
            signals,
            js_shell_detected: false,
            access_gate: None,
            non_json_api_payload: true,
            metadata: PageMetadataHints::default(),
        };
    }

    let imgs = extract_chapter_images(ctx.doc, ctx.final_url, &base);
    let chapters = extract_chapters(ctx.doc, &base);
    let nav = extract_prev_next(ctx.doc, ctx.final_url, &base);
    let reader_box = if ctx.doc.select(&READER_CONTAINER_SEL).next().is_some() {
        ReaderBoxPresence::Present
    } else {
        ReaderBoxPresence::Absent
    };
    let state_script = if has_embedded_reader_state(ctx.doc, &lower_html) {
        EmbeddedStatePresence::Present
    } else {
        EmbeddedStatePresence::Absent
    };
    let js_shell = detect_js_shell(ctx.doc, &lower_html, imgs.len(), chapters.len());
    let catalog_cards = ctx.doc.select(&CARD_LINK_SEL).count();

    let access_gate = detect_access_gate(
        ctx,
        &lower_html,
        imgs.len(),
        chapters.len(),
        state_script,
        &mut signals,
    );

    if let Some(gate) = access_gate {
        let gate_kind = match gate {
            AccessGateKind::LoginRequired | AccessGateKind::SessionExpired => UrlKind::LoginPage,
            AccessGateKind::Verification => UrlKind::VerificationPage,
            AccessGateKind::Paywall => UrlKind::PaywallPage,
            _ => UrlKind::BlockedOrRestricted,
        };
        return PageAnalysis {
            page_kind: gate_kind,
            confidence: 0.9,
            signals,
            js_shell_detected: js_shell,
            access_gate: Some(gate),
            non_json_api_payload: false,
            metadata: extract_metadata_hints(ctx.doc, &base, &lower_html, 0),
        };
    }

    let sigs = ComicSignalsInput {
        final_url: ctx.final_url,
        url_kind,
        img_count: imgs.len(),
        chapter_count: chapters.len(),
        has_chapter_nav: nav.prev.is_some() || nav.next.is_some(),
        has_reader_box: reader_box == ReaderBoxPresence::Present,
        has_state_script: state_script == EmbeddedStatePresence::Present,
        js_shell,
        catalog_cards,
        doc: ctx.doc,
        lower_html: &lower_html,
    };
    let (page_kind, confidence) = classify_comic_page_kind(&sigs, &mut signals);

    if js_shell {
        signals.push("js_spa_shell_detected".to_owned());
    }

    let chapter_img_count = if page_kind == UrlKind::ChapterPage {
        imgs.len()
    } else {
        0
    };

    PageAnalysis {
        page_kind,
        confidence,
        signals,
        js_shell_detected: js_shell,
        access_gate: None,
        non_json_api_payload: false,
        metadata: extract_metadata_hints(ctx.doc, &base, &lower_html, chapter_img_count),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReaderBoxPresence {
    Present,
    Absent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EmbeddedStatePresence {
    Present,
    Absent,
}

#[expect(
    clippy::struct_excessive_bools,
    reason = "internal helper bundles precomputed page classification flags"
)]
struct ComicSignalsInput<'a> {
    final_url: &'a Url,
    url_kind: UrlKind,
    img_count: usize,
    chapter_count: usize,
    has_chapter_nav: bool,
    has_reader_box: bool,
    has_state_script: bool,
    js_shell: bool,
    catalog_cards: usize,
    doc: &'a Html,
    lower_html: &'a str,
}

fn is_binary_or_non_json_content_type(content_type: Option<&str>, body: &str) -> bool {
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
    false
}

fn has_embedded_reader_state(doc: &Html, lower_html: &str) -> bool {
    if IMG_ATTR_HINTS_RE.is_match(lower_html) {
        return true;
    }
    doc.select(&SCRIPT_SEL).any(|s| {
        s.value()
            .attr("id")
            .is_some_and(|id| id == "__NEXT_DATA__" || id == "__NUXT_DATA__")
            || s.value()
                .attr("type")
                .is_some_and(|t| t == "application/ld+json")
    })
}

fn detect_js_shell(doc: &Html, lower_html: &str, img_count: usize, chapter_count: usize) -> bool {
    if img_count >= 3 || chapter_count >= 2 {
        return false;
    }
    let has_root = doc.select(&ROOT_SHELL_SEL).next().is_some();
    let has_reader_box = doc.select(&READER_CONTAINER_SEL).next().is_some();
    let visible_img_tags = doc.select(&IMG_SEL).count();
    let script_count = doc.select(&SCRIPT_SEL).count();
    let body_text_len: usize = doc
        .select(&TEXT_COUNT_SEL)
        .map(|el| clean_text(&el.text().collect::<String>()).len())
        .sum();

    if (has_root || lower_html.contains("__next_data__") || lower_html.contains("data-reactroot"))
        && visible_img_tags < 3
        && script_count >= 1
    {
        return true;
    }
    if has_reader_box && img_count == 0 && script_count >= 1 {
        return true;
    }
    body_text_len < 160 && script_count >= 2 && visible_img_tags == 0
}

fn detect_access_gate(
    ctx: PageInspectInput<'_>,
    lower_html: &str,
    img_count: usize,
    chapter_count: usize,
    state_script: EmbeddedStatePresence,
    signals: &mut Vec<String>,
) -> Option<AccessGateKind> {
    if looks_like_verification_gate(lower_html, img_count, chapter_count) {
        signals.push("verification_challenge_markers".to_owned());
        return Some(AccessGateKind::Verification);
    }
    if looks_like_drm_imgx(lower_html) && img_count == 0 {
        signals.push("drm_or_imgx_canvas_taint".to_owned());
        return Some(AccessGateKind::DrmProtected);
    }
    if looks_like_paywall(lower_html, img_count, chapter_count, state_script) {
        signals.push("paywall_or_vip_markers".to_owned());
        return Some(AccessGateKind::Paywall);
    }
    if looks_like_geo_block(lower_html, img_count, chapter_count) {
        signals.push("geo_restriction_markers".to_owned());
        return Some(AccessGateKind::GeoRestriction);
    }
    if looks_like_login_gate(ctx, lower_html, img_count, chapter_count, state_script) {
        signals.push("login_redirect_or_password_form".to_owned());
        return Some(if ctx.session == SessionPresence::WithCookies {
            AccessGateKind::SessionExpired
        } else {
            AccessGateKind::LoginRequired
        });
    }
    None
}

fn looks_like_verification_gate(lower_html: &str, img_count: usize, chapter_count: usize) -> bool {
    if img_count >= 2 || chapter_count >= 2 {
        return false;
    }
    lower_html.contains("cf-browser-verification")
        || lower_html.contains("cf_chl_opt")
        || lower_html.contains("just a moment...")
        || lower_html.contains("verify you are human")
        || lower_html.contains("ddos-guard")
        || lower_html.contains("cf-turnstile")
        || (lower_html.contains("g-recaptcha") && lower_html.contains("challenge"))
}

fn looks_like_drm_imgx(lower_html: &str) -> bool {
    let has_imgx = (lower_html.contains("t.truyen.moe/taint.png") || lower_html.contains("imgx"))
        && (lower_html.contains("taint") || lower_html.contains("storagekey"));
    let has_drm = lower_html.contains("widevine")
        || lower_html.contains("encrypted media extensions")
        || lower_html.contains("drm protected");
    has_imgx || has_drm
}

fn looks_like_paywall(
    lower_html: &str,
    img_count: usize,
    chapter_count: usize,
    state_script: EmbeddedStatePresence,
) -> bool {
    if img_count > 0 || chapter_count >= 2 || state_script == EmbeddedStatePresence::Present {
        return false;
    }
    let markers = [
        "chapter is locked",
        "this chapter requires a subscription",
        "unlock this chapter",
        "purchase this chapter",
        "mở khóa chương",
        "mua chương",
        "nạp xu để đọc",
        "dành riêng cho thành viên vip",
        "vip only chapter",
        "subscribe to read",
    ];
    markers.iter().any(|m| lower_html.contains(m))
}

fn looks_like_geo_block(lower_html: &str, img_count: usize, chapter_count: usize) -> bool {
    if img_count > 0 || chapter_count > 0 {
        return false;
    }
    [
        "not available in your country",
        "not available in your region",
        "unavailable in your location",
        "content is geo-blocked",
        "giới hạn tại quốc gia của bạn",
        "không khả dụng tại khu vực",
    ]
    .iter()
    .any(|m| lower_html.contains(m))
}

fn looks_like_login_gate(
    ctx: PageInspectInput<'_>,
    lower_html: &str,
    img_count: usize,
    chapter_count: usize,
    state_script: EmbeddedStatePresence,
) -> bool {
    if img_count > 0 || chapter_count >= 2 || state_script == EmbeddedStatePresence::Present {
        return false;
    }
    let submitted_lower = ctx.submitted_url.path().to_ascii_lowercase();
    let final_lower = ctx.final_url.path().to_ascii_lowercase();
    if !is_login_path(&submitted_lower) && is_login_path(&final_lower) {
        return true;
    }
    let has_pw_input = ctx.doc.select(&PASSWORD_INPUT_SEL).next().is_some();
    let mentions_login_gate = lower_html.contains("please log in")
        || lower_html.contains("sign in to continue")
        || lower_html.contains("login to read")
        || lower_html.contains("đăng nhập để đọc")
        || lower_html.contains("vui lòng đăng nhập");
    (mentions_login_gate || is_login_path(&final_lower)) && has_pw_input
        || (mentions_login_gate && lower_html.len() < 30_000)
}

fn is_login_path(path_lower: &str) -> bool {
    path_lower.contains("/login")
        || path_lower.contains("/signin")
        || path_lower.contains("/sign-in")
        || path_lower.contains("/dang-nhap")
        || path_lower.contains("/account/login")
}

fn classify_non_reader_card_grid(
    s: &ComicSignalsInput<'_>,
    signals: &mut Vec<String>,
) -> Option<(UrlKind, f32)> {
    if s.has_reader_box || s.has_chapter_nav {
        return None;
    }
    if matches!(
        s.url_kind,
        UrlKind::WebsiteHome | UrlKind::CatalogPage | UrlKind::SearchPage
    ) && s.catalog_cards >= 2
    {
        signals.push(format!("catalog_cards={}", s.catalog_cards));
        return Some((s.url_kind, 0.85));
    }
    if s.url_kind != UrlKind::ChapterPage && s.chapter_count >= 2 && s.catalog_cards >= s.img_count
    {
        signals.push(format!("chapter_links={}", s.chapter_count));
        let conf = if s.chapter_count >= 3 { 0.92 } else { 0.78 };
        return Some((UrlKind::MangaDetail, conf));
    }
    if s.url_kind != UrlKind::ChapterPage
        && s.chapter_count == 0
        && s.catalog_cards >= 4
        && s.catalog_cards >= s.img_count
    {
        signals.push(format!("catalog_cards={}", s.catalog_cards));
        let kind = if s.final_url.path() == "/" {
            UrlKind::WebsiteHome
        } else {
            UrlKind::CatalogPage
        };
        return Some((kind, 0.84));
    }
    None
}

fn classify_comic_page_kind(
    s: &ComicSignalsInput<'_>,
    signals: &mut Vec<String>,
) -> (UrlKind, f32) {
    if let Some(hit) = classify_non_reader_card_grid(s, signals) {
        return hit;
    }

    if s.img_count >= 3 {
        signals.push(format!("dom_or_script_images={}", s.img_count));
        let conf = if s.has_chapter_nav || s.has_reader_box || s.url_kind == UrlKind::ChapterPage {
            0.95
        } else {
            0.84
        };
        return (UrlKind::ChapterPage, conf);
    }

    if s.img_count >= 1
        && (s.url_kind == UrlKind::ChapterPage || s.has_reader_box || s.has_chapter_nav)
        && s.chapter_count == 0
    {
        signals.push(format!("chapter_context_images={}", s.img_count));
        return (UrlKind::ChapterPage, 0.78);
    }

    if s.chapter_count > 0 {
        signals.push(format!("chapter_links={}", s.chapter_count));
        let conf = if s.chapter_count >= 3 { 0.92 } else { 0.75 };
        return (UrlKind::MangaDetail, conf);
    }

    if s.url_kind == UrlKind::ChapterPage && (s.has_reader_box || s.has_state_script || s.js_shell)
    {
        signals.push("chapter_url_with_dynamic_or_empty_reader".to_owned());
        return (UrlKind::ChapterPage, 0.72);
    }

    if s.catalog_cards >= 4
        || (matches!(
            s.url_kind,
            UrlKind::WebsiteHome | UrlKind::CatalogPage | UrlKind::SearchPage
        ) && s.catalog_cards >= 2)
    {
        signals.push(format!("catalog_cards={}", s.catalog_cards));
        return (s.url_kind, 0.85);
    }

    let has_comic_terms = s.lower_html.contains("manga")
        || s.lower_html.contains("manhwa")
        || s.lower_html.contains("manhua")
        || s.lower_html.contains("webtoon")
        || s.lower_html.contains("truyện tranh")
        || s.lower_html.contains("chap ");

    let prose_len: usize = s
        .doc
        .select(&TEXT_COUNT_SEL)
        .filter(|el| !is_inside_chrome(*el))
        .map(|el| clean_text(&el.text().collect::<String>()).len())
        .sum();
    let path_lower = s.final_url.path().to_ascii_lowercase();
    let non_comic_path = [
        "/blog/",
        "/news/",
        "/article/",
        "/product/",
        "/shop/",
        "/privacy",
        "/terms",
    ]
    .iter()
    .any(|p| path_lower.contains(p));

    if !has_comic_terms
        && !s.has_state_script
        && !s.has_reader_box
        && (non_comic_path || prose_len >= 120)
    {
        signals.push(format!("unrelated_prose_len={prose_len}"));
        return (UrlKind::NotAComicPage, 0.86);
    }

    if s.js_shell {
        return (s.url_kind, 0.45);
    }

    signals.push("insufficient_comic_structure".to_owned());
    (UrlKind::UnsupportedUrl, 0.2)
}

fn extract_metadata_hints(
    doc: &Html,
    base: &Url,
    lower_html: &str,
    img_count: usize,
) -> PageMetadataHints {
    let title = extract_title(doc);
    let canonical_url = doc.select(&META_TYPE_SEL).find_map(|el| {
        if el.value().name() == "link" {
            el.value().attr("href").and_then(|h| abs_url(base, h))
        } else {
            None
        }
    });

    let reading_direction = if lower_html.contains("webtoon")
        || lower_html.contains("long-strip")
        || lower_html.contains("manhwa")
    {
        Some(ReadingDirectionHint::Webtoon)
    } else if lower_html.contains("dir=\"rtl\"") || lower_html.contains("manga") {
        Some(ReadingDirectionHint::Rtl)
    } else {
        Some(ReadingDirectionHint::Ltr)
    };

    let chapter_title = title
        .as_ref()
        .filter(|t| extract_chapter_number(t).is_some())
        .cloned();
    let series_title = if chapter_title.is_some() { None } else { title };

    PageMetadataHints {
        series_title,
        chapter_title,
        expected_page_count: (img_count > 0).then_some(img_count),
        reading_direction,
        canonical_url,
    }
}

/// Checks if a DOM element belongs to a high-signal comic reader container.
#[must_use]
pub fn is_inside_reader_container(el: ElementRef<'_>) -> bool {
    for anc in el.ancestors().filter_map(ElementRef::wrap) {
        let mut buf = String::new();
        if let Some(id) = anc.value().attr("id") {
            buf.push_str(id);
            buf.push(' ');
        }
        if let Some(cls) = anc.value().attr("class") {
            buf.push_str(cls);
        }
        let lower = buf.to_ascii_lowercase();
        if lower.contains("reading-content")
            || lower.contains("readerarea")
            || lower.contains("chapter_content")
            || lower.contains("chapter-content")
            || lower.contains("page-break")
            || lower.contains("comic-viewer")
        {
            return true;
        }
    }
    false
}
