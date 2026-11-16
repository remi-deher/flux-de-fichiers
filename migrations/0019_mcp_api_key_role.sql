-- Rôle MCP (et lecteur) sur les clés d'API (ADR 0015) : SQLite ne modifie pas une contrainte CHECK,
-- la table api_keys est donc recréée en conservant toutes les clés existantes.
CREATE TABLE api_keys_next (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    key_hash TEXT NOT NULL,
    role TEXT NOT NULL CHECK (role IN ('admin', 'mcp', 'worker', 'viewer')),
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    last_used_at TEXT,
    revoked_at TEXT
);

INSERT INTO api_keys_next (id, name, key_hash, role, created_at, last_used_at, revoked_at)
SELECT id, name, key_hash, role, created_at, last_used_at, revoked_at FROM api_keys;

DROP TABLE api_keys;
ALTER TABLE api_keys_next RENAME TO api_keys;
