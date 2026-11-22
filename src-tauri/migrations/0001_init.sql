CREATE TABLE IF NOT EXISTS manga (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    url TEXT NOT NULL UNIQUE,
    site TEXT NOT NULL,
    title TEXT NOT NULL,
    cover_url TEXT,
    latest_chapter_url TEXT,
    latest_chapter_name TEXT,
    has_new INTEGER NOT NULL DEFAULT 0,
    checked_at INTEGER,
    updated_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS chapter (
    manga_id INTEGER NOT NULL REFERENCES manga(id) ON DELETE CASCADE,
    url TEXT NOT NULL,
    name TEXT NOT NULL,
    number REAL,
    idx INTEGER NOT NULL,
    read INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (manga_id, url)
);

CREATE INDEX IF NOT EXISTS idx_chapter_manga_idx ON chapter(manga_id, idx);

CREATE TABLE IF NOT EXISTS history (
    manga_id INTEGER PRIMARY KEY REFERENCES manga(id) ON DELETE CASCADE,
    chapter_url TEXT NOT NULL,
    chapter_name TEXT NOT NULL,
    scroll_ratio REAL NOT NULL DEFAULT 0.0,
    read_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_history_read_at ON history(read_at DESC);

CREATE TABLE IF NOT EXISTS settings (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS image_cache (
    key TEXT PRIMARY KEY,
    content_type TEXT NOT NULL,
    size INTEGER NOT NULL,
    last_access INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_image_cache_lru ON image_cache(last_access ASC);
