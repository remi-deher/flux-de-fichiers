-- Transfert direct d'une source entre workers (ADR 0019).
-- Adresse du serveur de transfert annoncée par un worker (facultative).
ALTER TABLE workers ADD COLUMN transfer_url TEXT
    CHECK (transfer_url IS NULL OR length(transfer_url) BETWEEN 8 AND 256);

-- Billets de lecture d'une source : seule l'empreinte du secret est conservée.
CREATE TABLE source_tickets (
    token_hash TEXT PRIMARY KEY,
    job_id TEXT NOT NULL REFERENCES jobs(id) ON DELETE CASCADE,
    source_worker_id TEXT NOT NULL REFERENCES workers(id) ON DELETE CASCADE,
    target_worker_id TEXT NOT NULL REFERENCES workers(id) ON DELETE CASCADE,
    lease_epoch INTEGER NOT NULL,
    expires_at TEXT NOT NULL
);
CREATE INDEX source_tickets_expiry_idx ON source_tickets (expires_at);
