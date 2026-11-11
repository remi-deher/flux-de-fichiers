-- Table de gestion des dépôts distants de plugins et d'outils
CREATE TABLE IF NOT EXISTS plugin_repositories (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    index_url TEXT NOT NULL UNIQUE,
    is_official BOOLEAN NOT NULL DEFAULT 0,
    enabled BOOLEAN NOT NULL DEFAULT 1,
    last_synced_at TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Insertion idempotente du catalogue officiel
INSERT OR IGNORE INTO plugin_repositories (id, name, index_url, is_official, enabled)
VALUES (
    'repo.official',
    'Catalogue Officiel Flux de Fichiers',
    'https://raw.githubusercontent.com/remi-deher/flux-de-fichiers-plugins/main/catalogue/index.json',
    1,
    1
);
