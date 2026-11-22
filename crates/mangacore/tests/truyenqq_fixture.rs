//! Integration tests verifying the `TruyenQQ` adapter against saved HTML fixtures and live pages.

use mangacore::source::truyenqq::TruyenQqSource;
use mangacore::{HeadlessPolicy, Registry, Source};
use url::Url;

const MANGA_FIXTURE: &str = include_str!("fixtures/truyenqq_manga.html");
const CHAPTER_FIXTURE: &str = include_str!("fixtures/truyenqq_chapter.html");

#[test]
fn parses_truyenqq_manga_fixture() {
    let url = Url::parse("https://truyenqqko.com/truyen-tranh/canh-sat-thieu-nien-16695").unwrap();
    let manga = TruyenQqSource::parse_manga_html(MANGA_FIXTURE, &url).unwrap();

    assert_eq!(manga.title, "Cảnh Sát Thiếu Niên");
    assert!(manga.cover.is_some(), "cover image should be parsed");
    assert!(
        manga.chapters.len() >= 40,
        "expected >= 40 chapters, got {}",
        manga.chapters.len()
    );

    // Verify ascending chapter order.
    let first = manga.chapters.first().unwrap();
    let last = manga.chapters.last().unwrap();
    assert!(
        first.number.unwrap_or(0.0) < last.number.unwrap_or(0.0),
        "chapters should be sorted ascending: first={:?}, last={:?}",
        first.number,
        last.number
    );
}

#[test]
fn parses_truyenqq_chapter_fixture() {
    let url = Url::parse("https://truyenqqko.com/truyen-tranh/canh-sat-thieu-nien-16695-chap-44")
        .unwrap();
    let chapter = TruyenQqSource::parse_chapter_html(CHAPTER_FIXTURE, &url).unwrap();

    assert_eq!(
        chapter.images.len(),
        36,
        "expected 36 page images in fixture"
    );
    assert_eq!(
        chapter.images[0].referer, url,
        "referer must match chapter url for hotlink protection"
    );
    assert!(
        !chapter.images[0].fallbacks.is_empty(),
        "data-cdn fallback mirror should be preserved"
    );
    assert_eq!(
        chapter.prev.as_ref().map(Url::as_str),
        Some("https://truyenqqko.com/truyen-tranh/canh-sat-thieu-nien-16695-chap-43")
    );
    assert_eq!(
        chapter.next.as_ref().map(Url::as_str),
        Some("https://truyenqqko.com/truyen-tranh/canh-sat-thieu-nien-16695-chap-45")
    );
    assert_eq!(
        chapter.manga_url.as_ref().map(Url::as_str),
        Some("https://truyenqqko.com/truyen-tranh/canh-sat-thieu-nien-16695")
    );
}

#[test]
fn matches_truyenqq_mirror_domains() {
    let source = TruyenQqSource;
    assert!(source.matches(&Url::parse("https://truyenqqko.com/truyen-tranh/a").unwrap()));
    assert!(source.matches(&Url::parse("https://truyenqqgo.com/truyen-tranh/a").unwrap()));
    assert!(source.matches(&Url::parse("https://truyenqqvn.com/truyen-tranh/a").unwrap()));
    assert!(!source.matches(&Url::parse("https://example.com/truyen-tranh/a").unwrap()));
}

#[tokio::test]
#[ignore = "requires live internet access to TruyenQQ"]
async fn live_truyenqq_resolve_and_image_fetch() {
    let registry = Registry::new().unwrap();
    let resolved = registry
        .resolve(
            "https://truyenqqko.com/truyen-tranh/canh-sat-thieu-nien-16695",
            HeadlessPolicy::Disabled,
        )
        .await
        .unwrap();

    let manga = resolved.manga();
    assert!(!manga.chapters.is_empty(), "live manga has chapters");

    let chapter = registry
        .chapter(&manga.chapters[0].url, HeadlessPolicy::Disabled)
        .await
        .unwrap();
    assert!(!chapter.images.is_empty(), "live chapter has images");

    let first_img = &chapter.images[0];
    let fetched = registry
        .http()
        .fetch_image(&first_img.url, Some(&first_img.referer))
        .await
        .unwrap();
    assert!(
        fetched.content_type.starts_with("image/"),
        "expected image/*, got {}",
        fetched.content_type
    );
    assert!(!fetched.bytes.is_empty(), "image bytes non-empty");
}
