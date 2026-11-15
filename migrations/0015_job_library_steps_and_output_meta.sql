-- Liaison optionnelle d'une tâche à une bibliothèque et journal d'exécution des blocs de flux.
ALTER TABLE jobs ADD COLUMN library_id TEXT REFERENCES libraries(id) ON DELETE SET NULL;
CREATE INDEX jobs_library_idx ON jobs (library_id);

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
CREATE INDEX job_steps_job_idx ON job_steps (job_id, step_index);
