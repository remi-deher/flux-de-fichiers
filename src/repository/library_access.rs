//! Règle d'accès d'un worker aux tâches d'une bibliothèque (ADR 0021), partagée par `claim` et
//! le choix de cible d'un relais.

/// Expression SQL booléenne sur la table `jobs` : le worker `worker` peut-il traiter la tâche ?
///
/// - tâche sans dossier configuré (`source_key` nul) : règle historique (source du central, ou
///   détenue par ce worker) ;
/// - tâche de bibliothèque : le worker a un montage vérifié (accès direct), ou c'est le worker
///   intégré et la source est celle de la centrale, ou il est « par transfert » dans la
///   bibliothèque, quelle que soit la politique de sortie.
///
/// `worker` et `integrated` sont des paramètres SQL (`?3`, `?4`…).
pub fn job_access_sql(worker: &str, integrated: &str) -> String {
    format!(
        "((jobs.source_key IS NULL AND (jobs.source_worker_id IS NULL OR jobs.source_worker_id={worker})) \
          OR (jobs.source_key IS NOT NULL AND ( \
              EXISTS (SELECT 1 FROM verified_job_source_mounts m \
                      WHERE m.job_id=jobs.id AND m.worker_id={worker}) \
              OR (EXISTS (SELECT 1 FROM verified_job_source_mounts m \
                          WHERE m.job_id=jobs.id AND m.worker_id=zeroblob(16)) \
                  AND EXISTS (SELECT 1 FROM workers iw WHERE iw.id={worker} AND iw.name={integrated})) \
              OR EXISTS (SELECT 1 FROM library_transfer_workers t \
                         WHERE t.library_id=jobs.library_id AND t.worker_id={worker}))))"
    )
}

/// Expression SQL booléenne : la source de la tâche est-elle joignable (central, ou au moins un
/// détenteur en ligne) ? `online_seconds` est un paramètre SQL.
pub fn source_reachable_sql(online_seconds: &str) -> String {
    format!(
        "((jobs.source_key IS NULL AND jobs.source_worker_id IS NULL) \
          OR EXISTS (SELECT 1 FROM verified_job_source_mounts m \
                     WHERE m.job_id=jobs.id AND m.worker_id=zeroblob(16)) \
          OR EXISTS (SELECT 1 FROM verified_job_source_mounts m JOIN workers o ON o.id=m.worker_id \
                     WHERE m.job_id=jobs.id \
                       AND o.last_seen > datetime('now', '-' || {online_seconds} || ' seconds')) \
          OR EXISTS (SELECT 1 FROM workers o WHERE o.id=jobs.source_worker_id \
                     AND o.last_seen > datetime('now', '-' || {online_seconds} || ' seconds')))"
    )
}
