//! Synthetic unit and integration tests for URL recognition, diagnostics, and dedicated
//! website adapters (`MangaDex`, `TruyenQQ`, `MoeTruyen`).
//!
//! All HTML and JSON payloads below are strictly synthetic fixtures with invented titles
//! and placeholder URLs — zero copyrighted content.

use mangacore::error::ScrapeError;
use mangacore::recognize::{
    RecognitionStatus, UrlKind, classify_url_pre_fetch, diagnostic_from_error,
};
use mangacore::source::{mangadex, moetruyen, truyenqq};
use url::Url;

fn parse_url(raw: &str) -> Url {
    Url::parse(raw).expect("valid test URL")
}

#[test]
fn url_classification_mangadex_and_generic_kinds() {
    let cases = [
        (
            "https://mangadex.org/",
            Some("mangadex"),
            UrlKind::WebsiteHome,
        ),
        (
            "https://mangadex.org/titles",
            Some("mangadex"),
            UrlKind::CatalogPage,
        ),
        (
            "https://mangadex.org/search?q=sample",
            Some("mangadex"),
            UrlKind::SearchPage,
        ),
        (
            "https://mangadex.org/title/11111111-2222-3333-4444-555555555555/demo-slug",
            Some("mangadex"),
            UrlKind::MangaDetail,
        ),
        (
            "https://mangadex.org/chapter/aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee",
            Some("mangadex"),
            UrlKind::ChapterPage,
        ),
        (
            "https://uploads.mangadex.org/covers/11111111-2222-3333-4444-555555555555/cover.jpg",
            Some("mangadex"),
            UrlKind::ImageUrl,
        ),
        (
            "https://api.mangadex.org/ping",
            Some("mangadex"),
            UrlKind::ApiEndpoint,
        ),
        (
            "https://example.org/assets/page01.webp",
            None,
            UrlKind::ImageUrl,
        ),
    ];

    for (raw, expected_adapter, expected_kind) in cases {
        let info = classify_url_pre_fetch(raw)
            .unwrap_or_else(|diag| panic!("expected valid URL '{raw}', got diag: {diag:?}"));
        assert_eq!(
            info.adapter_id, expected_adapter,
            "adapter mismatch for {raw}"
        );
        assert_eq!(info.url_kind, expected_kind, "kind mismatch for {raw}");
    }
}

#[test]
fn url_classification_truyenqq_and_moetruyen_kinds() {
    let cases = [
        (
            "https://m.truyenqqko.com/",
            Some("truyenqq"),
            UrlKind::WebsiteHome,
        ),
        (
            "https://m.truyenqqko.com/truyen-tranh/",
            Some("truyenqq"),
            UrlKind::CatalogPage,
        ),
        (
            "https://truyenqqko.com/tim-kiem.html?q=sample",
            Some("truyenqq"),
            UrlKind::SearchPage,
        ),
        (
            "https://m.truyenqqko.com/truyen-tranh/synthetic-tale-99999",
            Some("truyenqq"),
            UrlKind::MangaDetail,
        ),
        (
            "https://truyenqqko.com/truyen-tranh/synthetic-tale-99999-chap-12.html",
            Some("truyenqq"),
            UrlKind::ChapterPage,
        ),
        (
            "https://moetruyen.net/",
            Some("moetruyen"),
            UrlKind::WebsiteHome,
        ),
        (
            "https://moetruyen.net/manga/",
            Some("moetruyen"),
            UrlKind::CatalogPage,
        ),
        (
            "https://moetruyen.net/manga?q=sample",
            Some("moetruyen"),
            UrlKind::SearchPage,
        ),
        (
            "https://moetruyen.net/manga/42-synthetic-story",
            Some("moetruyen"),
            UrlKind::MangaDetail,
        ),
        (
            "https://moetruyen.net/manga/42-synthetic-story/chapters/3.5",
            Some("moetruyen"),
            UrlKind::ChapterPage,
        ),
    ];

    for (raw, expected_adapter, expected_kind) in cases {
        let info = classify_url_pre_fetch(raw)
            .unwrap_or_else(|diag| panic!("expected valid URL '{raw}', got diag: {diag:?}"));
        assert_eq!(
            info.adapter_id, expected_adapter,
            "adapter mismatch for {raw}"
        );
        assert_eq!(info.url_kind, expected_kind, "kind mismatch for {raw}");
    }
}

#[test]
fn invalid_and_private_urls_are_rejected_with_structured_diagnostics() {
    for raw in [
        "",
        "   ",
        "ftp://example.org/manga",
        "http://127.0.0.1/admin",
        "javascript:alert(1)",
    ] {
        let err = classify_url_pre_fetch(raw).expect_err("expected invalid URL rejection");
        assert_eq!(
            err.status,
            RecognitionStatus::InvalidUrl,
            "status for '{raw}'"
        );
        assert_eq!(err.url_kind, UrlKind::InvalidUrl, "kind for '{raw}'");
    }
}

#[test]
fn error_diagnostics_distinguish_network_dns_tls_rate_limit_and_api_failures() {
    let url = parse_url("https://mangadex.org/");

    let dns = diagnostic_from_error(
        &ScrapeError::DnsFailure {
            host: "mangadex.org".to_owned(),
            url: url.as_str().to_owned(),
            detail: "nxdomain".to_owned(),
        },
        url.as_str(),
        Some(&url),
        UrlKind::WebsiteHome,
        Some("MangaDex"),
        Some("mangadex"),
        "fetch",
    );
    assert_eq!(dns.status, RecognitionStatus::DnsFailure);

    let tls = diagnostic_from_error(
        &ScrapeError::TlsFailure {
            host: "mangadex.org".to_owned(),
            url: url.as_str().to_owned(),
            detail: "connection reset".to_owned(),
        },
        url.as_str(),
        Some(&url),
        UrlKind::WebsiteHome,
        Some("MangaDex"),
        Some("mangadex"),
        "fetch",
    );
    assert_eq!(tls.status, RecognitionStatus::TlsFailure);

    let rate = diagnostic_from_error(
        &ScrapeError::RateLimited {
            url: url.as_str().to_owned(),
            retry_after_secs: Some(30),
        },
        url.as_str(),
        Some(&url),
        UrlKind::WebsiteHome,
        Some("MangaDex"),
        Some("mangadex"),
        "fetch",
    );
    assert_eq!(rate.status, RecognitionStatus::RateLimited);
    assert_eq!(rate.http_status, Some(429));

    let restricted = diagnostic_from_error(
        &ScrapeError::AccessRestricted {
            host: "example.org".to_owned(),
            status: 403,
            url: url.as_str().to_owned(),
            detail: "forbidden".to_owned(),
        },
        url.as_str(),
        Some(&url),
        UrlKind::MangaDetail,
        Some("example.org"),
        Some("generic"),
        "fetch",
    );
    assert_eq!(restricted.status, RecognitionStatus::AccessRestricted);
    assert_eq!(restricted.http_status, Some(403));

    let api_err = diagnostic_from_error(
        &ScrapeError::ApiFailure {
            service: "mangadex".to_owned(),
            url: "https://api.mangadex.org/manga".to_owned(),
            status: Some(503),
            detail: "maintenance".to_owned(),
        },
        url.as_str(),
        Some(&url),
        UrlKind::CatalogPage,
        Some("MangaDex"),
        Some("mangadex"),
        "api",
    );
    assert_eq!(api_err.status, RecognitionStatus::ApiFailure);
    assert_eq!(api_err.http_status, Some(503));
}

#[test]
fn mangadex_catalog_and_detail_json_parsers() {
    let catalog_json = r#"{
        "result": "ok",
        "limit": 24,
        "offset": 0,
        "total": 48,
        "data": [{
            "id": "11111111-2222-3333-4444-555555555555",
            "type": "manga",
            "attributes": {
                "title": { "en": "Synthetic Starlight" },
                "altTitles": [{ "ja": "合成スターライト" }, { "vi": "Ánh Sao Giả Lập" }],
                "description": { "en": "A mock sci-fi manga synopsis." },
                "originalLanguage": "ja",
                "status": "ongoing",
                "tags": [
                    { "attributes": { "name": { "en": "Sci-Fi" }, "group": "genre" } },
                    { "attributes": { "name": { "en": "Time Travel" }, "group": "theme" } }
                ]
            },
            "relationships": [
                { "id": "aaaa1111-2222-3333-4444-555555555555", "type": "cover_art", "attributes": { "fileName": "cover123.jpg" } },
                { "id": "bbbb1111-2222-3333-4444-555555555555", "type": "author", "attributes": { "name": "Test Author" } },
                { "id": "cccc1111-2222-3333-4444-555555555555", "type": "artist", "attributes": { "name": "Test Artist" } }
            ]
        }]
    }"#;

    let home_url = parse_url("https://mangadex.org/");
    let catalog = mangadex::parse_catalog_json(catalog_json, &home_url, UrlKind::WebsiteHome, None)
        .expect("catalog json parses");
    assert_eq!(catalog.entries.len(), 1);
    assert_eq!(catalog.entries[0].title, "Synthetic Starlight");
    assert_eq!(
        catalog.entries[0].source_id.as_deref(),
        Some("mangadex:11111111-2222-3333-4444-555555555555")
    );
    assert!(catalog.entries[0].cover.is_some());
    assert_eq!(catalog.pagination.current_page, 1);
    assert_eq!(catalog.pagination.total_pages, Some(2));
    assert!(catalog.pagination.next_url.is_some());

    let detail_json = r#"{
        "result": "ok",
        "data": {
            "id": "11111111-2222-3333-4444-555555555555",
            "type": "manga",
            "attributes": {
                "title": { "en": "Synthetic Starlight" },
                "altTitles": [{ "ja": "合成スターライト" }],
                "description": { "en": "A mock sci-fi manga synopsis." },
                "originalLanguage": "ja",
                "status": "ongoing",
                "tags": [
                    { "attributes": { "name": { "en": "Sci-Fi" }, "group": "genre" } },
                    { "attributes": { "name": { "en": "Time Travel" }, "group": "theme" } }
                ]
            },
            "relationships": [
                { "id": "aaaa1111-2222-3333-4444-555555555555", "type": "cover_art", "attributes": { "fileName": "cover123.jpg" } },
                { "id": "bbbb1111-2222-3333-4444-555555555555", "type": "author", "attributes": { "name": "Test Author" } },
                { "id": "cccc1111-2222-3333-4444-555555555555", "type": "artist", "attributes": { "name": "Test Artist" } }
            ]
        }
    }"#;
    let detail_req =
        parse_url("https://api.mangadex.org/manga/11111111-2222-3333-4444-555555555555");
    let manga = mangadex::parse_manga_detail_json(detail_json, &detail_req).expect("detail parses");
    assert_eq!(manga.title, "Synthetic Starlight");
    assert_eq!(manga.original_title.as_deref(), Some("合成スターライト"));
    assert_eq!(manga.authors, vec!["Test Author".to_owned()]);
    assert_eq!(manga.artists, vec!["Test Artist".to_owned()]);
    assert_eq!(manga.genres, vec!["Sci-Fi".to_owned()]);
    assert_eq!(manga.tags, vec!["Time Travel".to_owned()]);
    assert_eq!(manga.status.as_deref(), Some("ongoing"));
}

#[test]
fn mangadex_feed_chapter_meta_and_at_home_parsers() {
    let detail_req =
        parse_url("https://api.mangadex.org/manga/11111111-2222-3333-4444-555555555555");
    let feed_json = r#"{
        "result": "ok",
        "total": 3,
        "data": [
            {
                "id": "dddd1111-2222-3333-4444-555555555555",
                "type": "chapter",
                "attributes": {
                    "volume": "1",
                    "chapter": "1",
                    "title": "Awakening",
                    "translatedLanguage": "en",
                    "publishAt": "2026-01-15T12:00:00+00:00",
                    "pages": 18,
                    "externalUrl": null
                }
            },
            {
                "id": "eeee1111-2222-3333-4444-555555555555",
                "type": "chapter",
                "attributes": {
                    "volume": "1",
                    "chapter": "2",
                    "title": "External Only",
                    "translatedLanguage": "en",
                    "publishAt": "2026-01-20T12:00:00+00:00",
                    "pages": 0,
                    "externalUrl": "https://example.org/paywall/2"
                }
            },
            {
                "id": "ffff1111-2222-3333-4444-555555555555",
                "type": "chapter",
                "attributes": {
                    "volume": "1",
                    "chapter": "2",
                    "title": "Hành Trình",
                    "translatedLanguage": "vi",
                    "publishAt": "2026-01-21T12:00:00+00:00",
                    "pages": 20,
                    "externalUrl": null
                }
            }
        ]
    }"#;
    let (chapters, total) =
        mangadex::parse_chapter_feed_json(feed_json, &detail_req).expect("feed parses");
    assert_eq!(total, 3);
    assert_eq!(
        chapters.len(),
        2,
        "external 0-page chapter should be filtered out"
    );
    assert_eq!(chapters[0].name, "Chapter 1 - Awakening");
    assert_eq!(chapters[0].language.as_deref(), Some("en"));
    assert_eq!(chapters[0].published_at.as_deref(), Some("2026-01-15"));
    assert_eq!(chapters[1].language.as_deref(), Some("vi"));
}

#[test]
fn mangadex_chapter_meta_and_at_home_parsers() {
    let chap_meta_json = r#"{
        "result": "ok",
        "data": {
            "id": "dddd1111-2222-3333-4444-555555555555",
            "type": "chapter",
            "attributes": {
                "chapter": "1",
                "title": "Awakening",
                "translatedLanguage": "en",
                "pages": 18
            },
            "relationships": [
                { "id": "11111111-2222-3333-4444-555555555555", "type": "manga" }
            ]
        }
    }"#;
    let chap_url = parse_url("https://mangadex.org/chapter/dddd1111-2222-3333-4444-555555555555");
    let (parent_id, chap_label, chap_num) =
        mangadex::parse_chapter_meta_json(chap_meta_json, &chap_url).expect("chapter meta parses");
    assert_eq!(
        parent_id.as_deref(),
        Some("11111111-2222-3333-4444-555555555555")
    );
    assert_eq!(chap_label, "Chapter 1 - Awakening");
    assert_eq!(chap_num, Some(1.0));

    let at_home_json = r#"{
        "result": "ok",
        "baseUrl": "https://cdn.example.invalid",
        "chapter": {
            "hash": "abc123hash",
            "data": ["p1.png", "p2.png"],
            "dataSaver": ["s1.jpg", "s2.jpg"]
        }
    }"#;
    let pages = mangadex::parse_at_home_json(at_home_json, &chap_url, &chap_url)
        .expect("at-home pages parse");
    assert_eq!(pages.len(), 2);
    assert_eq!(
        pages[0].url.as_str(),
        "https://cdn.example.invalid/data/abc123hash/p1.png"
    );
    assert_eq!(pages[0].fallbacks.len(), 1);
    assert_eq!(
        pages[0].fallbacks[0].as_str(),
        "https://cdn.example.invalid/data-saver/abc123hash/s1.jpg"
    );
}

#[test]
fn truyenqq_amp_catalog_and_canonicalization() {
    let mobile_url = parse_url("https://m.truyenqqko.com/truyen-tranh/demo-hero-7777");
    let canonical = truyenqq::canonical_desktop_url(&mobile_url);
    assert_eq!(
        canonical.as_str(),
        "https://truyenqqko.com/truyen-tranh/demo-hero-7777"
    );

    let amp_catalog_html = r#"<!doctype html>
    <html>
      <head><title>TruyenQQ Mobile Catalog</title></head>
      <body>
        <nav>
          <a href="/the-loai/action-26.html">Action</a>
          <a href="/the-loai/fantasy-30.html">Fantasy</a>
        </nav>
        <div class="list_item">
          <div class="item">
            <a href="/truyen-tranh/demo-hero-7777" title="Demo Hero">
              <amp-img src="https://img.example.invalid/demo-hero.jpg" width="160" height="220"></amp-img>
              <div class="title">Demo Hero</div>
            </a>
            <a class="chapter" href="/truyen-tranh/demo-hero-7777-chap-15.html">Chương 15</a>
          </div>
          <div class="item">
            <a href="https://external-ads.example.invalid/promo">Sponsored</a>
          </div>
        </div>
        <div class="pagination">
          <a class="active" href="/truyen-moi-cap-nhat/trang-1.html">1</a>
          <a href="/truyen-moi-cap-nhat/trang-2.html">2</a>
        </div>
      </body>
    </html>"#;

    let catalog_url = parse_url("https://m.truyenqqko.com/truyen-tranh/");
    let catalog =
        truyenqq::parse_catalog_html(amp_catalog_html, &catalog_url, UrlKind::CatalogPage)
            .expect("truyenqq amp catalog parses");
    assert_eq!(catalog.entries.len(), 1, "external ad link must be ignored");
    assert_eq!(catalog.entries[0].title, "Demo Hero");
    assert_eq!(
        catalog.entries[0].url.as_str(),
        "https://truyenqqko.com/truyen-tranh/demo-hero-7777"
    );
    assert_eq!(
        catalog.entries[0].source_id.as_deref(),
        Some("truyenqq:7777")
    );
    assert_eq!(catalog.categories.len(), 3);
    assert!(catalog.categories.iter().any(|c| c.label == "Action"));
    assert!(catalog.categories.iter().any(|c| c.label == "Fantasy"));
    assert!(catalog.pagination.next_url.is_some());
}

#[test]
fn truyenqq_detail_and_chapter_parsers() {
    let canonical = parse_url("https://truyenqqko.com/truyen-tranh/demo-hero-7777");
    let detail_html = r#"<!doctype html>
    <html>
      <body>
        <div class="book_avatar"><img src="https://img.example.invalid/demo-hero.jpg" /></div>
        <div class="book_other">
          <h1>Demo Hero</h1>
          <ul class="list-info">
            <li class="othername row"><p class="name">Tên khác</p><h2 class="other-name">Hero Alt; Second Name</h2></li>
            <li class="author row"><p class="name">Tác giả</p><p class="col-xs-9"><a href="/tac-gia/a">Author One</a></p></li>
            <li class="status row"><p class="name">Tình trạng</p><p class="col-xs-9">Đang Cập Nhật</p></li>
          </ul>
          <ul class="list01">
            <li><a href="/the-loai/action-26.html">Action</a></li>
            <li><a href="/the-loai/fantasy-30.html">Fantasy</a></li>
          </ul>
        </div>
        <div class="story-detail-info">Synthetic description for demo hero story.</div>
        <div class="works-chapter-list">
          <div class="works-chapter-item">
            <div class="name-chap"><a href="/truyen-tranh/demo-hero-7777-chap-2.html">Chương 2</a></div>
            <div class="time-chap">02/01/2026</div>
          </div>
          <div class="works-chapter-item">
            <div class="name-chap"><a href="/truyen-tranh/demo-hero-7777-chap-1.html">Chương 1</a></div>
            <div class="time-chap">01/01/2026</div>
          </div>
        </div>
      </body>
    </html>"#;

    let manga =
        truyenqq::parse_detail_html(detail_html, &canonical).expect("truyenqq detail parses");
    assert_eq!(manga.title, "Demo Hero");
    assert_eq!(manga.source_id.as_deref(), Some("truyenqq:7777"));
    assert_eq!(manga.authors, vec!["Author One".to_owned()]);
    assert_eq!(
        manga.genres,
        vec!["Action".to_owned(), "Fantasy".to_owned()]
    );
    assert_eq!(manga.status.as_deref(), Some("Đang Cập Nhật"));
    assert_eq!(manga.chapters.len(), 2);
    assert_eq!(
        manga.chapters[0].number,
        Some(1.0),
        "chapters must be ordered oldest-first"
    );
    assert_eq!(manga.chapters[1].number, Some(2.0));
    assert_eq!(
        manga.chapters[0].published_at.as_deref(),
        Some("01/01/2026")
    );

    let chapter_html = r#"<!doctype html>
    <html>
      <body>
        <h1 class="detail-title">Demo Hero - Chương 1</h1>
        <div class="chapter_content">
          <div class="page-chapter">
            <img data-original="https://cdn1.example.invalid/p1.jpg" data-cdn="https://cdn2.example.invalid/p1.jpg" src="data:image/gif;base64,R0lGODlhAQABAAAAACw=" />
          </div>
          <div class="page-chapter">
            <img data-original="https://cdn1.example.invalid/p2.jpg" data-cdn="https://cdn2.example.invalid/p2.jpg" />
          </div>
        </div>
      </body>
    </html>"#;
    let chap_url = parse_url("https://truyenqqko.com/truyen-tranh/demo-hero-7777-chap-1.html");
    let chap =
        truyenqq::parse_chapter_html(chapter_html, &chap_url).expect("truyenqq chapter parses");
    assert_eq!(chap.images.len(), 2);
    assert_eq!(
        chap.images[0].url.as_str(),
        "https://cdn1.example.invalid/p1.jpg"
    );
    assert_eq!(chap.images[0].fallbacks.len(), 1);
    assert_eq!(
        chap.images[0].fallbacks[0].as_str(),
        "https://cdn2.example.invalid/p1.jpg"
    );
}

#[test]
fn moetruyen_catalog_and_media_fallback_parser() {
    let base = parse_url("https://moetruyen.net/manga/");
    let (primary, fallback) = moetruyen::split_media_fallback(
        "https://cdn-primary.example.invalid/cover.webp#media-fallback=https%3A%2F%2Fcdn-backup.example.invalid%2Fcover.webp",
        &base,
    )
    .expect("media fallback splits cleanly");
    assert_eq!(
        primary.as_str(),
        "https://cdn-primary.example.invalid/cover.webp"
    );
    assert_eq!(
        fallback.as_ref().map(Url::as_str),
        Some("https://cdn-backup.example.invalid/cover.webp")
    );

    let catalog_html = r#"<!doctype html>
    <html>
      <body>
        <nav><a href="/genres/comedy">Comedy</a></nav>
        <div class="grid">
          <article class="card">
            <a href="/manga/88-synthetic-comedy" title="Synthetic Comedy">
              <img src="https://cdn.example.invalid/88.webp#media-fallback=https%3A%2F%2Fbackup.example.invalid%2F88.webp" alt="Synthetic Comedy" />
              <h3>Synthetic Comedy</h3>
            </a>
            <a href="/manga/88-synthetic-comedy/chapters/5">Chapter 5</a>
          </article>
        </div>
        <div class="pagination">
          <a href="/manga?page=2">2</a>
        </div>
      </body>
    </html>"#;

    let catalog = moetruyen::parse_catalog_html(catalog_html, &base, UrlKind::CatalogPage)
        .expect("moetruyen catalog parses");
    assert_eq!(catalog.entries.len(), 1);
    assert_eq!(catalog.entries[0].title, "Synthetic Comedy");
    assert_eq!(
        catalog.entries[0].source_id.as_deref(),
        Some("moetruyen:88")
    );
    assert!(catalog.pagination.next_url.is_some());
}

#[test]
fn moetruyen_detail_and_embedded_media_chapter_parsers() {
    let detail_html = r#"<!doctype html>
    <html>
      <body>
        <main>
          <h1>Synthetic Comedy [Tới Chap 5]</h1>
          <img class="manga-cover" src="https://cdn.example.invalid/88.webp" />
          <div class="meta">
            <p>Tên khác: Mock Comedy Alt</p>
            <p>Tác giả: <a href="/authors/mock-writer">Mock Writer</a></p>
            <p>Trạng thái: Đang tiến hành</p>
          </div>
          <div class="genres"><a href="/genres/comedy">Comedy</a></div>
          <div class="manga-description"><p>A synthetic comedy synopsis.</p></div>
          <section id="chapters">
            <a href="/manga/88-synthetic-comedy/chapters/2" data-chapter-id="202" data-chapter-number="2">Chapter 2</a>
            <a href="/manga/88-synthetic-comedy/chapters/1" data-chapter-id="201" data-chapter-number="1">Chapter 1</a>
            <a href="/manga/88-synthetic-comedy?chapterPage=2#chapters">2</a>
          </section>
        </main>
      </body>
    </html>"#;
    let detail_url = parse_url("https://moetruyen.net/manga/88-synthetic-comedy");
    let (manga, total_pages) =
        moetruyen::parse_detail_html(detail_html, &detail_url).expect("moetruyen detail parses");
    assert_eq!(
        manga.title, "Synthetic Comedy",
        "chapter count suffix must be stripped"
    );
    assert_eq!(manga.source_id.as_deref(), Some("moetruyen:88"));
    assert_eq!(total_pages, 2);
    assert_eq!(manga.chapters.len(), 2);
    assert_eq!(manga.chapters[0].number, Some(1.0));
    assert_eq!(manga.chapters[1].number, Some(2.0));

    let chapter_html = r#"<!doctype html>
    <html>
      <body>
        <h1>Chapter 1</h1>
        <script>
          window.__CHAPTER__ = {
            media: [
              {"pageIndex": 0, "primaryUrl": "https://cdn.example.invalid/0.js", "fallbackUrl": ""},
              {"pageIndex": 1, "primaryUrl": "https://cdn.example.invalid/page1.png", "fallbackUrl": "https://backup.example.invalid/page1.png"},
              {"pageIndex": 2, "primaryUrl": "https://cdn.example.invalid/page2.png", "fallbackUrl": ""}
            ]
          };
        </script>
      </body>
    </html>"#;
    let chap_url = parse_url("https://moetruyen.net/manga/88-synthetic-comedy/chapters/1");
    let chap =
        moetruyen::parse_chapter_html(chapter_html, &chap_url).expect("moetruyen chapter parses");
    assert_eq!(chap.images.len(), 2, "0.js sentinel must be skipped");
    assert_eq!(
        chap.images[0].url.as_str(),
        "https://cdn.example.invalid/page1.png"
    );
    assert_eq!(chap.images[0].fallbacks.len(), 1);
    assert_eq!(
        chap.images[0].fallbacks[0].as_str(),
        "https://backup.example.invalid/page1.png"
    );
    assert_eq!(
        chap.images[1].url.as_str(),
        "https://cdn.example.invalid/page2.png"
    );
}

#[tokio::test]
#[ignore = "live network metadata-level verification against target websites"]
async fn live_metadata_verification_across_three_target_sites() {
    use mangacore::{HeadlessPolicy, Registry, Resolved};

    let registry = Registry::new().expect("registry builds");

    let targets = [
        ("MangaDex", "https://mangadex.org/"),
        ("TruyenQQ", "https://m.truyenqqko.com/truyen-tranh/"),
        ("MoeTruyen", "https://moetruyen.net/manga/"),
    ];

    for (label, raw_url) in targets {
        println!("\n=== LIVE CHECK: {label} ({raw_url}) ===");
        let (resolved, diag) = registry
            .resolve_with_diagnostics(raw_url, HeadlessPolicy::Disabled)
            .await
            .unwrap_or_else(|d| panic!("{label} catalog resolve failed: {d:?}"));
        println!(
            "catalog diag: status={:?} kind={:?} adapter={:?} resolved={:?}",
            diag.status, diag.url_kind, diag.adapter, diag.resolved_url
        );

        let catalog = match resolved {
            Resolved::Catalog(c) => c,
            other => panic!("{label} expected Catalog, got {other:?}"),
        };
        assert!(
            !catalog.entries.is_empty(),
            "{label} catalog returned 0 entries"
        );
        println!(
            "catalog entries={} categories={} next_page={}",
            catalog.entries.len(),
            catalog.categories.len(),
            catalog.pagination.next_url.is_some()
        );

        // Try up to the first 3 catalog entries (in case an entry has 0 translated chapters).
        let mut verified_manga = false;
        for entry in catalog.entries.iter().take(3) {
            match registry
                .resolve_with_diagnostics(entry.url.as_str(), HeadlessPolicy::Disabled)
                .await
            {
                Ok((Resolved::Manga(manga), mdiag)) => {
                    println!(
                        "detail ok: status={:?} chapters={} authors={} genres={} cover={}",
                        mdiag.status,
                        manga.chapters.len(),
                        manga.authors.len(),
                        manga.genres.len(),
                        manga.cover.is_some()
                    );
                    if let Some(ch_ref) = manga.chapters.first() {
                        let chapter = registry
                            .chapter(&ch_ref.url, HeadlessPolicy::Disabled)
                            .await
                            .unwrap_or_else(|e| panic!("{label} chapter fetch failed: {e:?}"));
                        println!(
                            "chapter ok: pages={} fallbacks_on_p0={}",
                            chapter.images.len(),
                            chapter.images.first().map_or(0, |img| img.fallbacks.len())
                        );
                        assert!(
                            !chapter.images.is_empty(),
                            "{label} chapter returned 0 images"
                        );
                        verified_manga = true;
                        break;
                    }
                }
                Ok(_) => {}
                Err(d) => {
                    println!("entry skip ({}): {:?}", entry.url, d.1.status);
                }
            }
        }
        assert!(
            verified_manga,
            "{label} failed to verify detail+chapter on first 3 catalog entries"
        );
    }
}
