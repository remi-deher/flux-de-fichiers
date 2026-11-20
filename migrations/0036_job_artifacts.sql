-- Artefacts retenus chez un worker et tirés en direct par un autre (ADR 0022) : le central ne
-- conserve que leur description, jamais les octets.
CREATE TABLE job_artifacts (
    job_id TEXT NOT NULL REFERENCES jobs(id) ON DELETE CASCADE,
    kind TEXT NOT NULL CHECK (kind IN ('working', 'result')),
    holder_worker_id TEXT NOT NULL REFERENCES workers(id) ON DELETE CASCADE,
    -- Époque du bail sous lequel le détenteur a déclaré l'artefact.
    lease_epoch INTEGER NOT NULL,
    root TEXT NOT NULL CHECK (length(root) BETWEEN 1 AND 1024),
    name TEXT NOT NULL CHECK (length(name) BETWEEN 1 AND 255),
    sha256 TEXT NOT NULL CHECK (length(sha256) = 64),
    bytes INTEGER NOT NULL CHECK (bytes > 0),
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (job_id, kind)
);

-- Un billet désigne soit la source du job (colonne vide), soit un artefact.
ALTER TABLE source_tickets ADD COLUMN artifact_kind TEXT
    CHECK (artifact_kind IS NULL OR artifact_kind IN ('working', 'result'));
