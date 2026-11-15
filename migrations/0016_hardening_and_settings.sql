-- Durcissement des baux (fencing epoch), télémétrie/adresse/version des workers et indexation SQLite.
ALTER TABLE jobs ADD COLUMN lease_epoch INTEGER NOT NULL DEFAULT 0;

CREATE INDEX IF NOT EXISTS jobs_worker_recent_idx ON jobs (worker_id, created_at DESC, id DESC);
CREATE INDEX IF NOT EXISTS jobs_flow_idx ON jobs (flow_id);

ALTER TABLE workers ADD COLUMN version TEXT;
ALTER TABLE workers ADD COLUMN remote_addr TEXT;
ALTER TABLE workers ADD COLUMN telemetry_json TEXT;
