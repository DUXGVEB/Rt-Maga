//! Integration tests verifying generic HTML heuristics on arbitrary manga sites.

use mangacore::parse::classify::{PageKind, classify_page};
use mangacore::source::generic::GenericSource;
use scraper::Html;
use url::Url;

const GENERIC_MANGA_HTML: &str = r#"
<!doctype html>
<html>
<head>
  <title>Solo Leveling - ExampleManga</title>
  <meta property="og:title" content="Solo Leveling - ExampleManga" />
  <meta property="og:image" content="https://cdn.example.com/covers/solo.jpg" />
</head>
<body>
  <header><a href="/chapter-999-promo">Promo</a></header>
  <h1>Solo Leveling</h1>
  <div class="chapter-list">
    <a href="/manga/solo-leveling/chapter-10">Chapter 10</a>
    <a href="/manga/solo-leveling/chapter-2-5">Chapter 2.5</a>
    <a href="/manga/solo-leveling/chapter-2">Chapter 2</a>
    <a href="/manga/solo-leveling/chapter-1">Chapter 1</a>
  </div>
</body>
</html>
"#;

const GENERIC_CHAPTER_HTML: &str = r#"
<!doctype html>
<html>
<head>
  <title>Solo Leveling Chapter 2 - ExampleManga</title>
</head>
<body>
  <header><img src="https://cdn.example.com/logo.png" width="120" height="40" /></header>
  <nav class="breadcrumb">
    <a href="/">Home</a>
    <a href="/manga/solo-leveling">Solo Leveling</a>
    <a href="/manga/solo-leveling/chapter-2">Chapter 2</a>
  </nav>
  <div class="ads-banner">
    <img src="https://ads.doubleclick.net/banner1.jpg" />
    <img src="https://cdn.example.com/banner-promo.gif" />
  </div>
  <main class="reading-content">
    <div class="page-item"><img data-src="https://img.example.com/solo/2/01.webp" src="/loading.gif" /></div>
    <div class="page-item"><img data-src="https://img.example.com/solo/2/02.webp" src="/loading.gif" /></div>
    <div class="page-item"><img data-src="https://img.example.com/solo/2/03.webp" src="/loading.gif" /></div>
    <div class="page-item"><img data-src="https://img.example.com/solo/2/04.webp" src="/loading.gif" /></div>
  </main>
  <div class="controls">
    <a class="btn-prev-chap" href="/manga/solo-leveling/chapter-1">Chap trước</a>
    <a class="btn-next-chap" href="/manga/solo-leveling/chapter-2-5">Chap sau</a>
  </div>
</body>
</html>
"#;

const SCRIPT_IMAGES_CHAPTER_HTML: &str = r#"
<!doctype html>
<html>
<head><title>JS Manga Chap 7</title></head>
<body>
  <h1>JS Manga Chap 7</h1>
  <script>
    window.__PAGES__ = [
      "https:\/\/img.jsmanga.org\/series\/7\/001.jpg",
      "https:\/\/img.jsmanga.org\/series\/7\/002.jpg",
      "https:\/\/img.jsmanga.org\/series\/7\/003.jpg",
      "https:\/\/img.jsmanga.org\/series\/7\/004.jpg"
    ];
  </script>
</body>
</html>
"#;

#[test]
fn generic_parses_manga_and_sorts_fractional_chapters() {
    let url = Url::parse("https://example.com/manga/solo-leveling").unwrap();
    let doc = Html::parse_document(GENERIC_MANGA_HTML);
    assert_eq!(classify_page(&doc, &url), PageKind::Manga);

    let manga = GenericSource::parse_manga_html(GENERIC_MANGA_HTML, &url).unwrap();
    assert_eq!(manga.title, "Solo Leveling");
    assert_eq!(
        manga.cover.as_ref().map(Url::as_str),
        Some("https://cdn.example.com/covers/solo.jpg")
    );
    let numbers: Vec<Option<f32>> = manga.chapters.iter().map(|c| c.number).collect();
    assert_eq!(
        numbers,
        vec![Some(1.0), Some(2.0), Some(2.5), Some(10.0)],
        "chapters must be sorted ascending including fractional numbers"
    );
}

#[test]
fn generic_extracts_chapter_images_and_filters_ads() {
    let url = Url::parse("https://example.com/manga/solo-leveling/chapter-2").unwrap();
    let doc = Html::parse_document(GENERIC_CHAPTER_HTML);
    assert_eq!(classify_page(&doc, &url), PageKind::Chapter);

    let chapter = GenericSource::parse_chapter_html(GENERIC_CHAPTER_HTML, &url).unwrap();
    let urls: Vec<&str> = chapter.images.iter().map(|i| i.url.as_str()).collect();
    assert_eq!(
        urls,
        vec![
            "https://img.example.com/solo/2/01.webp",
            "https://img.example.com/solo/2/02.webp",
            "https://img.example.com/solo/2/03.webp",
            "https://img.example.com/solo/2/04.webp",
        ],
        "should prefer data-src and exclude logos, gifs, and ad domains"
    );
    assert_eq!(
        chapter.prev.as_ref().map(Url::as_str),
        Some("https://example.com/manga/solo-leveling/chapter-1")
    );
    assert_eq!(
        chapter.next.as_ref().map(Url::as_str),
        Some("https://example.com/manga/solo-leveling/chapter-2-5")
    );
    assert_eq!(
        chapter.manga_url.as_ref().map(Url::as_str),
        Some("https://example.com/manga/solo-leveling")
    );
}

#[test]
fn generic_extracts_script_embedded_images() {
    let url = Url::parse("https://jsmanga.org/series/chap-7").unwrap();
    let chapter = GenericSource::parse_chapter_html(SCRIPT_IMAGES_CHAPTER_HTML, &url).unwrap();
    assert_eq!(chapter.images.len(), 4);
    assert_eq!(
        chapter.images[0].url.as_str(),
        "https://img.jsmanga.org/series/7/001.jpg"
    );
}

#[test]
fn generic_also_handles_truyenqq_fixtures_without_adapter() {
    let manga_html = include_str!("fixtures/truyenqq_manga.html");
    let chap_html = include_str!("fixtures/truyenqq_chapter.html");

    let manga_url =
        Url::parse("https://truyenqqko.com/truyen-tranh/canh-sat-thieu-nien-16695").unwrap();
    let chap_url =
        Url::parse("https://truyenqqko.com/truyen-tranh/canh-sat-thieu-nien-16695-chap-44")
            .unwrap();

    let manga = GenericSource::parse_manga_html(manga_html, &manga_url).unwrap();
    assert!(manga.chapters.len() >= 40);

    let chap = GenericSource::parse_chapter_html(chap_html, &chap_url).unwrap();
    assert_eq!(chap.images.len(), 36);
}
