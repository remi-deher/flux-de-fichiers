-- Gestion et persistance des flux de traitement (Flows) type Zapier / FileFlows
CREATE TABLE flows (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    description TEXT,
    file_mask TEXT NOT NULL DEFAULT '*.mkv, *.mp4',
    is_default BOOLEAN NOT NULL DEFAULT 0,
    definition TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX flows_default_idx ON flows (is_default);
