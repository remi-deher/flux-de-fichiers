-- Sorties des nœuds exécutés avant un relais (ADR 0018, ADR 0025) : le repreneur les relit par
-- `{{nodes.<id>.message|nextPort}}`. JSON `{ "<nœud>": { "message", "nextPort" } }`, absent si aucune.
ALTER TABLE job_relays ADD COLUMN node_outputs_json TEXT;
