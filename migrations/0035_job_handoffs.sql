-- Transfert en cours de flux : le bail d'origine reste actif jusqu'à l'accusé du repreneur.
CREATE TABLE job_handoffs (
    job_id TEXT NOT NULL REFERENCES jobs(id) ON DELETE CASCADE,
    from_worker_id TEXT NOT NULL REFERENCES workers(id) ON DELETE CASCADE,
    source_lease_epoch INTEGER NOT NULL,
    target_worker_id TEXT REFERENCES workers(id),
    requirement_json TEXT,
    resume_node_id TEXT NOT NULL CHECK (length(resume_node_id) BETWEEN 1 AND 128),
    artifact_name TEXT NOT NULL CHECK (length(artifact_name) BETWEEN 1 AND 255),
    artifact_sha256 TEXT NOT NULL CHECK (length(artifact_sha256) = 64),
    artifact_bytes INTEGER NOT NULL CHECK (artifact_bytes >= 0),
    warning INTEGER NOT NULL DEFAULT 0 CHECK (warning IN (0, 1)),
    receiver_worker_id TEXT REFERENCES workers(id),
    receiver_token_hash TEXT,
    state TEXT NOT NULL CHECK (state IN ('uploading', 'pending', 'claimed', 'acknowledged', 'failed')),
    expires_at TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    acknowledged_at TEXT,
    staging_cleaned INTEGER NOT NULL DEFAULT 0 CHECK (staging_cleaned IN (0, 1)),
    PRIMARY KEY (job_id, source_lease_epoch)
);
CREATE INDEX job_handoffs_state_expiry ON job_handoffs(state, expires_at);
CREATE UNIQUE INDEX job_handoffs_one_active ON job_handoffs(job_id)
    WHERE state IN ('uploading', 'pending', 'claimed');

-- Instantané du journal au dernier accusé, pour conserver les étapes des workers précédents.
CREATE TABLE job_handoff_steps (
    job_id TEXT NOT NULL REFERENCES jobs(id) ON DELETE CASCADE,
    step_index INTEGER NOT NULL,
    node_id TEXT NOT NULL,
    block_kind TEXT NOT NULL,
    operation_id TEXT,
    status TEXT NOT NULL,
    message TEXT,
    output_excerpt TEXT,
    created_at TEXT NOT NULL,
    PRIMARY KEY (job_id, step_index)
);
