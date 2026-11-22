//! Integration tests verifying generic HTML heuristics on synthetic series and chapter pages.

use mangacore::parse::classify::{PageKind, classify_page};
use mangacore::source::generic::GenericSource;
use scraper::Html;
use url::Url;

const GENERIC_MANGA_HTML: &str = r#"
<!doctype html>
<html>
<head>
  <title>Starlight Odyssey - ExampleReader</title>
  <meta property="og:title" content="Starlight Odyssey - ExampleReader" />
  <meta property="og:image" content="https://cdn.example.com/covers/starlight.jpg" />
</head>
<body>
  <header><a href="/chapter-999-promo">Promo</a></header>
  <h1>Starlight Odyssey</h1>
  <div class="chapter-list">
    <a href="/series/starlight-odyssey/chapter-10">Chapter 10</a>
    <a href="/series/starlight-odyssey/chapter-2-5">Chapter 2.5</a>
    <a href="/series/starlight-odyssey/chapter-2">Chapter 2</a>
    <a href="/series/starlight-odyssey/chapter-1">Chapter 1</a>
  </div>
</body>
</html>
"#;

const GENERIC_CHAPTER_HTML: &str = r#"
<!doctype html>
<html>
<head>
  <title>Starlight Odyssey Chapter 2 - ExampleReader</title>
</head>
<body>
  <header><img src="https://cdn.example.com/logo.png" width="120" height="40" /></header>
  <nav class="breadcrumb">
    <a href="/">Home</a>
    <a href="/series/starlight-odyssey">Starlight Odyssey</a>
    <a href="/series/starlight-odyssey/chapter-2">Chapter 2</a>
  </nav>
  <div class="ads-banner">
    <img src="https://ads.example.net/banner1.jpg" />
    <img src="https://cdn.example.com/banner-promo.gif" />
  </div>
  <main class="reading-content">
    <div class="page-item"><img data-src="https://img.example.com/starlight/2/01.webp" src="/loading.gif" /></div>
    <div class="page-item"><img data-src="https://img.example.com/starlight/2/02.webp" src="/loading.gif" /></div>
    <div class="page-item"><img data-src="https://img.example.com/starlight/2/03.webp" src="/loading.gif" /></div>
    <div class="page-item"><img data-src="https://img.example.com/starlight/2/04.webp" src="/loading.gif" /></div>
  </main>
  <div class="controls">
    <a class="btn-prev-chap" href="/series/starlight-odyssey/chapter-1">Prev</a>
    <a class="btn-next-chap" href="/series/starlight-odyssey/chapter-2-5">Next</a>
  </div>
</body>
</html>
"#;

const CMS_LAYOUT_MANGA_HTML: &str = r#"
<!doctype html>
<html>
<head><title>Dawn Horizon</title></head>
<body>
  <div class="book_avatar"><img src="https://cdn.example.org/covers/dawn.jpg" /></div>
  <h1 itemprop="name">Dawn Horizon</h1>
  <div class="works-chapter-list">
    <div class="works-chapter-item"><div class="name-chap"><a href="/series/dawn-horizon-100-chap-3">Chapter 3</a></div></div>
    <div class="works-chapter-item"><div class="name-chap"><a href="/series/dawn-horizon-100-chap-2">Chapter 2</a></div></div>
    <div class="works-chapter-item"><div class="name-chap"><a href="/series/dawn-horizon-100-chap-1">Chapter 1</a></div></div>
  </div>
</body>
</html>
"#;

const CMS_LAYOUT_CHAPTER_HTML: &str = r#"
<!doctype html>
<html>
<head><title>Dawn Horizon Chapter 2</title></head>
<body>
  <div class="chapter_content">
    <div class="page-chapter"><img class="lazy" data-original="https://img1.example.org/100/2/0.jpg" data-cdn="https://cdn2.example.org/100/2/0.jpg" src="/placeholder.gif" /></div>
    <div class="page-chapter"><img class="lazy" data-original="https://img1.example.org/100/2/1.jpg" data-cdn="https://cdn2.example.org/100/2/1.jpg" src="/placeholder.gif" /></div>
    <div class="page-chapter"><img class="lazy" data-original="https://img1.example.org/100/2/2.jpg" data-cdn="https://cdn2.example.org/100/2/2.jpg" src="/placeholder.gif" /></div>
  </div>
  <a class="prev link-prev-chap" href="/series/dawn-horizon-100-chap-1">Prev</a>
  <a class="next link-next-chap" href="/series/dawn-horizon-100-chap-3">Next</a>
</body>
</html>
"#;

const SCRIPT_IMAGES_CHAPTER_HTML: &str = r#"
<!doctype html>
<html>
<head><title>Script Series Chap 7</title></head>
<body>
  <h1>Script Series Chap 7</h1>
  <script>
    window.__PAGES__ = [
      "https:\/\/img.example.net\/series\/7\/001.jpg",
      "https:\/\/img.example.net\/series\/7\/002.jpg",
      "https:\/\/img.example.net\/series\/7\/003.jpg",
      "https:\/\/img.example.net\/series\/7\/004.jpg"
    ];
  </script>
</body>
</html>
"#;

#[test]
fn generic_parses_manga_and_sorts_fractional_chapters() {
    let url = Url::parse("https://example.com/series/starlight-odyssey").unwrap();
    let doc = Html::parse_document(GENERIC_MANGA_HTML);
    assert_eq!(classify_page(&doc, &url), PageKind::Manga);

    let manga = GenericSource::parse_manga_html(GENERIC_MANGA_HTML, &url).unwrap();
    assert_eq!(manga.title, "Starlight Odyssey");
    assert_eq!(
        manga.cover.as_ref().map(Url::as_str),
        Some("https://cdn.example.com/covers/starlight.jpg")
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
    let url = Url::parse("https://example.com/series/starlight-odyssey/chapter-2").unwrap();
    let doc = Html::parse_document(GENERIC_CHAPTER_HTML);
    assert_eq!(classify_page(&doc, &url), PageKind::Chapter);

    let chapter = GenericSource::parse_chapter_html(GENERIC_CHAPTER_HTML, &url).unwrap();
    let urls: Vec<&str> = chapter.images.iter().map(|i| i.url.as_str()).collect();
    assert_eq!(
        urls,
        vec![
            "https://img.example.com/starlight/2/01.webp",
            "https://img.example.com/starlight/2/02.webp",
            "https://img.example.com/starlight/2/03.webp",
            "https://img.example.com/starlight/2/04.webp",
        ],
        "should prefer data-src and exclude logos, gifs, and ad domains"
    );
    assert_eq!(
        chapter.prev.as_ref().map(Url::as_str),
        Some("https://example.com/series/starlight-odyssey/chapter-1")
    );
    assert_eq!(
        chapter.next.as_ref().map(Url::as_str),
        Some("https://example.com/series/starlight-odyssey/chapter-2-5")
    );
    assert_eq!(
        chapter.manga_url.as_ref().map(Url::as_str),
        Some("https://example.com/series/starlight-odyssey")
    );
}

#[test]
fn generic_extracts_script_embedded_images() {
    let url = Url::parse("https://example.net/series/chap-7").unwrap();
    let chapter = GenericSource::parse_chapter_html(SCRIPT_IMAGES_CHAPTER_HTML, &url).unwrap();
    assert_eq!(chapter.images.len(), 4);
    assert_eq!(
        chapter.images[0].url.as_str(),
        "https://img.example.net/series/7/001.jpg"
    );
}

#[test]
fn generic_parses_common_cms_layout_with_fallback_mirrors() {
    let manga_url = Url::parse("https://example.org/series/dawn-horizon-100").unwrap();
    let manga = GenericSource::parse_manga_html(CMS_LAYOUT_MANGA_HTML, &manga_url).unwrap();
    assert_eq!(manga.title, "Dawn Horizon");
    assert_eq!(
        manga.cover.as_ref().map(Url::as_str),
        Some("https://cdn.example.org/covers/dawn.jpg")
    );
    assert_eq!(manga.chapters.len(), 3);
    assert_eq!(manga.chapters[0].number, Some(1.0));
    assert_eq!(manga.chapters[2].number, Some(3.0));

    let chap_url = Url::parse("https://example.org/series/dawn-horizon-100-chap-2").unwrap();
    let chap = GenericSource::parse_chapter_html(CMS_LAYOUT_CHAPTER_HTML, &chap_url).unwrap();
    assert_eq!(chap.images.len(), 3);
    assert_eq!(
        chap.images[0].url.as_str(),
        "https://img1.example.org/100/2/0.jpg"
    );
    assert_eq!(
        chap.images[0].fallbacks[0].as_str(),
        "https://cdn2.example.org/100/2/0.jpg",
        "data-cdn fallback mirror should be preserved"
    );
    assert_eq!(
        chap.prev.as_ref().map(Url::as_str),
        Some("https://example.org/series/dawn-horizon-100-chap-1")
    );
    assert_eq!(
        chap.next.as_ref().map(Url::as_str),
        Some("https://example.org/series/dawn-horizon-100-chap-3")
    );
    assert_eq!(
        chap.manga_url.as_ref().map(Url::as_str),
        Some("https://example.org/series/dawn-horizon-100")
    );
}
