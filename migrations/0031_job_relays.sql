-- Relais d'une tâche vers la file d'un autre worker (ADR 0018).
-- La cible réutilise `jobs.target_worker_id` ; cette table garde la marque « relayée ».
CREATE TABLE job_relays (
    job_id TEXT PRIMARY KEY REFERENCES jobs(id) ON DELETE CASCADE,
    from_worker_id TEXT NOT NULL REFERENCES workers(id) ON DELETE CASCADE,
    relay_count INTEGER NOT NULL CHECK (relay_count >= 1),
    resume_node_id TEXT NOT NULL CHECK (length(resume_node_id) BETWEEN 1 AND 128),
    relayed_at TEXT NOT NULL DEFAULT (datetime('now'))
);
