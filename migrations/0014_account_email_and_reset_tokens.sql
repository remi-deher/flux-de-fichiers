-- Adresse e-mail facultative des comptes et jetons de réinitialisation du mot de passe (ADR 0015).
ALTER TABLE accounts ADD COLUMN email TEXT;
CREATE UNIQUE INDEX accounts_email_unique ON accounts(email COLLATE NOCASE) WHERE email IS NOT NULL;

-- Seul le haché SHA-256 du jeton est conservé ; un jeton se consomme une fois ou expire.
CREATE TABLE password_reset_tokens (
    token_hash TEXT PRIMARY KEY,
    account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    expires_at TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX password_reset_tokens_account ON password_reset_tokens(account_id);
