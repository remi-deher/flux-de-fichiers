-- Métadonnées du dernier vrai job de chaque flux (ADR 0025) : l'éditeur les propose comme échantillon
-- de simulation et d'aperçu des références. Une ligne par flux, remplacée à chaque nouveau job.
CREATE TABLE flow_media_samples (
    flow_id TEXT PRIMARY KEY REFERENCES flows(id) ON DELETE CASCADE,
    job_id TEXT NOT NULL,
    source_name TEXT NOT NULL,
    media_info_json TEXT NOT NULL,
    recorded_at TEXT NOT NULL DEFAULT (datetime('now'))
);
