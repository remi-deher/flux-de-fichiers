-- Variables personnalisées (`vars.*`) figées sur une étape de notification différée après publication.
-- Colonnes facultatives : les étapes existantes restent valides (aucune variable).
ALTER TABLE job_steps ADD COLUMN variables_json TEXT;
ALTER TABLE job_handoff_steps ADD COLUMN variables_json TEXT;
