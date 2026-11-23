ALTER TABLE manga ADD COLUMN hidden INTEGER NOT NULL DEFAULT 0;

ALTER TABLE image_cache ADD COLUMN site TEXT;
ALTER TABLE image_cache ADD COLUMN manga_id INTEGER;

CREATE INDEX IF NOT EXISTS idx_image_cache_site ON image_cache(site);
CREATE INDEX IF NOT EXISTS idx_image_cache_manga_id ON image_cache(manga_id);

CREATE TABLE IF NOT EXISTS diagnostic_log (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    created_at INTEGER NOT NULL,
    level TEXT NOT NULL,
    category TEXT NOT NULL,
    message TEXT NOT NULL,
    detail TEXT,
    source TEXT,
    chapter_url TEXT,
    stack_trace TEXT
);

CREATE INDEX IF NOT EXISTS idx_diagnostic_log_created_at ON diagnostic_log(created_at DESC);

CREATE TABLE IF NOT EXISTS reader_session (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    active INTEGER NOT NULL DEFAULT 0,
    manga_id INTEGER NOT NULL,
    chapter_url TEXT NOT NULL,
    chapter_name TEXT NOT NULL,
    scroll_ratio REAL NOT NULL DEFAULT 0.0,
    page_index INTEGER NOT NULL DEFAULT 0,
    reading_mode TEXT NOT NULL DEFAULT 'vertical',
    updated_at INTEGER NOT NULL
);
