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
/// `worker` est un paramètre SQL (`?3`…). Le worker intégré est celui qui sert les dossiers du
/// central (`workers.serves_central`), quel que soit son nom.
pub fn job_access_sql(worker: &str) -> String {
    format!(
        "((jobs.source_key IS NULL AND (jobs.source_worker_id IS NULL OR jobs.source_worker_id={worker})) \
          OR (jobs.source_key IS NOT NULL AND ( \
              EXISTS (SELECT 1 FROM verified_job_source_mounts m \
                      WHERE m.job_id=jobs.id AND m.worker_id={worker}) \
              OR (EXISTS (SELECT 1 FROM verified_job_source_mounts m \
                          WHERE m.job_id=jobs.id AND m.worker_id=zeroblob(16)) \
                  AND EXISTS (SELECT 1 FROM workers iw WHERE iw.id={worker} AND iw.serves_central=1)) \
              OR EXISTS (SELECT 1 FROM library_transfer_workers t \
                         WHERE t.library_id=jobs.library_id AND t.worker_id={worker}))))"
    )
}

/// Expression SQL booléenne : le worker `worker` peut-il lire la source de la tâche (ADR 0022) ?
/// Oui si le central la voit, si le worker la monte lui-même, ou si un détenteur en ligne annonce une
/// adresse de transfert où la tirer. Un détenteur sans adresse ne sert personne d'autre que lui.
/// `worker` et `online_seconds` sont des paramètres ou des constantes SQL.
pub fn source_readable_sql(worker: &str, online_seconds: &str) -> String {
    format!(
        "((jobs.source_key IS NULL AND jobs.source_worker_id IS NULL) \
          OR jobs.source_worker_id={worker} \
          OR EXISTS (SELECT 1 FROM verified_job_source_mounts m \
                     WHERE m.job_id=jobs.id AND (m.worker_id=zeroblob(16) OR m.worker_id={worker})) \
          OR EXISTS (SELECT 1 FROM verified_job_source_mounts m JOIN workers o ON o.id=m.worker_id \
                     WHERE m.job_id=jobs.id AND o.transfer_url IS NOT NULL \
                       AND o.last_seen > datetime('now', '-' || {online_seconds} || ' seconds')) \
          OR EXISTS (SELECT 1 FROM workers o WHERE o.id=jobs.source_worker_id \
                     AND o.transfer_url IS NOT NULL \
                     AND o.last_seen > datetime('now', '-' || {online_seconds} || ' seconds')))"
    )
}
