CREATE TABLE library_folders (
    library_id TEXT NOT NULL REFERENCES libraries(id) ON DELETE CASCADE,
    worker_id TEXT REFERENCES workers(id) ON DELETE CASCADE,
    local_path TEXT NOT NULL CHECK (length(local_path) BETWEEN 1 AND 512),
    access TEXT NOT NULL CHECK (access IN ('read', 'read_write'))
);

CREATE UNIQUE INDEX library_folders_location_idx
    ON library_folders (library_id, coalesce(worker_id, zeroblob(16)), local_path);
CREATE INDEX library_folders_worker_idx ON library_folders(worker_id);

INSERT INTO library_folders SELECT library_id, worker_id, local_path, access FROM library_workers;
DROP TABLE library_workers;

ALTER TABLE jobs ADD COLUMN source_root TEXT;
ALTER TABLE jobs ADD COLUMN source_worker_id TEXT;
ALTER TABLE jobs ADD COLUMN source_mtime_ns INTEGER;
ALTER TABLE jobs ADD COLUMN source_writable INTEGER NOT NULL DEFAULT 1;

CREATE INDEX jobs_library_source_history_idx ON jobs (library_id, source_root, source_worker_id, source_name, created_at);
