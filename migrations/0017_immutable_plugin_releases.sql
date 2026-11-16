-- Épinglage TOFU des releases de dépôts et archives d'outils (immuabilité par version).
CREATE TABLE IF NOT EXISTS plugin_release_pins (
    scope TEXT NOT NULL,
    version TEXT NOT NULL,
    sha256 TEXT NOT NULL,
    source_url TEXT NOT NULL,
    first_seen_at TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (scope, version)
);
