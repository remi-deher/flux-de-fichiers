-- Automatisation des bibliothèques : scan planifié, règles de filtrage et stratégie de publication.
ALTER TABLE libraries ADD COLUMN scan_interval_minutes INTEGER NOT NULL DEFAULT 0;
ALTER TABLE libraries ADD COLUMN last_scanned_at TEXT;
ALTER TABLE libraries ADD COLUMN include_extensions TEXT NOT NULL DEFAULT '';
ALTER TABLE libraries ADD COLUMN exclude_patterns TEXT NOT NULL DEFAULT '';
ALTER TABLE libraries ADD COLUMN min_file_bytes INTEGER NOT NULL DEFAULT 0;
ALTER TABLE libraries ADD COLUMN output_policy TEXT NOT NULL DEFAULT 'output_dir'
    CHECK (output_policy IN ('output_dir', 'replace_with_backup', 'replace_in_place'));
