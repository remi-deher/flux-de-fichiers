-- Relais en cours de flux par artefact (ADR 0022) : le repreneur tire le fichier de travail chez son
-- détenteur. La reprise à un nœud du flux n'a de sens que tant que l'artefact est déclaré ; sinon
-- (reprise après perte du repreneur) la tâche repart de la source.
ALTER TABLE job_relays ADD COLUMN artifact_relay INTEGER NOT NULL DEFAULT 0
    CHECK (artifact_relay IN (0, 1));
