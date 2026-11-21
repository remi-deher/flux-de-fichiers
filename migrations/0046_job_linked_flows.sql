-- Flux liés figés avec une tâche (ADR 0026) : flux d'échec et sous-flux.
-- Additive : une tâche existante n'a aucun flux lié, elle s'exécute comme avant.
-- Sans clé étrangère vers flow_versions : une version figée reste lisible après suppression du flux.
CREATE TABLE job_linked_flows (
    job_id TEXT NOT NULL REFERENCES jobs(id) ON DELETE CASCADE,
    role TEXT NOT NULL CHECK (role IN ('failure', 'sub_flow')),
    flow_id TEXT NOT NULL,
    version INTEGER NOT NULL CHECK (version >= 1),
    PRIMARY KEY (job_id, role, flow_id)
);
