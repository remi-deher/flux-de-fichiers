-- Annulation active ('canceled'), priorité des tâches, et concurrence / fenêtre horaire des workers.
CREATE TABLE job_steps_backup (
    id INTEGER PRIMARY KEY,
    job_id TEXT NOT NULL,
    step_index INTEGER NOT NULL,
    node_id TEXT NOT NULL,
    block_kind TEXT NOT NULL,
    operation_id TEXT,
    status TEXT NOT NULL,
    message TEXT,
    output_excerpt TEXT,
    created_at TEXT NOT NULL
);

INSERT INTO job_steps_backup (id, job_id, step_index, node_id, block_kind, operation_id, status, message, output_excerpt, created_at)
SELECT id, job_id, step_index, node_id, block_kind, operation_id, status, message, output_excerpt, created_at FROM job_steps;

DROP TABLE job_steps;

CREATE TABLE jobs_next (
    id TEXT PRIMARY KEY,
    source_name TEXT NOT NULL,
    target_worker_id TEXT REFERENCES workers(id),
    worker_id TEXT REFERENCES workers(id),
    status TEXT NOT NULL DEFAULT 'queued'
        CHECK (status IN ('queued', 'running', 'succeeded', 'failed', 'canceled')),
    attempts INTEGER NOT NULL DEFAULT 0,
    lease_token TEXT,
    leased_until TEXT,
    output_name TEXT,
    error_code TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    source_sha256 TEXT,
    source_bytes INTEGER,
    output_sha256 TEXT,
    output_bytes INTEGER,
    warning BOOLEAN NOT NULL DEFAULT 0,
    flow_id TEXT,
    flow_version INTEGER,
    library_id TEXT REFERENCES libraries(id) ON DELETE SET NULL,
    lease_epoch INTEGER NOT NULL DEFAULT 0,
    priority INTEGER NOT NULL DEFAULT 0
);

INSERT INTO jobs_next (
    id, source_name, target_worker_id, worker_id, status, attempts, lease_token, leased_until,
    output_name, error_code, created_at, updated_at, source_sha256, source_bytes,
    output_sha256, output_bytes, warning, flow_id, flow_version, library_id, lease_epoch
)
SELECT
    id, source_name, target_worker_id, worker_id, status, attempts, lease_token, leased_until,
    output_name, error_code, created_at, updated_at, source_sha256, source_bytes,
    output_sha256, output_bytes, warning, flow_id, flow_version, library_id, lease_epoch
FROM jobs;

DROP TABLE jobs;
ALTER TABLE jobs_next RENAME TO jobs;

CREATE INDEX jobs_claim_idx ON jobs (priority DESC, created_at ASC, id ASC) WHERE status = 'queued';
CREATE INDEX jobs_expired_idx ON jobs (leased_until) WHERE status = 'running';
CREATE INDEX jobs_recent_idx ON jobs (created_at DESC, id);
CREATE UNIQUE INDEX jobs_output_name_idx ON jobs(output_name) WHERE output_name IS NOT NULL;
CREATE INDEX jobs_library_idx ON jobs (library_id);
CREATE INDEX jobs_worker_recent_idx ON jobs (worker_id, created_at DESC, id DESC);
CREATE INDEX jobs_flow_idx ON jobs (flow_id);

CREATE TABLE job_steps (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    job_id TEXT NOT NULL REFERENCES jobs(id) ON DELETE CASCADE,
    step_index INTEGER NOT NULL,
    node_id TEXT NOT NULL,
    block_kind TEXT NOT NULL,
    operation_id TEXT,
    status TEXT NOT NULL CHECK (status IN ('succeeded', 'failed', 'skipped')),
    message TEXT,
    output_excerpt TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

INSERT INTO job_steps (id, job_id, step_index, node_id, block_kind, operation_id, status, message, output_excerpt, created_at)
SELECT id, job_id, step_index, node_id, block_kind, operation_id, status, message, output_excerpt, created_at FROM job_steps_backup;

DROP TABLE job_steps_backup;
CREATE INDEX job_steps_job_idx ON job_steps (job_id, step_index);

ALTER TABLE workers ADD COLUMN max_concurrency INTEGER NOT NULL DEFAULT 1;
ALTER TABLE workers ADD COLUMN schedule_json TEXT;
