-- Unified multi-format local comic chapter index (CBZ/ZIP, CBR/RAR, CB7/7z, CBT/TAR,
-- EPUB, PDF, image folders, and loose image imports).
-- Migrates any pre-existing rows from legacy `cbz_chapter_archive` cleanly while enforcing
-- a PRIMARY KEY on `chapter_url` for atomic UPSERTs.

CREATE TABLE IF NOT EXISTS local_comic_chapter (
    manga_id INTEGER NOT NULL REFERENCES manga(id) ON DELETE CASCADE,
    chapter_url TEXT PRIMARY KEY,
    source_format TEXT NOT NULL DEFAULT 'zip',
    rel_path TEXT NOT NULL,
    page_entries_json TEXT NOT NULL,
    file_size INTEGER NOT NULL,
    created_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_local_comic_chapter_manga_id
    ON local_comic_chapter(manga_id);

INSERT OR IGNORE INTO local_comic_chapter
    (manga_id, chapter_url, source_format, rel_path, page_entries_json, file_size, created_at)
SELECT
    manga_id,
    chapter_url,
    'zip',
    rel_path,
    page_entries_json,
    file_size,
    created_at
FROM cbz_chapter_archive;
