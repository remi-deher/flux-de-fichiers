//! Flux d'échec en attente (ADR 0026) : une perte de bail ou un échec décidé par le central marque
//! la tâche ; le central l'exécute une seule fois ; un échec déjà traité par le worker ne l'est pas.

use forgearr::{
    domain::{
        JobErrorCode, JobStatus, JobStepEntry, JobStepStatus, LeaseRequest, UpdateFlowRequest,
    },
    job_runner::FAILURE_FLOW_STEP_NODE,
    repository::{self, FailureFlowState},
};
use serde_json::json;
use sqlx::{SqlitePool, sqlite::SqlitePoolOptions};
use uuid::Uuid;

const FAILURE_FLOW: &str = "flow.echec";
const SOURCE_BYTES: i64 = 12;

async fn pool() -> anyhow::Result<SqlitePool> {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await?;
    sqlx::migrate!().run(&pool).await?;
    Ok(pool)
}

/// Définit un flux d'échec global : toute tâche créée ensuite le fige.
async fn with_global_failure_flow(pool: &SqlitePool) -> anyhow::Result<()> {
    let request = UpdateFlowRequest {
        name: FAILURE_FLOW.into(),
        description: None,
        file_mask: "*.mkv".into(),
        is_default: false,
        definition: json!({"nodes": [{"id": "in", "type": "file-input", "name": "Entrée"}],
                           "connections": []})
        .to_string(),
    };
    repository::save_flow(pool, FAILURE_FLOW, &request).await?;
    repository::set_default_failure_flow(pool, Some(FAILURE_FLOW)).await?;
    Ok(())
}

/// Crée une tâche et la réserve pour un nouveau worker ; retourne la tâche et son bail.
async fn leased_job(pool: &SqlitePool, name: &str) -> anyhow::Result<(Uuid, LeaseRequest)> {
    let job = repository::insert_job(pool, name, None, &"ab".repeat(32), SOURCE_BYTES).await?;
    let worker = repository::register(pool, &format!("worker-{name}")).await?;
    let claim = repository::claim(pool, worker.id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("bail attendu"))?;
    anyhow::ensure!(claim.job_id == job.id, "tâche inattendue");
    let lease = LeaseRequest {
        worker_id: worker.id,
        lease_token: claim.lease_token,
        lease_epoch: claim.lease_epoch,
        telemetry: None,
    };
    Ok((job.id, lease))
}

/// Fait expirer le bail ; la prochaine réservation constate la perte.
async fn expire_lease(pool: &SqlitePool, job: Uuid, worker: Uuid) -> anyhow::Result<()> {
    sqlx::query("UPDATE jobs SET leased_until=datetime('now', '-1 seconds') WHERE id=?1")
        .bind(job)
        .execute(pool)
        .await?;
    repository::claim(pool, worker).await?;
    Ok(())
}

fn summary_step(index: i32) -> JobStepEntry {
    JobStepEntry {
        step_index: index,
        node_id: FAILURE_FLOW_STEP_NODE.into(),
        block_kind: FAILURE_FLOW_STEP_NODE.into(),
        operation_id: None,
        status: JobStepStatus::Succeeded,
        message: None,
        output_excerpt: None,
        variables: None,
    }
}

#[tokio::test]
async fn a_lost_lease_leaves_the_failure_flow_pending_once() -> anyhow::Result<()> {
    let pool = pool().await?;
    with_global_failure_flow(&pool).await?;
    let (job, lease) = leased_job(&pool, "perdu.mkv").await?;
    assert_eq!(repository::failure_flow_state(&pool, job).await?, None);

    expire_lease(&pool, job, lease.worker_id).await?;
    let failed = repository::get_job(&pool, job).await?;
    assert_eq!(failed.status, JobStatus::Failed);
    assert_eq!(failed.error_code, Some(JobErrorCode::LeaseExpired));
    assert_eq!(
        repository::failure_flow_state(&pool, job).await?,
        Some(FailureFlowState::Pending)
    );

    let (taken, claim) = repository::take_pending_failure_flow(&pool)
        .await?
        .ok_or_else(|| anyhow::anyhow!("flux d'échec en attente attendu"))?;
    assert_eq!(taken.id, job);
    assert_eq!(claim.linked_flows.len(), 1);
    assert_eq!(
        repository::failure_flow_state(&pool, job).await?,
        Some(FailureFlowState::Done)
    );
    // Prise en charge unique : aucune seconde exécution.
    assert!(
        repository::take_pending_failure_flow(&pool)
            .await?
            .is_none()
    );
    // La tâche reste échouée.
    assert_eq!(
        repository::get_job(&pool, job).await?.status,
        JobStatus::Failed
    );
    Ok(())
}

#[tokio::test]
async fn a_failure_flow_already_run_by_the_worker_is_not_repeated() -> anyhow::Result<()> {
    let pool = pool().await?;
    with_global_failure_flow(&pool).await?;
    let (job, lease) = leased_job(&pool, "worker.mkv").await?;
    repository::save_job_steps_for_lease(&pool, job, &lease, &[summary_step(1)]).await?;
    repository::fail(&pool, job, &lease, JobErrorCode::OperationFailed).await?;
    assert_eq!(
        repository::failure_flow_state(&pool, job).await?,
        Some(FailureFlowState::Done)
    );
    assert!(
        repository::take_pending_failure_flow(&pool)
            .await?
            .is_none()
    );
    Ok(())
}

#[tokio::test]
async fn a_flow_run_by_the_worker_before_losing_its_lease_is_not_repeated() -> anyhow::Result<()> {
    let pool = pool().await?;
    with_global_failure_flow(&pool).await?;
    let (job, lease) = leased_job(&pool, "bail-perdu.mkv").await?;
    repository::save_job_steps_for_lease(&pool, job, &lease, &[summary_step(1)]).await?;
    expire_lease(&pool, job, lease.worker_id).await?;
    assert_eq!(
        repository::get_job(&pool, job).await?.status,
        JobStatus::Failed
    );
    assert_eq!(
        repository::failure_flow_state(&pool, job).await?,
        Some(FailureFlowState::Done)
    );
    assert!(
        repository::take_pending_failure_flow(&pool)
            .await?
            .is_none()
    );
    Ok(())
}

#[tokio::test]
async fn an_aborted_central_failure_flow_stays_visible_in_the_journal() -> anyhow::Result<()> {
    let pool = pool().await?;
    let (job, lease) = leased_job(&pool, "abandon.mkv").await?;
    repository::save_job_steps_for_lease(&pool, job, &lease, &[summary_step(1)]).await?;
    repository::record_failure_flow_not_run(&pool, job, "catalogue indisponible").await?;
    let steps = repository::list_job_steps(&pool, job).await?;
    let last = steps
        .last()
        .ok_or_else(|| anyhow::anyhow!("étape attendue"))?;
    assert_eq!(steps.len(), 2);
    assert_eq!(last.node_id, FAILURE_FLOW_STEP_NODE);
    assert_eq!(last.status, JobStepStatus::Failed);
    assert_eq!(
        last.message.as_deref(),
        Some(repository::failure_flow_not_run_message("catalogue indisponible").as_str())
    );
    Ok(())
}

#[tokio::test]
async fn a_worker_failure_outside_the_flow_is_left_pending() -> anyhow::Result<()> {
    let pool = pool().await?;
    with_global_failure_flow(&pool).await?;
    let (job, lease) = leased_job(&pool, "transfert.mkv").await?;
    repository::fail(&pool, job, &lease, JobErrorCode::TransferFailed).await?;
    assert_eq!(
        repository::failure_flow_state(&pool, job).await?,
        Some(FailureFlowState::Pending)
    );
    Ok(())
}

#[tokio::test]
async fn a_job_without_failure_flow_is_never_marked() -> anyhow::Result<()> {
    let pool = pool().await?;
    let (job, lease) = leased_job(&pool, "simple.mkv").await?;
    expire_lease(&pool, job, lease.worker_id).await?;
    assert_eq!(
        repository::get_job(&pool, job).await?.status,
        JobStatus::Failed
    );
    assert_eq!(repository::failure_flow_state(&pool, job).await?, None);
    assert!(
        repository::take_pending_failure_flow(&pool)
            .await?
            .is_none()
    );
    Ok(())
}

#[tokio::test]
async fn a_new_attempt_resets_the_failure_flow_state() -> anyhow::Result<()> {
    let pool = pool().await?;
    with_global_failure_flow(&pool).await?;
    let (job, lease) = leased_job(&pool, "relance.mkv").await?;
    expire_lease(&pool, job, lease.worker_id).await?;
    repository::take_pending_failure_flow(&pool).await?;
    repository::append_job_steps(&pool, job, &[summary_step(1)]).await?;

    repository::retry_job(&pool, job).await?;
    let claim = repository::claim(&pool, lease.worker_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("nouvelle tentative attendue"))?;
    assert_eq!(claim.job_id, job);
    assert_eq!(repository::failure_flow_state(&pool, job).await?, None);
    // La synthèse de l'échec précédent ne fait plus croire que le flux a été exécuté.
    assert!(repository::list_job_steps(&pool, job).await?.is_empty());
    Ok(())
}

#[tokio::test]
async fn central_steps_are_appended_after_the_journal() -> anyhow::Result<()> {
    let pool = pool().await?;
    let (job, lease) = leased_job(&pool, "journal.mkv").await?;
    let mut first = summary_step(1);
    first.node_id = "in".into();
    repository::save_job_steps_for_lease(&pool, job, &lease, &[first]).await?;
    repository::append_job_steps(&pool, job, &[summary_step(1), summary_step(2)]).await?;
    let indexes: Vec<i64> = repository::list_job_steps(&pool, job)
        .await?
        .iter()
        .map(|step| i64::from(step.step_index))
        .collect();
    assert_eq!(indexes, vec![1, 2, 3]);
    Ok(())
}
