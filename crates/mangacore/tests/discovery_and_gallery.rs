//! Integration tests for CMS structure detection, 18+/gallery extraction, tag parsing,
//! adult-signal classification, declarative JSON rule packs, and two-phase link matching.

use mangacore::{
    CmsKind, CustomRuleSpec, RuleCatalogConfig, RuleChaptersConfig, RuleDetailConfig,
    RulePagesConfig, RulePaginationKind, UserQueryKind, classify_user_input, detect_cms_from_html,
    normalize_source_root,
    parse::gallery::{parse_gallery_chapter, parse_gallery_manga, promote_thumbnail_to_fullsize},
    parse::meta::{detect_adult_signals, extract_creators, extract_tags_and_genres},
    parse_rule_catalog_html, parse_rule_chapter_html, parse_rule_manga_html,
    rewrite_domain_override, validate_rule_spec, within_edit_ratio,
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

fn demo_rule_spec() -> CustomRuleSpec {
    CustomRuleSpec {
        id: "demo-grid".to_owned(),
        name: "Demo Grid".to_owned(),
        version: 1,
        domains: vec!["mirror-hub.example".to_owned()],
        fingerprints: vec!["data-demo-grid".to_owned()],
        probe_paths: vec![],
        catalog: RuleCatalogConfig {
            endpoint: "/catalog".to_owned(),
            search_endpoint: Some("/search".to_owned()),
            search_param: Some("q".to_owned()),
            pagination: RulePaginationKind::PathSegment,
            page_param: None,
            page_size: None,
            item_selector: ".grid-card".to_owned(),
            title_selector: Some(".card-title".to_owned()),
            cover_selector: Some("img".to_owned()),
            link_selector: Some("a.card-link".to_owned()),
        },
        detail: RuleDetailConfig {
            title_selector: Some("h1.series-name".to_owned()),
            cover_selector: Some(".poster img".to_owned()),
            description_selector: Some(".synopsis".to_owned()),
            author_selector: Some(".author-name".to_owned()),
            status_selector: Some(".series-status".to_owned()),
            tag_selector: Some(".series-tags a".to_owned()),
        },
        chapters: RuleChaptersConfig {
            selector: "ul.chap-list li".to_owned(),
            title_selector: Some("a".to_owned()),
            link_selector: Some("a".to_owned()),
            ajax_endpoint: None,
            ajax_action: None,
        },
        pages: RulePagesConfig {
            image_selector: ".reader-pages img".to_owned(),
            image_attr: Some("data-real".to_owned()),
        },
    }
}

#[test]
fn validates_and_executes_declarative_json_rule_pack() {
    let spec = demo_rule_spec();
    validate_rule_spec(&spec).expect("rule pack should validate cleanly");

    let base = Url::parse("https://mirror-hub.example").unwrap();
    let cat_html = r#"
    <div data-demo-grid="1">
      <div class="grid-card">
        <a class="card-link" href="/series/star-forge"><img src="/covers/sf.jpg" /></a>
        <div class="card-title">Star Forge</div>
      </div>
    </div>"#;
    let cat = parse_rule_catalog_html(
        cat_html,
        &base.join("/catalog").unwrap(),
        &base,
        &spec,
        None,
        1,
    )
    .unwrap();
    assert_eq!(cat.entries.len(), 1);
    assert_eq!(cat.entries[0].title, "Star Forge");
    assert_eq!(
        cat.search_url_template.as_deref(),
        Some("https://mirror-hub.example/search?q={query}")
    );

    let detail_html = r#"
    <div>
      <h1 class="series-name">Star Forge</h1>
      <div class="poster"><img src="/covers/sf.jpg" /></div>
      <div class="synopsis">An interstellar blacksmith journey.</div>
      <span class="author-name">J. Smith</span>
      <span class="series-status">Ongoing</span>
      <div class="series-tags"><a href="/tag/sci-fi">Sci-Fi</a><a href="/tag/adventure">Adventure</a></div>
      <ul class="chap-list">
        <li><a href="/series/star-forge/ch-2">Chapter 2</a></li>
        <li><a href="/series/star-forge/ch-1">Chapter 1</a></li>
      </ul>
    </div>"#;
    let manga = parse_rule_manga_html(
        detail_html,
        &base.join("/series/star-forge").unwrap(),
        &spec,
    )
    .unwrap();
    assert_eq!(manga.title, "Star Forge");
    assert_eq!(manga.chapters.len(), 2);
    assert_eq!(
        manga.chapters[0].name, "Chapter 1",
        "chapters must normalize ascending"
    );
    assert_eq!(manga.tags, vec!["Sci-Fi", "Adventure"]);

    let reader_html = r#"
    <div class="reader-pages">
      <img data-real="https://img.mirror-hub.example/sf/1/01.jpg" src="/loading.gif" />
      <img data-real="https://img.mirror-hub.example/sf/1/02.jpg" src="/loading.gif" />
    </div>"#;
    let ch = parse_rule_chapter_html(
        reader_html,
        &base.join("/series/star-forge/ch-1").unwrap(),
        &spec,
    )
    .unwrap();
    assert_eq!(ch.images.len(), 2);
    assert_eq!(
        ch.images[0].url.as_str(),
        "https://img.mirror-hub.example/sf/1/01.jpg"
    );
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
