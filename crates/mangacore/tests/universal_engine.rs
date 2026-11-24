//! Synthetic unit and integration tests for the Gt-Maga Universal Source Compatibility,
//! Adaptive Extraction (Strategies A/C/D), Binary Image Resolution, Panic Isolation, and
//! 27-Code Failure Classification Engine.
//!
//! All HTML, JSON, image bytes, and HTTP fixtures below are 100% synthetic.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use async_trait::async_trait;
use mangacore::detect::{
    AccessGateKind, PageInspectInput, ReadingDirectionHint, SessionPresence, analyze_html_page,
};
use mangacore::error::ScrapeError;
use mangacore::extract::{
    HtmlMinimumPolicy, extract_chapter_html_adaptive, extract_structured_chapter,
    extract_structured_manga, is_unsupported_api_payload,
};
use mangacore::http::{HttpClient, HttpPolicy, ProxyConfig, ProxyEndpoint, ProxyProtocol};
use mangacore::model::{Chapter, Resolved};
use mangacore::resolver::{DetectedImageFormat, ImageValidationError, inspect_image_bytes};
use mangacore::{
    ExtractionStrategyKind, HeadlessPolicy, Registry, Source, SourceErrorCode, UrlKind,
    describe_error_code, redact_url_for_diagnostics,
};
use scraper::Html;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use url::Url;

fn parse_url(raw: &str) -> Url {
    Url::parse(raw).expect("valid test URL")
}

fn minimal_valid_png(width: u32, height: u32) -> Vec<u8> {
    let mut out = Vec::with_capacity(33);
    out.extend_from_slice(&[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]);
    out.extend_from_slice(&13_u32.to_be_bytes());
    out.extend_from_slice(b"IHDR");
    out.extend_from_slice(&width.to_be_bytes());
    out.extend_from_slice(&height.to_be_bytes());
    out.extend_from_slice(&[8, 2, 0, 0, 0]);
    out.extend_from_slice(&[0_u8; 4]);
    out
}

fn minimal_valid_jpeg(width: u16, height: u16) -> Vec<u8> {
    let mut out = Vec::new();
    // SOI
    out.extend_from_slice(&[0xFF, 0xD8]);
    // SOF0 marker (length 11: 2 len + 1 precision + 2 height + 2 width + 4 tail)
    out.extend_from_slice(&[0xFF, 0xC0, 0x00, 0x0B, 0x08]);
    out.extend_from_slice(&height.to_be_bytes());
    out.extend_from_slice(&width.to_be_bytes());
    out.extend_from_slice(&[0x03, 0x01, 0x11, 0x00]);
    // SOS marker + >= 32 total bytes + EOI
    out.extend_from_slice(&[0xFF, 0xDA, 0x00, 0x08, 0x01, 0x01, 0x00, 0x00, 0x3F, 0x00]);
    out.extend_from_slice(&[0xAA_u8; 12]);
    out.extend_from_slice(&[0xFF, 0xD9]);
    out
}

fn minimal_webp_vp8(width: u16, height: u16) -> Vec<u8> {
    let mut out = Vec::with_capacity(30);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&22_u32.to_le_bytes());
    out.extend_from_slice(b"WEBP");
    out.extend_from_slice(b"VP8 ");
    out.extend_from_slice(&10_u32.to_le_bytes());
    out.extend_from_slice(&[0x10, 0x00, 0x00]); // 3-byte frame tag
    out.extend_from_slice(&[0x9D, 0x01, 0x2A]); // VP8 start code
    out.extend_from_slice(&(width & 0x3FFF).to_le_bytes());
    out.extend_from_slice(&(height & 0x3FFF).to_le_bytes());
    out
}

fn minimal_webp_vp8l(width: u32, height: u32) -> Vec<u8> {
    let packed = (width - 1) | ((height - 1) << 14);
    let mut out = Vec::with_capacity(26);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&18_u32.to_le_bytes());
    out.extend_from_slice(b"WEBP");
    out.extend_from_slice(b"VP8L");
    out.extend_from_slice(&6_u32.to_le_bytes());
    out.push(0x2F);
    out.extend_from_slice(&packed.to_le_bytes());
    out.push(0x00);
    out
}

fn minimal_webp_vp8x(width: u32, height: u32) -> Vec<u8> {
    let w_minus = (width - 1).to_le_bytes();
    let h_minus = (height - 1).to_le_bytes();
    let mut out = Vec::with_capacity(30);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&22_u32.to_le_bytes());
    out.extend_from_slice(b"WEBP");
    out.extend_from_slice(b"VP8X");
    out.extend_from_slice(&10_u32.to_le_bytes());
    out.extend_from_slice(&[0_u8; 4]); // flags
    out.extend_from_slice(&w_minus[..3]);
    out.extend_from_slice(&h_minus[..3]);
    out
}

fn minimal_gif(width: u16, height: u16) -> Vec<u8> {
    let mut out = Vec::with_capacity(14);
    out.extend_from_slice(b"GIF89a");
    out.extend_from_slice(&width.to_le_bytes());
    out.extend_from_slice(&height.to_le_bytes());
    out.extend_from_slice(&[0xF7, 0x00, 0x00, 0x00]);
    out
}

fn minimal_bmp(width: u32, height: u32) -> Vec<u8> {
    let mut out = vec![0_u8; 26];
    out[0..2].copy_from_slice(b"BM");
    out[14..18].copy_from_slice(&40_u32.to_le_bytes());
    out[18..22].copy_from_slice(&width.to_le_bytes());
    out[22..26].copy_from_slice(&height.to_le_bytes());
    out
}

fn minimal_avif(width: u32, height: u32) -> Vec<u8> {
    let mut ftyp = Vec::new();
    ftyp.extend_from_slice(&16_u32.to_be_bytes());
    ftyp.extend_from_slice(b"ftyp");
    ftyp.extend_from_slice(b"avif");
    ftyp.extend_from_slice(&0_u32.to_be_bytes());

    let mut ispe = Vec::new();
    ispe.extend_from_slice(&20_u32.to_be_bytes());
    ispe.extend_from_slice(b"ispe");
    ispe.extend_from_slice(&0_u32.to_be_bytes()); // fullbox version/flags
    ispe.extend_from_slice(&width.to_be_bytes());
    ispe.extend_from_slice(&height.to_be_bytes());

    let ipco_len = u32::try_from(8 + ispe.len()).expect("ipco len fits u32");
    let mut ipco = Vec::new();
    ipco.extend_from_slice(&ipco_len.to_be_bytes());
    ipco.extend_from_slice(b"ipco");
    ipco.extend_from_slice(&ispe);

    let iprp_len = u32::try_from(8 + ipco.len()).expect("iprp len fits u32");
    let mut iprp = Vec::new();
    iprp.extend_from_slice(&iprp_len.to_be_bytes());
    iprp.extend_from_slice(b"iprp");
    iprp.extend_from_slice(&ipco);

    let meta_len = u32::try_from(12 + iprp.len()).expect("meta len fits u32");
    let mut meta = Vec::new();
    meta.extend_from_slice(&meta_len.to_be_bytes());
    meta.extend_from_slice(b"meta");
    meta.extend_from_slice(&0_u32.to_be_bytes()); // fullbox version/flags
    meta.extend_from_slice(&iprp);

    let mut out = ftyp;
    out.extend_from_slice(&meta);
    out
}

#[test]
fn error_taxonomy_covers_all_27_codes_and_stays_compact() {
    let all = SourceErrorCode::all();
    assert_eq!(all.len(), 27, "taxonomy must expose exactly 27 error codes");
    for code in all {
        let desc = describe_error_code(*code);
        assert!(
            !desc.title_vi.trim().is_empty(),
            "title_vi missing for {code:?}"
        );
        assert!(
            !desc.explanation_vi.trim().is_empty(),
            "explanation_vi missing for {code:?}"
        );
        assert!(
            !desc.title_en.trim().is_empty(),
            "title_en missing for {code:?}"
        );
        assert!(
            !desc.explanation_en.trim().is_empty(),
            "explanation_en missing for {code:?}"
        );
        assert!(
            !desc.suggested_action_vi.trim().is_empty(),
            "suggested_action_vi missing for {code:?}"
        );
        assert!(
            !desc.suggested_action_en.trim().is_empty(),
            "suggested_action_en missing for {code:?}"
        );
    }
    assert!(
        std::mem::size_of::<ScrapeError>() <= 128,
        "ScrapeError exceeded clippy::result_large_err limit: {} bytes",
        std::mem::size_of::<ScrapeError>()
    );
}

#[test]
fn redacts_secret_query_parameters_and_userinfo() {
    let raw = "https://reader:s3cr3t@cdn.example.com/ch1?page=2&token=abc123&sig=xyz999&jwt=hdr.pay.sig&session=sess42&view=webtoon";
    let redacted = redact_url_for_diagnostics(raw);
    assert!(!redacted.contains("reader"), "username must be redacted");
    assert!(!redacted.contains("s3cr3t"), "password must be redacted");
    assert!(!redacted.contains("abc123"), "token value must be redacted");
    assert!(!redacted.contains("xyz999"), "sig value must be redacted");
    assert!(
        !redacted.contains("hdr.pay.sig"),
        "jwt value must be redacted"
    );
    assert!(
        !redacted.contains("sess42"),
        "session value must be redacted"
    );
    assert!(redacted.contains("page=2"), "benign page param must stay");
    assert!(
        redacted.contains("view=webtoon"),
        "benign view param must stay"
    );
    assert!(
        redacted.contains("token=REDACTED") || redacted.contains("REDACTED"),
        "redaction placeholder must be present"
    );
}

#[test]
fn universal_page_detector_identifies_comic_and_non_comic_pages() {
    let cards_html = r#"
      <html><head><title>DemoComics</title></head><body>
      <main>
        <a href="/series/alpha"><img src="https://cdn.example.com/c1.jpg" /></a>
        <a href="/series/beta"><img src="https://cdn.example.com/c2.jpg" /></a>
        <a href="/series/gamma"><img src="https://cdn.example.com/c3.jpg" /></a>
        <a href="/series/delta"><img src="https://cdn.example.com/c4.jpg" /></a>
      </main></body></html>
    "#;
    let home_url = parse_url("https://demo-comics.example.com/");
    let home_doc = Html::parse_document(cards_html);
    let home = analyze_html_page(PageInspectInput {
        submitted_url: &home_url,
        final_url: &home_url,
        status: 200,
        content_type: Some("text/html"),
        html: cards_html,
        doc: &home_doc,
        session: SessionPresence::NoCookies,
    });
    assert_eq!(home.page_kind, UrlKind::WebsiteHome);
    assert!(home.confidence >= 0.8, "homepage confidence must be >= 0.8");

    let catalog_url = parse_url("https://demo-comics.example.com/catalog?page=2");
    let catalog = analyze_html_page(PageInspectInput {
        submitted_url: &catalog_url,
        final_url: &catalog_url,
        status: 200,
        content_type: Some("text/html"),
        html: cards_html,
        doc: &home_doc,
        session: SessionPresence::NoCookies,
    });
    assert_eq!(catalog.page_kind, UrlKind::CatalogPage);

    let search_url = parse_url("https://demo-comics.example.com/search?q=knight");
    let search = analyze_html_page(PageInspectInput {
        submitted_url: &search_url,
        final_url: &search_url,
        status: 200,
        content_type: Some("text/html"),
        html: cards_html,
        doc: &home_doc,
        session: SessionPresence::NoCookies,
    });
    assert_eq!(search.page_kind, UrlKind::SearchPage);
}

#[test]
fn universal_page_detector_identifies_series_chapter_and_non_comic() {
    let series_html = r#"
      <html><head><title>Nebula Chronicles</title></head><body>
        <h1>Nebula Chronicles</h1>
        <div class="chapters">
          <a href="/series/nebula/chapter-1">Chapter 1</a>
          <a href="/series/nebula/chapter-2">Chapter 2</a>
          <a href="/series/nebula/chapter-3">Chapter 3</a>
        </div>
      </body></html>
    "#;
    let series_url = parse_url("https://demo-comics.example.com/series/nebula");
    let series_doc = Html::parse_document(series_html);
    let series = analyze_html_page(PageInspectInput {
        submitted_url: &series_url,
        final_url: &series_url,
        status: 200,
        content_type: Some("text/html"),
        html: series_html,
        doc: &series_doc,
        session: SessionPresence::NoCookies,
    });
    assert_eq!(series.page_kind, UrlKind::MangaDetail);

    let chapter_html = r#"
      <html><head><title>Nebula Chronicles Chapter 1</title></head><body>
        <main class="reading-content">
          <img src="https://img.example.com/01.jpg" />
          <img src="https://img.example.com/02.jpg" />
          <img src="https://img.example.com/03.jpg" />
        </main>
        <p>Long-strip webtoon mode</p>
      </body></html>
    "#;
    let chapter_url = parse_url("https://demo-comics.example.com/series/nebula/chapter-1");
    let chapter_doc = Html::parse_document(chapter_html);
    let chapter = analyze_html_page(PageInspectInput {
        submitted_url: &chapter_url,
        final_url: &chapter_url,
        status: 200,
        content_type: Some("text/html"),
        html: chapter_html,
        doc: &chapter_doc,
        session: SessionPresence::NoCookies,
    });
    assert_eq!(chapter.page_kind, UrlKind::ChapterPage);
    assert_eq!(
        chapter.metadata.reading_direction,
        Some(ReadingDirectionHint::Webtoon)
    );
    assert_eq!(chapter.metadata.expected_page_count, Some(3));

    let blog_html = r"
      <html><head><title>Coffee Brewing Tips</title></head><body>
      <article>
        <p>Grind beans fresh before pouring hot water over the filter paper for a clean balanced espresso and pour-over extraction every single morning without bitterness.</p>
      </article>
      </body></html>
    ";
    let blog_url = parse_url("https://coffee-notes.example.com/blog/brew-guide");
    let blog_doc = Html::parse_document(blog_html);
    let blog = analyze_html_page(PageInspectInput {
        submitted_url: &blog_url,
        final_url: &blog_url,
        status: 200,
        content_type: Some("text/html"),
        html: blog_html,
        doc: &blog_doc,
        session: SessionPresence::NoCookies,
    });
    assert_eq!(blog.page_kind, UrlKind::NotAComicPage);
}

#[test]
fn universal_page_detector_identifies_access_gates_and_spa_shells() {
    let chapter_url = parse_url("https://demo-comics.example.com/series/nebula/chapter-1");

    let spa_html =
        r#"<html><body><div id="__next"></div><script src="/_next/app.js"></script></body></html>"#;
    let spa_doc = Html::parse_document(spa_html);
    let spa = analyze_html_page(PageInspectInput {
        submitted_url: &chapter_url,
        final_url: &chapter_url,
        status: 200,
        content_type: Some("text/html"),
        html: spa_html,
        doc: &spa_doc,
        session: SessionPresence::NoCookies,
    });
    assert!(spa.js_shell_detected, "empty SPA root must be detected");

    let login_html = r#"<html><body><form action="/login"><input type="password" name="pw" /><p>Please log in to continue</p></form></body></html>"#;
    let login_final = parse_url("https://demo-comics.example.com/login");
    let login_doc = Html::parse_document(login_html);
    let login = analyze_html_page(PageInspectInput {
        submitted_url: &chapter_url,
        final_url: &login_final,
        status: 200,
        content_type: Some("text/html"),
        html: login_html,
        doc: &login_doc,
        session: SessionPresence::NoCookies,
    });
    assert_eq!(login.access_gate, Some(AccessGateKind::LoginRequired));
    assert_eq!(login.page_kind, UrlKind::LoginPage);

    let expired = analyze_html_page(PageInspectInput {
        submitted_url: &chapter_url,
        final_url: &login_final,
        status: 200,
        content_type: Some("text/html"),
        html: login_html,
        doc: &login_doc,
        session: SessionPresence::WithCookies,
    });
    assert_eq!(expired.access_gate, Some(AccessGateKind::SessionExpired));

    let verify_html = r#"<html><head><title>Just a moment...</title></head><body><div id="cf-turnstile">Verify you are human</div></body></html>"#;
    let verify_doc = Html::parse_document(verify_html);
    let verify = analyze_html_page(PageInspectInput {
        submitted_url: &chapter_url,
        final_url: &chapter_url,
        status: 403,
        content_type: Some("text/html"),
        html: verify_html,
        doc: &verify_doc,
        session: SessionPresence::NoCookies,
    });
    assert_eq!(verify.access_gate, Some(AccessGateKind::Verification));
    assert_eq!(verify.page_kind, UrlKind::VerificationPage);

    let paywall_html = r#"<html><body><div class="locked">This chapter requires a subscription. Unlock this chapter with coins.</div></body></html>"#;
    let paywall_doc = Html::parse_document(paywall_html);
    let paywall = analyze_html_page(PageInspectInput {
        submitted_url: &chapter_url,
        final_url: &chapter_url,
        status: 402,
        content_type: Some("text/html"),
        html: paywall_html,
        doc: &paywall_doc,
        session: SessionPresence::NoCookies,
    });
    assert_eq!(paywall.access_gate, Some(AccessGateKind::Paywall));
    assert_eq!(paywall.page_kind, UrlKind::PaywallPage);

    let geo_html =
        "<html><body><p>Sorry, this title is not available in your region.</p></body></html>";
    let geo_doc = Html::parse_document(geo_html);
    let geo = analyze_html_page(PageInspectInput {
        submitted_url: &chapter_url,
        final_url: &chapter_url,
        status: 451,
        content_type: Some("text/html"),
        html: geo_html,
        doc: &geo_doc,
        session: SessionPresence::NoCookies,
    });
    assert_eq!(geo.access_gate, Some(AccessGateKind::GeoRestriction));

    let drm_html = r#"<html><body><canvas id="page" data-imgx="t.truyen.moe/taint.png" data-storagekey="abc"></canvas></body></html>"#;
    let drm_doc = Html::parse_document(drm_html);
    let drm = analyze_html_page(PageInspectInput {
        submitted_url: &chapter_url,
        final_url: &chapter_url,
        status: 200,
        content_type: Some("text/html"),
        html: drm_html,
        doc: &drm_doc,
        session: SessionPresence::NoCookies,
    });
    assert_eq!(drm.access_gate, Some(AccessGateKind::DrmProtected));
}

#[test]
fn strategy_a_handles_picture_srcset_css_bg_entities_and_single_webtoon() {
    let html = r#"
      <html><body>
        <main class="reading-content">
          <picture>
            <source srcset="https://cdn.example.com/p1-small.webp 400w, https://cdn.example.com/p1-full.webp?token=1&amp;v=2 1200w" />
            <img src="https://cdn.example.com/p1-small.webp" />
          </picture>
          <div class="page-item" style="background-image: url('https://cdn.example.com/p2.webp?a=1&#38;b=2');"></div>
          <img data-src="https://cdn.example.com/p3.webp" />
        </main>
      </body></html>
    "#;
    let url = parse_url("https://demo.example.com/series/alpha/chapter-1");
    let doc = Html::parse_document(html);
    let out = extract_chapter_html_adaptive(&doc, &url, &url, HtmlMinimumPolicy::StandardThree);
    let ch = out.chapter.expect("3 pages extracted cleanly");
    assert_eq!(ch.images.len(), 3);
    assert_eq!(
        ch.images[0].url.as_str(),
        "https://cdn.example.com/p1-full.webp?token=1&v=2"
    );
    assert_eq!(
        ch.images[1].url.as_str(),
        "https://cdn.example.com/p2.webp?a=1&b=2"
    );
    assert_eq!(ch.images[2].url.as_str(), "https://cdn.example.com/p3.webp");

    // 1-page long-strip chapter: rejected under StandardThree, accepted under ConfidentChapterOneOrMore.
    let single_html = r#"
      <html><body>
        <main class="reading-content">
          <img src="https://cdn.example.com/oneshot-long-strip.png" />
        </main>
      </body></html>
    "#;
    let single_doc = Html::parse_document(single_html);
    let rejected =
        extract_chapter_html_adaptive(&single_doc, &url, &url, HtmlMinimumPolicy::StandardThree);
    assert!(
        rejected.chapter.is_none(),
        "StandardThree must reject single-image generic HTML"
    );
    let accepted = extract_chapter_html_adaptive(
        &single_doc,
        &url,
        &url,
        HtmlMinimumPolicy::ConfidentChapterOneOrMore,
    );
    assert_eq!(
        accepted
            .chapter
            .expect("1-page confident chapter")
            .images
            .len(),
        1
    );
}

#[test]
fn strategy_c_extracts_next_data_ts_reader_madara_json_ld_and_flags_protobuf() {
    let ch_url = parse_url("https://demo.example.com/read/series/chapter-4");
    let series_url = parse_url("https://demo.example.com/series/nebula");

    // 1. ts_reader.run
    let ts_html = r#"
      <html><head><title>Nebula Chapter 4</title></head><body>
      <script>ts_reader.run({"prevUrl":"/read/series/chapter-3","nextUrl":"/read/series/chapter-5","sources":[{"source":"Server1","images":["https://cdn.example.com/4/01.jpg","https://cdn.example.com/4/02.jpg","https://cdn.example.com/4/03.jpg"]}]});</script>
      </body></html>
    "#;
    let ts_doc = Html::parse_document(ts_html);
    let ts = extract_structured_chapter(&ts_doc, ts_html, &ch_url, &ch_url)
        .expect("ts_reader chapter must extract");
    assert_eq!(ts.source_tag, "ts_reader");
    assert_eq!(ts.chapter.images.len(), 3);
    assert_eq!(
        ts.chapter.prev.as_ref().map(Url::as_str),
        Some("https://demo.example.com/read/series/chapter-3")
    );
    assert_eq!(
        ts.chapter.next.as_ref().map(Url::as_str),
        Some("https://demo.example.com/read/series/chapter-5")
    );

    // 2. Madara chapter_preloaded_images
    let madara_html = r#"
      <html><body><script>var chapter_preloaded_images = ["https://cdn.example.com/m/1.jpg", "https://cdn.example.com/m/2.jpg"];</script></body></html>
    "#;
    let madara_doc = Html::parse_document(madara_html);
    let madara = extract_structured_chapter(&madara_doc, madara_html, &ch_url, &ch_url)
        .expect("madara chapter must extract");
    assert_eq!(madara.source_tag, "madara_preloaded");
    assert_eq!(madara.chapter.images.len(), 2);

    // 3. #__NEXT_DATA__ chapter + series
    let next_ch_html = r#"
      <html><body>
      <script id="__NEXT_DATA__" type="application/json">{"props":{"pageProps":{"chapter":{"name":"Ch 4","pages":["https://cdn.example.com/n/1.webp","https://cdn.example.com/n/2.webp"]}}}}</script>
      </body></html>
    "#;
    let next_ch_doc = Html::parse_document(next_ch_html);
    let next_ch = extract_structured_chapter(&next_ch_doc, next_ch_html, &ch_url, &ch_url)
        .expect("next_data chapter must extract");
    assert_eq!(next_ch.source_tag, "next_data");
    assert_eq!(next_ch.chapter.images.len(), 2);

    let next_manga_html = r#"
      <html><body>
      <script id="__NEXT_DATA__" type="application/json">{"props":{"pageProps":{"manga":{"title":"Next Series","chapters":[{"name":"Chapter 1","url":"/series/nebula/ch-1"},{"name":"Chapter 2","url":"/series/nebula/ch-2"}]}}}}</script>
      </body></html>
    "#;
    let next_manga_doc = Html::parse_document(next_manga_html);
    let manga = extract_structured_manga(&next_manga_doc, &series_url, &series_url)
        .expect("next_data manga must extract");
    assert_eq!(manga.title, "Next Series");
    assert_eq!(manga.chapters.len(), 2);

    // 4. JSON-LD ComicIssue / ComicSeries
    let ld_ch_html = r#"
      <html><body>
      <script type="application/ld+json">{"@type":"ComicIssue","name":"Episode 4","associatedMedia":[{"contentUrl":"https://cdn.example.com/ld/1.png"},{"contentUrl":"https://cdn.example.com/ld/2.png"}]}</script>
      </body></html>
    "#;
    let ld_ch_doc = Html::parse_document(ld_ch_html);
    let ld_ch = extract_structured_chapter(&ld_ch_doc, ld_ch_html, &ch_url, &ch_url)
        .expect("json-ld ComicIssue must extract");
    assert_eq!(ld_ch.source_tag, "json_ld");
    assert_eq!(ld_ch.chapter.images.len(), 2);

    // 5. Binary / protobuf API detection
    assert!(
        is_unsupported_api_payload(Some("application/x-protobuf"), "raw"),
        "protobuf content type must be flagged"
    );
    assert!(
        is_unsupported_api_payload(Some("application/json"), "<html>not json</html>"),
        "HTML disguised as JSON must be flagged"
    );
    assert!(
        is_unsupported_api_payload(None, "\x01\x02\x03\x04\x05"),
        "binary control bytes must be flagged"
    );
    assert!(
        !is_unsupported_api_payload(Some("application/json"), "{\"ok\":true}"),
        "valid JSON must not be flagged"
    );
}

#[derive(Debug)]
struct OutdatedDemoAdapter;

#[async_trait]
impl Source for OutdatedDemoAdapter {
    fn id(&self) -> &'static str {
        "outdated-demo"
    }

    fn matches(&self, url: &Url) -> bool {
        url.host_str() == Some("mock-fallback.example.com")
    }

    async fn resolve(&self, _http: &HttpClient, url: &Url) -> Result<Resolved, ScrapeError> {
        Err(ScrapeError::MissingImages { url: url.clone() })
    }

    async fn chapter(&self, _http: &HttpClient, url: &Url) -> Result<Chapter, ScrapeError> {
        Err(ScrapeError::MissingImages { url: url.clone() })
    }
}

#[derive(Debug)]
struct PanickingAdapter;

#[async_trait]
impl Source for PanickingAdapter {
    fn id(&self) -> &'static str {
        "panic-demo"
    }

    fn matches(&self, url: &Url) -> bool {
        url.host_str() == Some("mock-panic.example.com")
    }

    async fn resolve(&self, _http: &HttpClient, _url: &Url) -> Result<Resolved, ScrapeError> {
        panic!("simulated unexpected DOM panic in adapter")
    }

    async fn chapter(&self, _http: &HttpClient, _url: &Url) -> Result<Chapter, ScrapeError> {
        panic!("simulated chapter panic in adapter")
    }
}

async fn spawn_mock_http_proxy(
    responses: Vec<(&'static str, &'static str, &'static str, &'static str)>,
) -> (HttpClient, tokio::task::JoinHandle<()>, Arc<AtomicUsize>) {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind local mock proxy port");
    let addr = listener.local_addr().expect("read mock proxy local_addr");
    let hits = Arc::new(AtomicUsize::new(0));
    let hits_srv = Arc::clone(&hits);

    let handle = tokio::spawn(async move {
        while let Ok((mut sock, _)) = listener.accept().await {
            hits_srv.fetch_add(1, Ordering::Relaxed);
            let mut buf = [0_u8; 4096];
            let n = sock.read(&mut buf).await.unwrap_or(0);
            let req = String::from_utf8_lossy(&buf[..n]);
            let (status, ct, body) = responses
                .iter()
                .find(|(needle, _, _, _)| req.contains(needle))
                .map_or(
                    ("404 Not Found", "text/html", "<h1>404</h1>"),
                    |(_, s, c, b)| (*s, *c, *b),
                );
            let resp = format!(
                "HTTP/1.1 {status}\r\nContent-Type: {ct}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = sock.write_all(resp.as_bytes()).await;
            let _ = sock.flush().await;
        }
    });

    let ep = ProxyEndpoint {
        protocol: ProxyProtocol::Http,
        host: "127.0.0.1".to_owned(),
        port: addr.port(),
        username: None,
        password: None,
    };
    let client = HttpClient::with_policy(HttpPolicy {
        proxy: ProxyConfig {
            enabled: true,
            default_proxy: ep,
            allow_direct_fallback: false,
            domain_rules: Vec::new(),
        },
        ..HttpPolicy::default()
    })
    .expect("construct mock proxy HttpClient");

    (client, handle, hits)
}

#[tokio::test]
async fn orchestrator_falls_back_from_outdated_adapter_and_isolates_panics_and_js_shells() {
    let body_structured = r#"<!doctype html><html><head><title>Demo Chapter 7</title></head><body><script>ts_reader.run({"sources":[{"images":["https://cdn.example.com/7/01.jpg","https://cdn.example.com/7/02.jpg"]}]});</script></body></html>"#;
    let body_js_shell = r#"<!doctype html><html><head><title>JS Reader</title></head><body><div id="__next"></div><script src="/bundle.js"></script></body></html>"#;

    let (client, server, _hits) = spawn_mock_http_proxy(vec![
        (
            "mock-fallback.example.com",
            "200 OK",
            "text/html; charset=utf-8",
            body_structured,
        ),
        (
            "mock-js.example.com",
            "200 OK",
            "text/html; charset=utf-8",
            body_js_shell,
        ),
    ])
    .await;

    let registry = Registry::with_adapters(
        client,
        vec![Box::new(OutdatedDemoAdapter), Box::new(PanickingAdapter)],
    );

    // 1. Outdated adapter fails -> orchestrator falls back to Strategy C structured extraction!
    let fallback_url = parse_url("http://mock-fallback.example.com/series/demo/chapter-7");
    let (ch, report) = registry
        .chapter_with_diagnostics(&fallback_url, HeadlessPolicy::Disabled)
        .await
        .expect("outdated adapter must recover via structured fallback");
    assert_eq!(ch.images.len(), 2);
    assert_eq!(
        report.selected_strategy,
        Some(ExtractionStrategyKind::StructuredDataAndApi)
    );
    assert!(
        report.evidence.iter().any(|e| e.contains("outdated-demo")),
        "diagnostic report must mention fallback from outdated adapter"
    );

    // 2. Panicking adapter must NOT crash process; surfaces AdapterExecutionFailed cleanly.
    let panic_url = parse_url("http://mock-panic.example.com/series/demo/chapter-1");
    let panic_err = registry
        .chapter_with_diagnostics(&panic_url, HeadlessPolicy::Disabled)
        .await
        .expect_err("panicking adapter must be isolated");
    assert_eq!(
        panic_err.0.error_code(),
        SourceErrorCode::AdapterExecutionFailed
    );
    assert_eq!(
        panic_err.1.error_code,
        Some(SourceErrorCode::AdapterExecutionFailed)
    );

    // 3. JS SPA empty shell with HeadlessPolicy::Disabled surfaces JavascriptRenderRequired.
    let js_url = parse_url("http://mock-js.example.com/series/spa/chapter-2");
    let js_err = registry
        .chapter_with_diagnostics(&js_url, HeadlessPolicy::Disabled)
        .await
        .expect_err("SPA shell without headless must explain JavascriptRenderRequired");
    assert_eq!(
        js_err.0.error_code(),
        SourceErrorCode::JavascriptRenderRequired
    );

    server.abort();
}

#[test]
fn universal_image_validator_checks_all_formats_corruption_and_tracking_pixels() {
    let png = inspect_image_bytes(&minimal_valid_png(800, 1200)).expect("valid PNG");
    assert_eq!(png.format, DetectedImageFormat::Png);
    assert_eq!((png.width, png.height), (800, 1200));

    let jpg = inspect_image_bytes(&minimal_valid_jpeg(900, 1400)).expect("valid JPEG");
    assert_eq!(jpg.format, DetectedImageFormat::Jpeg);
    assert_eq!((jpg.width, jpg.height), (900, 1400));

    let lossy_webp = inspect_image_bytes(&minimal_webp_vp8(720, 1080)).expect("valid VP8 WebP");
    assert_eq!(lossy_webp.format, DetectedImageFormat::Webp);
    assert_eq!((lossy_webp.width, lossy_webp.height), (720, 1080));

    let lossless_webp = inspect_image_bytes(&minimal_webp_vp8l(640, 960)).expect("valid VP8L WebP");
    assert_eq!(lossless_webp.format, DetectedImageFormat::Webp);
    assert_eq!((lossless_webp.width, lossless_webp.height), (640, 960));

    let extended_webp =
        inspect_image_bytes(&minimal_webp_vp8x(1000, 1500)).expect("valid VP8X WebP");
    assert_eq!(extended_webp.format, DetectedImageFormat::Webp);
    assert_eq!((extended_webp.width, extended_webp.height), (1000, 1500));

    let gif = inspect_image_bytes(&minimal_gif(500, 700)).expect("valid GIF");
    assert_eq!(gif.format, DetectedImageFormat::Gif);
    assert_eq!((gif.width, gif.height), (500, 700));

    let bmp = inspect_image_bytes(&minimal_bmp(400, 600)).expect("valid BMP");
    assert_eq!(bmp.format, DetectedImageFormat::Bmp);
    assert_eq!((bmp.width, bmp.height), (400, 600));

    let avif = inspect_image_bytes(&minimal_avif(1080, 1920)).expect("valid AVIF");
    assert_eq!(avif.format, DetectedImageFormat::Avif);
    assert_eq!((avif.width, avif.height), (1080, 1920));

    // Error discriminations
    assert!(
        matches!(
            inspect_image_bytes(&[]),
            Err(ImageValidationError::EmptyPayload)
        ),
        "empty payload must fail with EmptyPayload"
    );
    assert!(
        matches!(
            inspect_image_bytes(&[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00]),
            Err(ImageValidationError::TruncatedOrCorrupted { format: "png", .. })
        ),
        "truncated PNG must fail with TruncatedOrCorrupted"
    );
    let full_jpg = minimal_valid_jpeg(800, 1200);
    assert!(
        matches!(
            inspect_image_bytes(&full_jpg[..18]),
            Err(ImageValidationError::TruncatedOrCorrupted { format: "jpeg", .. })
        ),
        "truncated JPEG must fail with TruncatedOrCorrupted"
    );
    assert!(
        matches!(
            inspect_image_bytes(b"<!doctype html><html><body>Access denied</body></html>"),
            Err(ImageValidationError::UnsupportedFormat { .. })
        ),
        "HTML payload must fail with UnsupportedFormat"
    );
    assert!(
        matches!(
            inspect_image_bytes(br#"<svg xmlns="http://www.w3.org/2000/svg"></svg>"#),
            Err(ImageValidationError::UnsupportedFormat { .. })
        ),
        "SVG payload must fail with UnsupportedFormat"
    );
    assert!(
        matches!(
            inspect_image_bytes(&minimal_valid_png(1, 1)),
            Err(ImageValidationError::DimensionsTooSmall {
                width: 1,
                height: 1
            })
        ),
        "1x1 tracking PNG must fail with DimensionsTooSmall"
    );
}
