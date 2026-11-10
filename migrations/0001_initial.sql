CREATE TABLE workers (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL UNIQUE CHECK (length(name) BETWEEN 1 AND 80),
    last_seen TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE jobs (
    id TEXT PRIMARY KEY,
    source_name TEXT NOT NULL,
    target_worker_id TEXT REFERENCES workers(id),
    worker_id TEXT REFERENCES workers(id),
    status TEXT NOT NULL DEFAULT 'queued'
        CHECK (status IN ('queued', 'running', 'succeeded', 'failed')),
    attempts INTEGER NOT NULL DEFAULT 0,
    lease_token TEXT,
    leased_until TEXT,
    output_name TEXT,
    error_code TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX jobs_claim_idx ON jobs (created_at) WHERE status = 'queued';
CREATE INDEX jobs_expired_idx ON jobs (leased_until) WHERE status = 'running';
CREATE INDEX jobs_recent_idx ON jobs (created_at DESC, id);

CREATE TABLE sessions (
    token_hash TEXT PRIMARY KEY,
    expires_at TEXT NOT NULL
);
CREATE INDEX sessions_expiry_idx ON sessions (expires_at);
