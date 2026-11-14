-- Vocabulaire central / worker (ADR 0013) : les chemins de bibliothèque sont déclarés par worker.
ALTER TABLE library_nodes RENAME TO library_workers;
DROP INDEX library_nodes_worker_idx;
CREATE INDEX library_workers_worker_idx ON library_workers (worker_id);
