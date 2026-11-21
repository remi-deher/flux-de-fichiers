-- Flux d'échec en attente (ADR 0026) : une tâche qui passe à `failed` sans que son worker ait exécuté
-- le flux d'échec (perte de bail, échec décidé par le central) est marquée `pending` ; le central
-- l'exécute ensuite une seule fois et la marque `done`. Additive : les tâches existantes restent à NULL.
ALTER TABLE jobs ADD COLUMN failure_flow_state TEXT CHECK (failure_flow_state IN ('pending', 'done'));

-- Une seule transition vers `failed` marque la tâche, quel que soit le chemin (échec du worker,
-- perte de bail, échec décidé par le central). Si l'étape de synthèse du flux d'échec
-- (`job_runner::FAILURE_FLOW_STEP_NODE`) est déjà au journal, le worker l'a exécuté : `done`.
CREATE TRIGGER jobs_failure_flow_pending
AFTER UPDATE OF status ON jobs
WHEN NEW.status = 'failed' AND OLD.status <> 'failed' AND NEW.failure_flow_state IS NULL
  AND EXISTS (SELECT 1 FROM job_linked_flows l WHERE l.job_id = NEW.id AND l.role = 'failure')
BEGIN
    UPDATE jobs SET failure_flow_state = CASE
        WHEN EXISTS (SELECT 1 FROM job_steps s WHERE s.job_id = NEW.id AND s.node_id = 'failure-flow')
        THEN 'done' ELSE 'pending' END
    WHERE id = NEW.id;
END;

CREATE INDEX jobs_failure_flow_pending_idx ON jobs(failure_flow_state)
    WHERE failure_flow_state = 'pending';
