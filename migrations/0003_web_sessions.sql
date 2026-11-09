-- Web sessions are opaque, revocable credentials. Store only their CSRF verifier.
ALTER TABLE sessions ADD COLUMN csrf_hash TEXT;
