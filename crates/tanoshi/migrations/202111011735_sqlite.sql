ALTER TABLE download_queue ADD COLUMN source_id INTEGER NOT NULL;
ALTER TABLE download_queue ADD COLUMN manga_id INTEGER NOT NULL;
ALTER TABLE download_queue ADD COLUMN chapter_id INTEGER NOT NULL;