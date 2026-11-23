//! Flux d'échec en attente (ADR 0026) : tâches échouées hors exécution sur un worker (perte de
//! bail, échec décidé par le central) dont le flux d'échec reste à exécuter par le central.
//!
//! La migration 0048 marque `pending` toute tâche qui passe à `failed` avec un flux d'échec figé,
//! sauf si le worker l'a déjà exécuté (étape de synthèse au journal : le trigger fixe `done`). [`take_pending_failure_flow`]
//! passe la tâche à `done` avant l'exécution : au plus une exécution par échec, même après un
//! redémarrage ou avec plusieurs exécuteurs. Un worker qui exécute lui-même le flux d'échec obtient
//! d'abord l'accord du central, qui pose `done` sous son bail (ADR 0027).

use super::{
    flow_usage::{TaskFlow, failure_not_held_sql, held_json},
    leases::LeaseKey,
};
use crate::{
    domain::{Claim, Job, JobStatus, JobStepEntry, JobStepStatus},
    error::{AppError, AppResult},
    job_runner::{FAILURE_FLOW_STEP_NODE, failure_flow_summary_step},
};
use sqlx::{SqliteConnection, SqlitePool};
use uuid::Uuid;

/// État du flux d'échec d'une tâche (`jobs.failure_flow_state`, NULL : rien à faire).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FailureFlowState {
    /// La tâche a échoué sans que le flux d'échec ait été exécuté.
    Pending,
    /// Le flux d'échec a été exécuté (ou pris en charge) pour cet échec.
    Done,
}

impl FailureFlowState {
    pub const ALL: [Self; 2] = [Self::Pending, Self::Done];

    /// Valeur stockée en base.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Done => "done",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|state| state.as_str() == value)
    }
}

/// Nouvelle tentative : ni l'échec précédent ni les étapes de la tentative perdue ne valent plus
/// (celles d'un relais par artefact sont conservées).
pub(super) async fn reset_for_new_attempt(conn: &mut SqliteConnection, job: Uuid) -> AppResult<()> {
    forget_failure(conn, job).await?;
    super::relays::reset_steps(conn, job).await
}

/// L'échec précédent et la synthèse de son flux d'échec ne valent plus.
pub(super) async fn forget_failure(conn: &mut SqliteConnection, job: Uuid) -> AppResult<()> {
    sqlx::query("UPDATE jobs SET failure_flow_state=NULL WHERE id=?1")
        .bind(job)
        .execute(&mut *conn)
        .await?;
    sqlx::query("DELETE FROM job_steps WHERE job_id=?1 AND node_id=?2")
        .bind(job)
        .bind(FAILURE_FLOW_STEP_NODE)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

/// État du flux d'échec de la tâche.
pub async fn failure_flow_state(
    pool: &SqlitePool,
    job: Uuid,
) -> AppResult<Option<FailureFlowState>> {
    let value: Option<Option<String>> =
        sqlx::query_scalar("SELECT failure_flow_state FROM jobs WHERE id=?1")
            .bind(job)
            .fetch_optional(pool)
            .await?;
    let value = value.ok_or(AppError::NotFound)?;
    Ok(value.as_deref().and_then(FailureFlowState::parse))
}

/// Prend en charge la plus ancienne tâche en attente : elle passe à `done` dans la même requête,
/// puis la tâche est relue avec le contexte nécessaire à l'exécution de son flux d'échec.
pub async fn take_pending_failure_flow(pool: &SqlitePool) -> AppResult<Option<(Job, Claim)>> {
    take_failure_flow_except(pool, &[]).await
}

/// [`take_pending_failure_flow`], sauf ceux qui se servent d'une version retenue : ils restent en attente.
pub async fn take_failure_flow_except(
    pool: &SqlitePool,
    held: &[TaskFlow],
) -> AppResult<Option<(Job, Claim)>> {
    let taken: Option<(Uuid, i64)> = sqlx::query_as(&format!(
        "UPDATE jobs SET failure_flow_state=?1 \
         WHERE id=(SELECT id FROM jobs WHERE failure_flow_state=?2 AND status=?3 AND {not_held} \
                   ORDER BY updated_at, id LIMIT 1) \
           AND failure_flow_state=?2 \
         RETURNING id, lease_epoch",
        not_held = failure_not_held_sql("?4"),
    ))
    .bind(FailureFlowState::Done.as_str())
    .bind(FailureFlowState::Pending.as_str())
    .bind(JobStatus::Failed)
    .bind(held_json(held)?)
    .fetch_optional(pool)
    .await?;
    let Some((id, epoch)) = taken else {
        return Ok(None);
    };
    let context = async {
        let job = super::get_job(pool, id).await?;
        let claim = failure_claim(pool, &job).await?;
        AppResult::Ok((job, claim))
    }
    .await;
    match context {
        // Relancée entre-temps : cet échec ne compte plus.
        Ok((job, _)) if job.lease_epoch != epoch => Ok(None),
        Ok(context) => Ok(Some(context)),
        Err(error) => {
            // La tâche est déjà `done` : la perte du flux d'échec reste visible au journal.
            record_failure_flow_not_run(pool, (id, epoch), &error.to_string()).await?;
            Err(error)
        }
    }
}

/// Rend un flux d'échec pris en charge mais pas exécuté, pour un motif passager : il reste à exécuter. `false` :
/// la tâche a été relancée depuis.
pub async fn give_back_failure_flow(pool: &SqlitePool, (job, epoch): LeaseKey) -> AppResult<bool> {
    let given_back = sqlx::query(
        "UPDATE jobs SET failure_flow_state=?3 \
         WHERE id=?1 AND lease_epoch=?2 AND status=?4 AND failure_flow_state=?5",
    )
    .bind(job)
    .bind(epoch)
    .bind(FailureFlowState::Pending.as_str())
    .bind(JobStatus::Failed)
    .bind(FailureFlowState::Done.as_str())
    .execute(pool)
    .await?;
    Ok(given_back.rows_affected() > 0)
}

/// Rang de la première étape ajoutée par [`append_job_steps`] (numérotée après le journal).
const FIRST_APPENDED_STEP_INDEX: i32 = 1;

/// Motif journalisé quand le flux d'échec n'a pas pu démarrer.
pub fn failure_flow_not_run_message(reason: &str) -> String {
    format!("flux d'échec non exécuté par le central : {reason}")
}

/// Journalise une étape de synthèse en échec pour un flux d'échec pris en charge (`done`) mais
/// abandonné avant son exécution : sans nouvelle tentative, la perte reste visible.
pub async fn record_failure_flow_not_run(
    pool: &SqlitePool,
    failed: LeaseKey,
    reason: &str,
) -> AppResult<bool> {
    let step = failure_flow_summary_step(
        FIRST_APPENDED_STEP_INDEX,
        JobStepStatus::Failed,
        failure_flow_not_run_message(reason),
    );
    append_failure_steps(pool, failed, &[step]).await
}

/// Contexte d'exécution du flux d'échec par le central : sans bail ni flux principal, seulement
/// les flux liés figés, la bibliothèque et l'identité de la tâche.
async fn failure_claim(pool: &SqlitePool, job: &Job) -> AppResult<Claim> {
    Ok(Claim {
        job_id: job.id,
        lease_token: Uuid::nil(),
        lease_epoch: job.lease_epoch,
        source_name: job.source_name.clone(),
        source_sha256: job.source_sha256.clone().unwrap_or_default(),
        source_bytes: job.source_bytes.unwrap_or_default(),
        probe_tool_id: None,
        probe_version: None,
        required_tools: Vec::new(),
        library_id: job.library_id,
        frozen_flow: None,
        operations: Default::default(),
        block_operations: Default::default(),
        plugin_configs: Default::default(),
        sealed_secrets: None,
        allow_private_integrations: None,
        media_root: None,
        local_source_root: None,
        resume_node_id: None,
        resumed_outputs: Default::default(),
        handoff_input: None,
        prior_warning: job.warning,
        source_ticket: None,
        library_name: super::step_context::library_name(pool, job.library_id).await?,
        worker_name: None,
        resume_variables: Default::default(),
        linked_flows: super::linked_flows::frozen_linked_flows(pool, job.id).await?,
        lease_terms: None,
    })
}

/// Ajoute des étapes à la fin du journal d'un échec précis. `false` : la tâche a été relancée depuis,
/// les étapes sont abandonnées plutôt que mêlées à la nouvelle tentative.
pub async fn append_failure_steps(
    pool: &SqlitePool,
    (job, epoch): LeaseKey,
    steps: &[JobStepEntry],
) -> AppResult<bool> {
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    let current: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM jobs WHERE id=?1 AND lease_epoch=?2 AND status=?3)",
    )
    .bind(job)
    .bind(epoch)
    .bind(JobStatus::Failed)
    .fetch_one(&mut *tx)
    .await?;
    if !current {
        return Ok(false);
    }
    let prefix: i64 =
        sqlx::query_scalar("SELECT COALESCE(MAX(step_index), 0) FROM job_steps WHERE job_id=?1")
            .bind(job)
            .fetch_one(&mut *tx)
            .await?;
    for step in steps {
        super::jobs::insert_job_step(&mut tx, job, prefix, step).await?;
    }
    tx.commit().await?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn states_round_trip_through_their_stored_value() {
        for state in FailureFlowState::ALL {
            assert_eq!(FailureFlowState::parse(state.as_str()), Some(state));
        }
        assert_eq!(FailureFlowState::parse("running"), None);
    }

    #[test]
    fn an_aborted_failure_flow_names_its_reason() {
        let message = failure_flow_not_run_message("catalogue indisponible");
        assert!(message.contains("non exécuté"));
        assert!(message.ends_with("catalogue indisponible"));
    }
}
