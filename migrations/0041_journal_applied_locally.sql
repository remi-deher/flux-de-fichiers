-- Distingue le remplacement que le worker qui traite applique lui-même (confirmé par
-- `result/applied`) de celui qu'un autre détenteur tire et applique (confirmé par sa réponse).
ALTER TABLE source_publication_journal ADD COLUMN applied_locally INTEGER NOT NULL DEFAULT 0
    CHECK (applied_locally IN (0, 1));
