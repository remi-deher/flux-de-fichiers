-- Le résultat préparé est conservé lorsqu'une interruption rend le remplacement incertain.
CREATE TABLE source_publication_journal (
    job_id TEXT NOT NULL REFERENCES jobs(id) ON DELETE CASCADE,
    lease_epoch INTEGER NOT NULL,
    output_name TEXT NOT NULL,
    output_sha256 TEXT NOT NULL,
    output_bytes INTEGER NOT NULL,
    output_warning INTEGER NOT NULL CHECK (output_warning IN (0, 1)),
    output_policy TEXT NOT NULL CHECK (output_policy IN ('replace_with_backup', 'replace_in_place')),
    state TEXT NOT NULL CHECK (state IN ('in_progress', 'uncertain')),
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (job_id, lease_epoch)
);
CREATE INDEX source_publication_journal_state ON source_publication_journal(state);
