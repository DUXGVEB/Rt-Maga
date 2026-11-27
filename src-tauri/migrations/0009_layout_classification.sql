-- Persist explicit multi-signal layout classification ('manga' | 'webtoon' | NULL)
-- so Reader auto-presets distinguish classified Manga, classified Webtoon, and Undecided titles.
ALTER TABLE manga ADD COLUMN detected_layout TEXT;

UPDATE manga
SET detected_layout = 'webtoon'
WHERE detected_webtoon = 1;
