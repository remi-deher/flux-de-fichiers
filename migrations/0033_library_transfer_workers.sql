-- Workers « par transfert » d'une bibliothèque (ADR 0021) : ils traitent ses tâches sans dossier
-- déclaré et reçoivent les sources d'un worker à accès direct.
CREATE TABLE library_transfer_workers (
    library_id TEXT NOT NULL REFERENCES libraries(id) ON DELETE CASCADE,
    worker_id TEXT NOT NULL REFERENCES workers(id) ON DELETE CASCADE,
    PRIMARY KEY (library_id, worker_id)
);
CREATE INDEX library_transfer_workers_worker_idx ON library_transfer_workers (worker_id);
