-- Per-website discovered filter schemas (tags + dynamic filter axes) and
-- internal CBZ chapter archive index (strictly user-imported files only).

ALTER TABLE saved_source ADD COLUMN filter_schema_json TEXT;

CREATE TABLE IF NOT EXISTS site_filter_cache (
    host TEXT PRIMARY KEY,
    filter_schema_json TEXT NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS cbz_chapter_archive (
    manga_id INTEGER NOT NULL REFERENCES manga(id) ON DELETE CASCADE,
    chapter_url TEXT NOT NULL,
    rel_path TEXT NOT NULL,
    page_entries_json TEXT NOT NULL,
    file_size INTEGER NOT NULL,
    created_at INTEGER NOT NULL,
    PRIMARY KEY (manga_id, chapter_url)
);
