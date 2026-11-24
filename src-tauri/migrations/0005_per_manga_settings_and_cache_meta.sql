-- Per-manga isolated reader settings JSON (survives cache clears and app restarts).
ALTER TABLE manga ADD COLUMN reader_settings_json TEXT;

-- Distinguish small cover thumbnails (is_cover = 1) from transient chapter pages (is_cover = 0)
-- and track chapter ownership for immediate post-chapter/post-manga storage cleanup.
ALTER TABLE image_cache ADD COLUMN is_cover INTEGER NOT NULL DEFAULT 0;
ALTER TABLE image_cache ADD COLUMN chapter_url TEXT;

CREATE INDEX IF NOT EXISTS idx_image_cache_is_cover
    ON image_cache(is_cover, last_access);
CREATE INDEX IF NOT EXISTS idx_image_cache_manga_chapter
    ON image_cache(manga_id, chapter_url);
