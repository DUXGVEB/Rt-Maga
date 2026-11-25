-- Persistent date-added sorting on manga, saved source directory (with supplemental/18+ isolation),
-- and custom declarative JSON rule packs.

ALTER TABLE manga ADD COLUMN created_at INTEGER;

UPDATE manga
SET created_at = COALESCE(
    (SELECT b.created_at FROM bookmark b WHERE b.manga_id = manga.id),
    (SELECT h.read_at FROM history h WHERE h.manga_id = manga.id),
    updated_at
)
WHERE created_at IS NULL;

CREATE INDEX IF NOT EXISTS idx_manga_created_at ON manga(created_at DESC);

CREATE TABLE IF NOT EXISTS saved_source (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    source_key TEXT NOT NULL UNIQUE,
    display_name TEXT NOT NULL,
    base_url TEXT NOT NULL UNIQUE,
    source_kind TEXT NOT NULL,
    parser_ref TEXT,
    search_template TEXT,
    icon_url TEXT,
    is_enabled INTEGER NOT NULL DEFAULT 1,
    is_supplemental INTEGER NOT NULL DEFAULT 0,
    manual_visibility INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_saved_source_supp_created
    ON saved_source(is_supplemental, created_at DESC);

CREATE TABLE IF NOT EXISTS source_rule_spec (
    rule_id TEXT PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    version INTEGER NOT NULL DEFAULT 1,
    raw_json TEXT NOT NULL,
    origin TEXT NOT NULL DEFAULT 'user_imported',
    is_enabled INTEGER NOT NULL DEFAULT 1,
    updated_at INTEGER NOT NULL
);
