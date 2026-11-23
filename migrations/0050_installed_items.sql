-- Éléments installés (ADR 0029). Additive. Sans clé étrangère vers `plugin_repositories` : l'origine
-- reste lisible après le retrait du dépôt. La base fait autorité sur `/config/installed`, dont chaque
-- dossier se déduit de `item_id` et `content_sha256`.
CREATE TABLE installed_items (
    item_id TEXT PRIMARY KEY NOT NULL CHECK (length(item_id) BETWEEN 1 AND 64),
    -- Un pack n'a pas d'état propre ; un bloc suit son propriétaire.
    kind TEXT NOT NULL CHECK (kind NOT IN ('pack', 'block')),
    version TEXT NOT NULL,
    content_sha256 TEXT NOT NULL CHECK (length(content_sha256) = 64),
    -- Origine : le couple (identifiant, adresse). Le nom ne sert qu'à l'affichage.
    repository_id TEXT NOT NULL,
    repository_url TEXT NOT NULL,
    repository_name TEXT NOT NULL,
    -- Flux : dernier flux créé ; script : identifiant dans la collection locale ; NULL sinon.
    copy_id TEXT,
    -- 1 : la copie modifiable reste à écrire depuis la copie figée (rejoué au démarrage).
    copy_pending INTEGER NOT NULL DEFAULT 0 CHECK (copy_pending IN (0, 1)),
    -- 1 : porte une sonde ou une opération par défaut, donc sert à toute tâche. Illisible, sa copie ne le dirait
    -- plus.
    serves_every_task INTEGER NOT NULL DEFAULT 0 CHECK (serves_every_task IN (0, 1)),
    installed_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE UNIQUE INDEX installed_items_copy_idx ON installed_items(kind, copy_id) WHERE copy_id IS NOT NULL;

-- Dépôt du plugin pour lequel les réglages ont été saisis ; NULL : inconnu.
ALTER TABLE plugin_settings ADD COLUMN origin_repository_id TEXT;
ALTER TABLE plugin_settings ADD COLUMN origin_repository_url TEXT;
