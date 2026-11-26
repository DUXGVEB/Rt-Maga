//! Integration tests for CMS structure detection, 18+/gallery extraction, tag parsing,
//! adult-signal classification, dynamic filter discovery, universal pagination, and mirror matching.

use std::collections::BTreeMap;

use mangacore::{
    CatalogEntry, CatalogFilterQuery, CmsKind, SiteFilterSchema, UnhandledFilterMask,
    UpdatedWithinFilter, UrlKind, UserQueryKind, apply_page_to_template,
    build_filtered_catalog_url, classify_normalized_url, classify_user_input, detect_cms_from_html,
    extract_generic_filter_schema, extract_universal_pagination, filter_entries_client_side,
    normalize_source_root,
    parse::gallery::{parse_gallery_chapter, parse_gallery_manga, promote_thumbnail_to_fullsize},
    parse::meta::{detect_adult_signals, extract_creators, extract_tags_and_genres},
    rewrite_domain_override,
    source::truyenqq::{is_truyenqq_host, parse_catalog_html as parse_truyenqq_catalog},
    within_edit_ratio,
};
use scraper::Html;
use url::Url;

#[test]
fn detects_cms_structures_and_adult_signals_from_synthetic_dom() {
    let madara_html = r#"
    <html>
      <head>
        <title>Alpha Comics</title>
        <meta name="rating" content="adult" />
        <link rel="stylesheet" href="/wp-content/themes/madara/style.css" />
      </head>
      <body>
        <div class="page-item-detail manga">
          <div class="item-summary">
            <h3 class="h5"><a href="/manga/night-express/">Night Express</a></h3>
          </div>
        </div>
        <div class="page-item-detail manga">
          <div class="item-summary">
            <h3 class="h5"><a href="/manga/sky-lantern/">Sky Lantern</a></h3>
          </div>
        </div>
      </body>
    </html>"#;
    let doc = Html::parse_document(madara_html);
    assert_eq!(
        detect_cms_from_html(&doc, madara_html),
        Some(CmsKind::Madara)
    );
    let url = Url::parse("https://alpha-comics.example/").unwrap();
    assert!(detect_adult_signals(&url, madara_html, &[], &[]));

    let themesia_html = r#"
    <html>
      <head><link rel="stylesheet" href="/wp-content/themes/mangareader/css/app.css" /></head>
      <body>
        <div class="listupd">
          <div class="bsx"><a href="/series/one/">One</a></div>
          <div class="bsx"><a href="/series/two/">Two</a></div>
        </div>
      </body>
    </html>"#;
    let t_doc = Html::parse_document(themesia_html);
    assert_eq!(
        detect_cms_from_html(&t_doc, themesia_html),
        Some(CmsKind::MangaThemesia)
    );
    assert!(!detect_adult_signals(&url, themesia_html, &[], &[]));
}

#[test]
fn extracts_tags_with_counts_stripped_and_parses_single_work_gallery() {
    let gallery_html = r#"
    <html>
      <head><title>Moonlit Sketchbook</title></head>
      <body>
        <h1>Moonlit Sketchbook</h1>
        <div class="summary_image"><img src="https://cdn.gallery-archive.example/g/42/cover.jpg" /></div>
        <div class="tag-container">
          <span class="tags">
            <a class="tag" href="/tag/doujinshi/"><span class="name">doujinshi (12,450)</span></a>
            <a class="tag" href="/tag/full-color/"><span class="name">full color (890)</span></a>
            <a class="tag" href="/parody/original/"><span class="name">original (9,001)</span></a>
          </span>
          <a href="/artist/studio-sun/"><span class="name">Studio Sun</span></a>
        </div>
        <div id="thumbnail-container">
          <a href="/g/42/1/"><img data-src="https://cdn.gallery-archive.example/g/42/1t.jpg" /></a>
          <a href="/g/42/2/"><img data-src="https://cdn.gallery-archive.example/g/42/2t.jpg" /></a>
          <a href="/g/42/3/"><img data-src="https://cdn.gallery-archive.example/g/42/3t.jpg" /></a>
          <a href="/g/42/4/"><img data-src="https://cdn.gallery-archive.example/g/42/4t.jpg" /></a>
        </div>
      </body>
    </html>"#;

    let page_url = Url::parse("https://gallery-archive.example/g/42/").unwrap();
    let doc = Html::parse_document(gallery_html);
    let (genres, tags) = extract_tags_and_genres(&doc);
    assert_eq!(genres, Vec::<String>::new(), "expected no genres");
    assert_eq!(tags, vec!["doujinshi", "full color", "original"]);
    let (_, artists) = extract_creators(&doc);
    assert_eq!(artists, vec!["Studio Sun"]);
    assert!(detect_adult_signals(
        &page_url,
        gallery_html,
        &genres,
        &tags
    ));

    let manga = parse_gallery_manga(gallery_html, &page_url, &page_url)
        .expect("gallery work must synthesize a single Full Gallery chapter");
    assert_eq!(manga.title, "Moonlit Sketchbook");
    assert_eq!(manga.chapters.len(), 1);
    assert_eq!(manga.chapters[0].name, "Full Gallery (4 pages)");
    assert_eq!(manga.tags, vec!["doujinshi", "full color", "original"]);

    let chapter = parse_gallery_chapter(gallery_html, &page_url, &page_url)
        .expect("gallery pages must resolve");
    assert_eq!(chapter.images.len(), 4);
    assert_eq!(
        chapter.images[0].url.as_str(),
        "https://cdn.gallery-archive.example/g/42/1.jpg"
    );
    assert!(
        chapter.images[0]
            .fallbacks
            .iter()
            .any(|u| u.as_str().ends_with("/1.webp"))
    );

    let (full, fbs) = promote_thumbnail_to_fullsize(
        &Url::parse("https://cdn.gallery-archive.example/g/99/7t.png").unwrap(),
    );
    assert_eq!(
        full.as_str(),
        "https://cdn.gallery-archive.example/g/99/7.png"
    );
    assert!(fbs.iter().any(|u| u.as_str().ends_with("/7.jpg")));
}

#[test]
fn truyenqq_com_vn_homepage_pagination_and_multi_tag_filters_work() {
    assert!(is_truyenqq_host("truyenqq.com.vn"));
    assert!(is_truyenqq_host("m.truyenqq.com.vn"));
    assert!(is_truyenqq_host("www.truyenqq.com.vn"));

    let home_url = Url::parse("https://truyenqq.com.vn/").unwrap();
    let (adapter, _, kind) = classify_normalized_url(&home_url, "truyenqq.com.vn");
    assert_eq!(adapter, Some("truyenqq"));
    assert_eq!(kind, UrlKind::WebsiteHome);

    let sample_html = r#"
    <html>
      <head><title>TruyenQQ</title></head>
      <body>
        <nav>
          <a href="/the-loai/action-26.html">Action</a>
          <a href="/the-loai/fantasy-30.html">Fantasy</a>
          <a href="/the-loai/horror-44.html">Horror</a>
        </nav>
        <ul class="list_grid">
          <li>
            <div class="book_avatar">
              <a href="/truyen-tranh/kiem-thanh-tai-the-1001"><img src="https://cdn.example/c1.jpg" alt="Kiếm Thánh" /></a>
              <span class="time-ago">2 giờ trước</span>
            </div>
            <div class="book_name"><a href="/truyen-tranh/kiem-thanh-tai-the-1001" title="Kiếm Thánh Tái Thế">Kiếm Thánh Tái Thế</a></div>
            <div class="last_chapter"><a href="/truyen-tranh/kiem-thanh-tai-the-1001-chap-125.html">Chương 125</a></div>
            <div class="more-info">
              <div class="info"><p>Tình trạng: Đang Cập Nhật</p></div>
              <div class="list-tags"><a href="/the-loai/action-26.html">Action</a><a href="/the-loai/fantasy-30.html">Fantasy</a></div>
            </div>
          </li>
        </ul>
      </body>
    </html>"#;

    let cat = parse_truyenqq_catalog(sample_html, &home_url, UrlKind::WebsiteHome).unwrap();
    assert_eq!(cat.entries.len(), 1);
    assert_eq!(cat.entries[0].chapter_count, Some(125));
    assert_eq!(cat.entries[0].updated_at.as_deref(), Some("2 giờ trước"));
    assert_eq!(cat.entries[0].genres, vec!["Action", "Fantasy"]);
    assert_eq!(
        cat.pagination.next_url.as_ref().map(Url::as_str),
        Some("https://truyenqq.com.vn/truyen-moi-cap-nhat/trang-2.html"),
        "homepage must wire seamless page 2 navigation"
    );
    assert_eq!(
        cat.pagination.page_url_template.as_deref(),
        Some("https://truyenqq.com.vn/truyen-moi-cap-nhat/trang-{page}.html")
    );
    assert_ne!(cat.filter_schema.tags, []);

    let schema =
        mangacore::source::truyenqq::truyenqq_filter_schema("https://truyenqq.com.vn", None, None);
    let mut extra = BTreeMap::new();
    extra.insert("status".to_owned(), vec!["2".to_owned()]);
    let filters = CatalogFilterQuery {
        query: None,
        include_tags: vec!["26".to_owned(), "30".to_owned()],
        exclude_tags: vec!["44".to_owned()],
        min_chapters: Some(50),
        updated_within: UpdatedWithinFilter::Days7,
        extra,
    };
    let (built_url, unhandled) = build_filtered_catalog_url(
        &home_url,
        Some(&home_url),
        CmsKind::BuiltInAdapter,
        Some("truyenqq"),
        None,
        &schema,
        &filters,
        3,
    )
    .unwrap();
    assert!(
        built_url
            .as_str()
            .starts_with("https://truyenqq.com.vn/tim-kiem-nang-cao/trang-3.html?"),
        "got {built_url}"
    );
    assert!(built_url.as_str().contains("category=26%2C30"));
    assert!(built_url.as_str().contains("notcategory=44"));
    assert!(built_url.as_str().contains("minchapter=50"));
    assert!(built_url.as_str().contains("status=2"));
    assert!(!unhandled.include_tags && !unhandled.exclude_tags && !unhandled.min_chapters);
    assert!(
        unhandled.updated_within,
        "recency window falls back cleanly to client filter"
    );
}

#[test]
fn universal_pagination_and_dynamic_filter_extraction_work() {
    let html = r#"
    <html>
      <body>
        <form action="/filter" method="get">
          <input type="checkbox" name="genre[]" id="g-act" value="action" /><label for="g-act">Action</label>
          <input type="checkbox" name="genre[]" id="g-rom" value="romance" /><label for="g-rom">Romance</label>
          <label for="sel-type">Format</label>
          <select name="format" id="sel-type">
            <option value="">All</option>
            <option value="manga">Manga</option>
            <option value="manhwa">Manhwa</option>
          </select>
        </form>
        <div class="pagination">
          <span class="current">1</span>
          <a href="/catalog/page/2/">2</a>
          <a href="/catalog/page/3/">3</a>
          <a href="/catalog/page/48/">48</a>
          <a class="next" href="/catalog/page/2/">Next</a>
        </div>
      </body>
    </html>"#;

    let page_url = Url::parse("https://public-comics.example/catalog/").unwrap();
    let doc = Html::parse_document(html);

    let pag = extract_universal_pagination(&doc, &page_url, &page_url, true);
    assert_eq!(pag.current_page, 1);
    assert_eq!(pag.total_pages, Some(48));
    assert_eq!(
        pag.next_url.as_ref().map(Url::as_str),
        Some("https://public-comics.example/catalog/page/2/")
    );
    let jumped = apply_page_to_template(pag.page_url_template.as_deref().unwrap(), 19).unwrap();
    assert_eq!(
        jumped.as_str(),
        "https://public-comics.example/catalog/page/19/"
    );

    let schema: SiteFilterSchema = extract_generic_filter_schema(&doc, &page_url, None);
    assert_eq!(schema.include_tag_param.as_deref(), Some("genre[]"));
    assert_eq!(schema.tags.len(), 2);
    assert_eq!(schema.extra_axes.len(), 1);
    assert_eq!(schema.extra_axes[0].key, "format");

    let mut items = vec![
        CatalogEntry {
            url: Url::parse("https://public-comics.example/m/1").unwrap(),
            title: "Veteran Saga".to_owned(),
            alt_titles: Vec::new(),
            cover: None,
            latest_chapter: Some("Chapter 120".to_owned()),
            status: None,
            genres: vec!["Action".to_owned()],
            source_id: None,
            chapter_count: Some(120),
            updated_at: Some("3 hours ago".to_owned()),
        },
        CatalogEntry {
            url: Url::parse("https://public-comics.example/m/2").unwrap(),
            title: "Short Old".to_owned(),
            alt_titles: Vec::new(),
            cover: None,
            latest_chapter: Some("Chapter 5".to_owned()),
            status: None,
            genres: vec!["Romance".to_owned()],
            source_id: None,
            chapter_count: Some(5),
            updated_at: Some("2 months ago".to_owned()),
        },
    ];
    let q = CatalogFilterQuery {
        query: None,
        include_tags: vec!["action".to_owned()],
        exclude_tags: vec!["romance".to_owned()],
        min_chapters: Some(50),
        updated_within: UpdatedWithinFilter::Days7,
        extra: BTreeMap::new(),
    };
    filter_entries_client_side(
        &mut items,
        &schema,
        &q,
        UnhandledFilterMask {
            include_tags: true,
            exclude_tags: true,
            min_chapters: true,
            updated_within: true,
        },
    );
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].title, "Veteran Saga");
}

#[test]
fn classifies_user_inputs_and_computes_similarity_and_mirror_override() {
    match classify_user_input(
        "rtmaga://manga?id=42&source=custom_1&url=https://old.example/a&title=Demo",
    ) {
        UserQueryKind::AppDeepLink {
            manga_id,
            source_key,
            url,
            title,
        } => {
            assert_eq!(manga_id, Some(42));
            assert_eq!(source_key.as_deref(), Some("custom_1"));
            assert_eq!(url.as_deref(), Some("https://old.example/a"));
            assert_eq!(title.as_deref(), Some("Demo"));
        }
        other => panic!("expected AppDeepLink, got {other:?}"),
    }

    assert!(matches!(
        classify_user_input("alpha-comics.example/manga/demo"),
        UserQueryKind::WebUrl(_)
    ));
    assert_eq!(
        classify_user_input("solo sword master"),
        UserQueryKind::KeywordSearch("solo sword master".to_owned())
    );

    assert!(within_edit_ratio(
        "Star Forge Chronicle",
        "Star Forge Chronicls",
        0.20
    ));
    assert!(!within_edit_ratio(
        "Star Forge",
        "Totally Different Title",
        0.20
    ));

    let root = normalize_source_root("https://www.alpha-comics.example/manga/123?ref=top").unwrap();
    assert_eq!(root.as_str(), "https://www.alpha-comics.example/");

    let orig = Url::parse("https://old-mirror.example/manga/star-forge?lang=en").unwrap();
    let mirror = Url::parse("https://new-mirror.example").unwrap();
    let rewritten = rewrite_domain_override(&orig, &mirror);
    assert_eq!(
        rewritten.as_str(),
        "https://new-mirror.example/manga/star-forge?lang=en"
    );
}
