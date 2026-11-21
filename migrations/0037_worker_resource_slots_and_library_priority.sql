-- Places CPU/GPU par worker, classe de ressource des tâches et priorité par défaut des bibliothèques.
-- Additive : places NULL = mode historique (`max_concurrency` commun), tâches existantes en `cpu`,
-- bibliothèques à la priorité 0 (`LIBRARY_DEFAULT_JOB_PRIORITY`).
ALTER TABLE workers ADD COLUMN max_cpu_jobs INTEGER CHECK (max_cpu_jobs IS NULL OR max_cpu_jobs BETWEEN 0 AND 16);
ALTER TABLE workers ADD COLUMN max_gpu_jobs INTEGER CHECK (max_gpu_jobs IS NULL OR max_gpu_jobs BETWEEN 0 AND 16);
ALTER TABLE jobs ADD COLUMN resource_class TEXT NOT NULL DEFAULT 'cpu' CHECK (resource_class IN ('cpu', 'gpu'));
ALTER TABLE libraries ADD COLUMN default_priority INTEGER NOT NULL DEFAULT 0 CHECK (default_priority BETWEEN -100 AND 100);

-- Rattrapage des tâches en file : GPU si un bloc du flux figé exige `nvenc` ou `vaapi`
-- (même règle que `flow_resource_class`) ; une définition illisible reste `cpu`.
UPDATE jobs SET resource_class = 'gpu'
WHERE status = 'queued'
  AND EXISTS (
    SELECT 1
    FROM flow_versions v,
         json_each(CASE WHEN json_valid(v.definition) THEN v.definition ELSE '{}' END, '$.nodes') n
    WHERE v.flow_id = jobs.flow_id
      AND v.version = jobs.flow_version
      AND json_extract(n.value, '$.config.acceleration') IN ('nvenc', 'vaapi')
  );
