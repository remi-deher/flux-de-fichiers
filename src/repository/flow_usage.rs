//! Définitions de flux qui servent encore : flux actuels et tâches non terminées (ADR 0029). Lues dans la
//! connexion de l'appelant, pour qu'un refus de désinstaller porte sur l'état qu'il valide.
//!
//! Requêtes sous forme runtime : elles joignent des tables que les macros ne vérifieraient pas ensemble.

use crate::{
    error::{AppError, AppResult},
    flows::publication::DEFERRED_MESSAGE,
};
use sqlx::{SqliteConnection, SqlitePool};

/// Définition d'un flux actuel.
#[derive(Clone, Debug, PartialEq, Eq, sqlx::FromRow)]
pub struct FlowDefinition {
    pub name: String,
    pub definition: String,
}

/// Version figée d'un flux ; `None` : tâches sans flux.
pub type TaskFlow = Option<(String, i64)>;

/// Version figée dont des tâches non terminées se servent.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnfinishedFlow {
    /// Fichiers des tâches.
    pub sources: Vec<String>,
    /// `None` : tâches sans flux, traitées par l'opération par défaut.
    pub definition: Option<String>,
}

pub async fn flow_definitions(conn: &mut SqliteConnection) -> AppResult<Vec<FlowDefinition>> {
    Ok(
        sqlx::query_as("SELECT name, definition FROM flows ORDER BY name, id")
            .fetch_all(conn)
            .await?,
    )
}

/// Tâches en attente ou en cours, et tâches finies dont une suite reste à exécuter : flux d'échec en
/// attente, notifications différées à envoyer ou relançables. Une ligne par (version de flux, fichier).
const UNFINISHED_FLOWS: &str = "\
SELECT j.flow_id, j.flow_version, j.source_name FROM jobs j
WHERE j.status IN ('queued', 'running')
   OR (j.status = 'succeeded' AND j.output_name IS NOT NULL AND EXISTS (
        SELECT 1 FROM job_steps s WHERE s.job_id = j.id
          AND ((s.status = 'skipped' AND s.message = ?1)
               OR (s.deferred_notification = 1 AND s.status IN ('failed', 'skipped')))))
UNION
SELECT l.flow_id, l.version, j.source_name FROM job_linked_flows l JOIN jobs j ON j.id = l.job_id
WHERE j.status IN ('queued', 'running')
   OR (j.status = 'failed' AND j.failure_flow_state = 'pending')
ORDER BY 1, 2, 3";

pub async fn unfinished_flows(conn: &mut SqliteConnection) -> AppResult<Vec<UnfinishedFlow>> {
    let rows: Vec<(Option<String>, Option<i64>, String)> = sqlx::query_as(UNFINISHED_FLOWS)
        .bind(DEFERRED_MESSAGE)
        .fetch_all(&mut *conn)
        .await?;
    let mut grouped: Vec<(TaskFlow, Vec<String>)> = Vec::new();
    for (flow_id, version, source) in rows {
        let key = flow_id.zip(version);
        match grouped.last_mut() {
            Some((known, sources)) if *known == key => sources.push(source),
            _ => grouped.push((key, vec![source])),
        }
    }
    let mut flows = Vec::with_capacity(grouped.len());
    for (key, sources) in grouped {
        let definition = match &key {
            Some((flow_id, version)) => {
                let found: Option<String> = sqlx::query_scalar(
                    "SELECT definition FROM flow_versions WHERE flow_id = ?1 AND version = ?2",
                )
                .bind(flow_id)
                .bind(version)
                .fetch_optional(&mut *conn)
                .await?;
                // Version introuvable : la tâche échouera sans rien utiliser.
                let Some(found) = found else {
                    continue;
                };
                Some(found)
            }
            None => None,
        };
        flows.push(UnfinishedFlow {
            sources,
            definition,
        });
    }
    Ok(flows)
}

/// Version figée que des tâches en file, ou des flux d'échec en attente, vont utiliser.
#[derive(Clone, Debug, PartialEq, Eq, sqlx::FromRow)]
pub struct AwaitingFlow {
    pub flow_id: String,
    pub version: i64,
    pub definition: String,
}

pub async fn awaiting_flows(pool: &SqlitePool) -> AppResult<Vec<AwaitingFlow>> {
    Ok(sqlx::query_as(
        "SELECT v.flow_id, v.version, v.definition FROM flow_versions v \
         WHERE EXISTS (SELECT 1 FROM jobs j WHERE j.status = 'queued' \
                       AND j.flow_id = v.flow_id AND j.flow_version = v.version) \
            OR EXISTS (SELECT 1 FROM job_linked_flows l JOIN jobs j ON j.id = l.job_id \
                       WHERE l.flow_id = v.flow_id AND l.version = v.version \
                         AND (j.status = 'queued' \
                              OR (j.status = 'failed' AND j.failure_flow_state = 'pending'))) \
         ORDER BY v.flow_id, v.version",
    )
    .fetch_all(pool)
    .await?)
}

/// Liste JSON des versions retenues, pour [`not_held_sql`] et [`failure_not_held_sql`].
pub(super) fn held_json(held: &[TaskFlow]) -> AppResult<String> {
    serde_json::to_string(held).map_err(|_| AppError::Internal)
}

/// Un flux lié de la tâche `jobs` est-il l'élément `held` de la liste ?
const LINKED_FLOW_HELD: &str = "EXISTS (SELECT 1 FROM job_linked_flows l WHERE l.job_id = jobs.id \
     AND l.flow_id = json_extract(held.value, '$[0]') AND l.version = json_extract(held.value, '$[1]'))";

/// Condition SQL : ni le flux de la tâche `jobs` ni ses flux liés ne sont dans la liste JSON `parameter`.
pub(super) fn not_held_sql(parameter: &str) -> String {
    format!(
        "NOT EXISTS (SELECT 1 FROM json_each({parameter}) held \
         WHERE (jobs.flow_id IS json_extract(held.value, '$[0]') \
                AND jobs.flow_version IS json_extract(held.value, '$[1]')) \
            OR {LINKED_FLOW_HELD})"
    )
}

/// Comme [`not_held_sql`] pour le flux d'échec d'une tâche `jobs`, exécuté sans son flux principal.
pub(super) fn failure_not_held_sql(parameter: &str) -> String {
    format!(
        "NOT EXISTS (SELECT 1 FROM json_each({parameter}) held \
         WHERE held.value IS NULL OR {LINKED_FLOW_HELD})"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{domain::UpdateFlowRequest, repository};
    use forgearr_shared::jobs::LeaseProtocol;
    use sqlx::sqlite::SqlitePoolOptions;

    const DEFINITION: &str = r#"{"nodes":[],"connections":[]}"#;

    async fn pool() -> anyhow::Result<SqlitePool> {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await?;
        sqlx::migrate!().run(&pool).await?;
        Ok(pool)
    }

    async fn sources(pool: &SqlitePool) -> AppResult<Vec<String>> {
        let mut conn = pool.acquire().await?;
        let flows = unfinished_flows(&mut conn).await?;
        Ok(flows.into_iter().flat_map(|flow| flow.sources).collect())
    }

    async fn set_status(pool: &SqlitePool, assignment: &str) -> AppResult<()> {
        sqlx::query(&format!("UPDATE jobs SET {assignment}"))
            .execute(pool)
            .await?;
        Ok(())
    }

    #[tokio::test]
    async fn only_unfinished_tasks_hold_their_frozen_version() -> anyhow::Result<()> {
        let pool = pool().await?;
        let flow = UpdateFlowRequest {
            name: "Encodage".to_owned(),
            description: None,
            file_mask: "*".to_owned(),
            definition: DEFINITION.to_owned(),
            is_default: true,
        };
        repository::save_flow(&pool, "flux.a", &flow).await?;
        let hash = "ab".repeat(32);
        let job = repository::insert_job(&pool, "film.mkv", None, &hash, 1).await?;
        let mut conn = pool.acquire().await?;
        assert_eq!(flow_definitions(&mut conn).await?[0].name, "Encodage");
        let unfinished = unfinished_flows(&mut conn).await?;
        assert_eq!(unfinished.len(), 1);
        assert_eq!(unfinished[0].sources, ["film.mkv"]);
        assert_eq!(unfinished[0].definition.as_deref(), Some(DEFINITION));
        drop(conn);

        set_status(&pool, "status = 'succeeded', output_name = 'film.mkv'").await?;
        assert_eq!(sources(&pool).await?, Vec::<String>::new());

        // Notification différée, puis relançable, puis envoyée.
        sqlx::query(
            "INSERT INTO job_steps (job_id, step_index, node_id, block_kind, status, message) \
             VALUES (?1, 1, 'n', 'sonarr.rescan', 'skipped', ?2)",
        )
        .bind(job.id)
        .bind(DEFERRED_MESSAGE)
        .execute(&pool)
        .await?;
        assert_eq!(sources(&pool).await?, ["film.mkv"]);
        for (status, held) in [("failed", 1), ("succeeded", 0)] {
            sqlx::query("UPDATE job_steps SET status = ?1, deferred_notification = 1")
                .bind(status)
                .execute(&pool)
                .await?;
            assert_eq!(sources(&pool).await?.len(), held, "{status}");
        }
        Ok(())
    }

    #[tokio::test]
    async fn held_tasks_stay_queued_and_the_others_are_reserved() -> anyhow::Result<()> {
        let pool = pool().await?;
        let worker = repository::register(&pool, "w").await?;
        let hash = "ab".repeat(32);
        let without_flow = repository::insert_job(&pool, "sans-flux.mkv", None, &hash, 1).await?;
        let flow = UpdateFlowRequest {
            name: "Flux".to_owned(),
            description: None,
            file_mask: "*".to_owned(),
            definition: DEFINITION.to_owned(),
            is_default: true,
        };
        repository::save_flow(&pool, "flux.a", &flow).await?;
        let with_flow = repository::insert_job(&pool, "avec-flux.mkv", None, &hash, 1).await?;
        let version = with_flow.flow_version.unwrap_or_default();
        let held: [TaskFlow; 2] = [None, Some(("flux.a".to_owned(), version))];
        let protocol = LeaseProtocol::LATEST;
        let none = repository::claim_except(&pool, worker.id, protocol, &held).await?;
        assert!(none.is_none());
        let first = repository::claim_except(&pool, worker.id, protocol, &held[..1]).await?;
        assert_eq!(first.map(|claim| claim.job_id), Some(with_flow.id));
        let waiting = repository::get_job(&pool, without_flow.id).await?;
        assert_eq!(waiting.attempts, 0);
        Ok(())
    }

    #[tokio::test]
    async fn a_pending_failure_flow_still_holds_its_linked_version() -> anyhow::Result<()> {
        let pool = pool().await?;
        let failure = UpdateFlowRequest {
            name: "Échec".to_owned(),
            description: None,
            file_mask: "*".to_owned(),
            definition: DEFINITION.to_owned(),
            is_default: false,
        };
        repository::save_flow(&pool, "flux.echec", &failure).await?;
        repository::set_default_failure_flow(&pool, Some("flux.echec")).await?;
        repository::insert_job(&pool, "film.mkv", None, &"ab".repeat(32), 1).await?;
        let mut conn = pool.acquire().await?;
        let queued = unfinished_flows(&mut conn).await?;
        drop(conn);
        let definitions: Vec<_> = queued
            .iter()
            .map(|flow| flow.definition.as_deref())
            .collect();
        assert_eq!(definitions, [None, Some(DEFINITION)]);
        assert_eq!(awaiting_flows(&pool).await?.len(), 1);

        set_status(&pool, "status = 'failed'").await?;
        assert_eq!(sources(&pool).await?, ["film.mkv"]);
        let awaiting = awaiting_flows(&pool).await?;
        assert_eq!(awaiting.len(), 1);
        assert_eq!(awaiting[0].flow_id, "flux.echec");

        // Retenu, il reste en attente ; rendu, il repart.
        let version = awaiting[0].version;
        for held in [Some(("flux.echec".to_owned(), version)), None] {
            let taken = repository::take_failure_flow_except(&pool, &[held]).await?;
            assert!(taken.is_none());
        }
        let other = [Some(("flux.echec".to_owned(), version + 1))];
        for _ in 0..2 {
            let taken = repository::take_failure_flow_except(&pool, &other).await?;
            let Some((job, _)) = taken else {
                anyhow::bail!("flux d'échec non pris");
            };
            let failed = (job.id, job.lease_epoch);
            assert!(repository::give_back_failure_flow(&pool, failed).await?);
            assert!(!repository::give_back_failure_flow(&pool, failed).await?);
        }
        set_status(&pool, "failure_flow_state = 'done'").await?;
        assert!(awaiting_flows(&pool).await?.is_empty());
        assert_eq!(sources(&pool).await?, Vec::<String>::new());
        Ok(())
    }
}
