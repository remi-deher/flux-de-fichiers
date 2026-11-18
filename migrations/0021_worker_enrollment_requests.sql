CREATE TABLE worker_enrollment_requests (
    id TEXT PRIMARY KEY,
    worker_name TEXT NOT NULL CHECK (length(worker_name) BETWEEN 1 AND 80),
    worker_version TEXT,
    source_addr TEXT,
    secret_hash TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending'
        CHECK (status IN ('pending', 'approved', 'rejected')),
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    expires_at TEXT NOT NULL
);
CREATE INDEX worker_enrollment_expires_idx ON worker_enrollment_requests (expires_at);
