ALTER TABLE jobs ADD COLUMN source_sha256 TEXT;
ALTER TABLE jobs ADD COLUMN source_bytes BIGINT;
ALTER TABLE jobs ADD COLUMN output_sha256 TEXT;
ALTER TABLE jobs ADD COLUMN output_bytes BIGINT;
ALTER TABLE jobs ADD COLUMN warning BOOLEAN NOT NULL DEFAULT false;
-- Existing incomplete prototype jobs cannot be executed without an input fingerprint.
UPDATE jobs SET status = 'failed', error_code = 'missing_source_fingerprint'
WHERE source_sha256 IS NULL AND status IN ('queued', 'running');
CREATE UNIQUE INDEX jobs_output_name_idx ON jobs(output_name) WHERE output_name IS NOT NULL;
