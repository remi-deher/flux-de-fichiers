CREATE TABLE workers (
    id UUID PRIMARY KEY,
    name TEXT NOT NULL UNIQUE CHECK (length(name) BETWEEN 1 AND 80),
    last_seen TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE jobs (
    id UUID PRIMARY KEY,
    source_name TEXT NOT NULL,
    target_worker_id UUID REFERENCES workers(id),
    worker_id UUID REFERENCES workers(id),
    status TEXT NOT NULL DEFAULT 'queued'
        CHECK (status IN ('queued', 'running', 'succeeded', 'failed')),
    attempts INTEGER NOT NULL DEFAULT 0,
    lease_token UUID,
    leased_until TIMESTAMPTZ,
    output_name TEXT,
    error_code TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX jobs_claim_idx ON jobs (created_at) WHERE status = 'queued';
CREATE INDEX jobs_expired_idx ON jobs (leased_until) WHERE status = 'running';
CREATE INDEX jobs_recent_idx ON jobs (created_at DESC, id);

CREATE TABLE sessions (
    token_hash TEXT PRIMARY KEY,
    expires_at TIMESTAMPTZ NOT NULL
);
CREATE INDEX sessions_expiry_idx ON sessions (expires_at);
