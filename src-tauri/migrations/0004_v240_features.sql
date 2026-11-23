-- v2.4.0: Enriched manga/chapter metadata, source-specific identity keys for duplicate prevention,
-- and structured diagnostic log fields for URL recognition and network debugging.

ALTER TABLE manga ADD COLUMN source_id TEXT;
ALTER TABLE manga ADD COLUMN canonical_url TEXT;
ALTER TABLE manga ADD COLUMN alt_titles_json TEXT;
ALTER TABLE manga ADD COLUMN original_title TEXT;
ALTER TABLE manga ADD COLUMN authors_json TEXT;
ALTER TABLE manga ADD COLUMN artists_json TEXT;
ALTER TABLE manga ADD COLUMN description TEXT;
ALTER TABLE manga ADD COLUMN genres_json TEXT;
ALTER TABLE manga ADD COLUMN tags_json TEXT;
ALTER TABLE manga ADD COLUMN status TEXT;
ALTER TABLE manga ADD COLUMN language TEXT;
ALTER TABLE manga ADD COLUMN available_languages_json TEXT;

CREATE INDEX IF NOT EXISTS idx_manga_source_id ON manga(source_id);
CREATE INDEX IF NOT EXISTS idx_manga_canonical_url ON manga(canonical_url);

ALTER TABLE chapter ADD COLUMN volume TEXT;
ALTER TABLE chapter ADD COLUMN published_at TEXT;
ALTER TABLE chapter ADD COLUMN language TEXT;
ALTER TABLE chapter ADD COLUMN source_id TEXT;

ALTER TABLE diagnostic_log ADD COLUMN failed_url TEXT;
ALTER TABLE diagnostic_log ADD COLUMN resolved_url TEXT;
ALTER TABLE diagnostic_log ADD COLUMN http_status INTEGER;
ALTER TABLE diagnostic_log ADD COLUMN adapter TEXT;
ALTER TABLE diagnostic_log ADD COLUMN parsing_stage TEXT;
ALTER TABLE diagnostic_log ADD COLUMN api_error TEXT;
