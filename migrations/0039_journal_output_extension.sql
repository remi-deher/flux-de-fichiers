-- Un résultat remplacé n'a plus de copie au central : le journal garde l'extension du résultat,
-- et `output_name` reste vide.
ALTER TABLE source_publication_journal ADD COLUMN output_extension TEXT NOT NULL DEFAULT '';
