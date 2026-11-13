-- Versions de flows : chaque enregistrement qui change le contenu crée une version immuable.
-- Une tâche retient la version du flow par défaut en vigueur à sa création ; modifier un flow
-- ne touche ni ne relance les tâches existantes.
ALTER TABLE flows ADD COLUMN version INTEGER NOT NULL DEFAULT 1;

-- Sans clé étrangère : une version reste lisible par ses tâches après suppression du flow.
CREATE TABLE flow_versions (
    flow_id TEXT NOT NULL,
    version INTEGER NOT NULL CHECK (version >= 1),
    name TEXT NOT NULL,
    description TEXT,
    file_mask TEXT NOT NULL,
    definition TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (flow_id, version)
);

INSERT INTO flow_versions (flow_id, version, name, description, file_mask, definition, created_at)
SELECT id, 1, name, description, file_mask, definition, updated_at FROM flows;

-- Seule la définition peut être réécrite, pour rechiffrer ses secrets (rotation de clé).
CREATE TRIGGER flow_versions_immutable
BEFORE UPDATE OF flow_id, version, name, description, file_mask ON flow_versions
BEGIN
    SELECT RAISE(ABORT, 'flow_version_immutable');
END;

CREATE TRIGGER flow_versions_no_delete
BEFORE DELETE ON flow_versions
BEGIN
    SELECT RAISE(ABORT, 'flow_version_immutable');
END;

ALTER TABLE jobs ADD COLUMN flow_id TEXT;
ALTER TABLE jobs ADD COLUMN flow_version INTEGER;
