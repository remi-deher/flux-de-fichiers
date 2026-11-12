-- Bibliothèques déclarées : un flux, et un chemin par nœud.
CREATE TABLE libraries (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL UNIQUE CHECK (length(name) BETWEEN 1 AND 80),
    flow_id TEXT REFERENCES flows(id) ON DELETE SET NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX libraries_flow_idx ON libraries (flow_id);

CREATE TABLE library_nodes (
    library_id TEXT NOT NULL REFERENCES libraries(id) ON DELETE CASCADE,
    worker_id TEXT NOT NULL REFERENCES workers(id) ON DELETE CASCADE,
    local_path TEXT NOT NULL CHECK (length(local_path) BETWEEN 1 AND 512),
    access TEXT NOT NULL CHECK (access IN ('read', 'read_write')),
    PRIMARY KEY (library_id, worker_id)
);

CREATE INDEX library_nodes_worker_idx ON library_nodes (worker_id);
