-- Rôle lecteur (ADR 0011) : SQLite ne modifie pas une contrainte CHECK, la table est donc recréée.
-- Les sessions ouvertes sont supprimées avec l'ancienne table (clé étrangère en cascade).
CREATE TABLE accounts_next (
    id TEXT PRIMARY KEY,
    username TEXT NOT NULL UNIQUE COLLATE NOCASE,
    password_hash TEXT NOT NULL,
    role TEXT NOT NULL CHECK (role IN ('admin', 'viewer')),
    disabled INTEGER NOT NULL DEFAULT 0,
    version INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

INSERT INTO accounts_next (id, username, password_hash, role, disabled, version, created_at, updated_at)
SELECT id, username, password_hash, role, disabled, version, created_at, updated_at FROM accounts;

DROP TABLE accounts;
ALTER TABLE accounts_next RENAME TO accounts;
