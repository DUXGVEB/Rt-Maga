ALTER TABLE manga ADD COLUMN reader_mode_override TEXT;
ALTER TABLE manga ADD COLUMN detected_webtoon INTEGER NOT NULL DEFAULT 0;

ALTER TABLE history ADD COLUMN page_index INTEGER NOT NULL DEFAULT 0;

CREATE TABLE IF NOT EXISTS bookmark (
    manga_id INTEGER PRIMARY KEY REFERENCES manga(id) ON DELETE CASCADE,
    created_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_bookmark_created_at ON bookmark(created_at DESC);

CREATE TABLE IF NOT EXISTS notification (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    manga_id INTEGER NOT NULL REFERENCES manga(id) ON DELETE CASCADE,
    manga_title TEXT NOT NULL,
    cover_url TEXT,
    manga_url TEXT NOT NULL,
    chapter_range TEXT NOT NULL,
    latest_chapter_url TEXT NOT NULL,
    chapter_count INTEGER NOT NULL DEFAULT 1,
    read INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_notification_created_at ON notification(created_at DESC);

CREATE TABLE IF NOT EXISTS notified_chapter (
    manga_id INTEGER NOT NULL REFERENCES manga(id) ON DELETE CASCADE,
    chapter_url TEXT NOT NULL,
    notified_at INTEGER NOT NULL,
    PRIMARY KEY (manga_id, chapter_url)
);
