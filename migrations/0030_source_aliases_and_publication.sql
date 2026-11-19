ALTER TABLE library_folders ADD COLUMN source_key TEXT;
ALTER TABLE jobs ADD COLUMN source_key TEXT;
CREATE INDEX jobs_logical_source_idx ON jobs (library_id, source_key, source_name);

CREATE TABLE job_source_mounts (
    job_id TEXT NOT NULL REFERENCES jobs(id) ON DELETE CASCADE,
    worker_id TEXT NOT NULL,
    local_path TEXT NOT NULL,
    writable INTEGER NOT NULL,
    PRIMARY KEY (job_id, worker_id, local_path)
);

CREATE VIEW verified_job_source_mounts AS
SELECT m.job_id, m.worker_id, m.local_path, (f.access = 'read_write') AS writable
FROM job_source_mounts m
JOIN jobs j ON j.id = m.job_id
JOIN library_folders f ON f.library_id = j.library_id
    AND f.source_key = j.source_key
    AND coalesce(f.worker_id, zeroblob(16)) = m.worker_id
    AND f.local_path = m.local_path;

ALTER TABLE job_steps ADD COLUMN notification_retry_until TEXT;
ALTER TABLE job_steps ADD COLUMN deferred_notification INTEGER NOT NULL DEFAULT 0;
