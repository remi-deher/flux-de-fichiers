CREATE TABLE plugin_settings (
    plugin_id TEXT PRIMARY KEY,
    settings TEXT NOT NULL,
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);
