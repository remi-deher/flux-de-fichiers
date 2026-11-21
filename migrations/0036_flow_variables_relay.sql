-- Variables personnalisées du flux (`vars.*`), transmises au worker qui reprend une tâche relayée.
-- Colonnes facultatives : les relais et remises existants restent valides (aucune variable).
ALTER TABLE job_relays ADD COLUMN variables_json TEXT;
ALTER TABLE job_handoffs ADD COLUMN variables_json TEXT;
