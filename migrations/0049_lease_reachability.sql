-- État injoignable et remise en file (ADR 0027). Additive : la table `jobs` n'est pas reconstruite.

-- Dernier contact du titulaire (réservation, battement ou accord).
ALTER TABLE jobs ADD COLUMN lease_contact_at TEXT;
-- Début de l'injoignabilité ; n'a de sens qu'en `running`.
ALTER TABLE jobs ADD COLUMN unreachable_since TEXT;
-- 0 : worker antérieur à l'ADR 0027 (jamais de remise en file automatique).
ALTER TABLE jobs ADD COLUMN lease_protocol INTEGER NOT NULL DEFAULT 0 CHECK (lease_protocol >= 0);
-- Le processus titulaire n'existe plus, mais une publication en cours reste à conclure : la tâche
-- n'occupe plus de place.
ALTER TABLE jobs ADD COLUMN lease_orphaned INTEGER NOT NULL DEFAULT 0 CHECK (lease_orphaned IN (0, 1));
-- Restitutions qui ont rendu leur tentative, bornées : une tâche qui fait arrêter son worker à
-- chaque fois finit par épuiser ses tentatives.
ALTER TABLE jobs ADD COLUMN free_releases INTEGER NOT NULL DEFAULT 0 CHECK (free_releases >= 0);
-- La tâche a été relayée sous un bail d'ancien protocole : ce qu'il a exécuté n'est inscrit nulle
-- part, donc aucune reprise automatique, même si le bail courant est récent.
ALTER TABLE jobs ADD COLUMN ungated_lease INTEGER NOT NULL DEFAULT 0 CHECK (ungated_lease IN (0, 1));
UPDATE jobs SET ungated_lease = 1 WHERE id IN (SELECT job_id FROM job_relays);

-- Identifiant de démarrage du dernier processus connu de ce worker.
ALTER TABLE workers ADD COLUMN boot_id TEXT;

-- Accord donné à l'exécutant du remplacement. Les intentions antérieures sont tenues pour autorisées.
ALTER TABLE source_publication_journal ADD COLUMN authorized_at TEXT;
UPDATE source_publication_journal SET authorized_at = created_at;

CREATE INDEX jobs_running_contact_idx ON jobs(lease_contact_at) WHERE status = 'running';

-- Actions irréversibles autorisées sous un bail.
CREATE TABLE job_irreversible_actions (
    job_id TEXT NOT NULL REFERENCES jobs(id) ON DELETE CASCADE,
    lease_epoch INTEGER NOT NULL,
    action TEXT NOT NULL CHECK (action IN ('in_place', 'http_call', 'failure_flow')),
    node_id TEXT NOT NULL CHECK (length(node_id) BETWEEN 1 AND 128),
    error_code TEXT,
    started_at TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (job_id, lease_epoch, action, node_id)
);
