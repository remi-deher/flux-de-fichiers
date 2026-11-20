-- L'ancienne remise par téléversement vers le central (ADR 0020) est remplacée par le relais par
-- artefact (ADR 0022) : plus aucun octet ne transite par le central. Le journal des étapes
-- (job_handoff_steps) reste utilisé par le relais par artefact.
DROP TABLE job_handoffs;
