//! Redémarrage d'un worker, détecté par son identifiant de démarrage (ADR 0027).

mod common;

use common::leases::{action, leased_job, output, pool, queued_job};
use forgearr::{
    domain::{IrreversibleAction, JobErrorCode, JobEventType, JobStatus, LibraryOutputPolicy},
    repository,
};
use uuid::Uuid;

#[tokio::test]
async fn only_a_new_boot_ends_the_jobs_the_worker_was_running() -> anyhow::Result<()> {
    let pool = pool().await?;
    let worker = repository::register(&pool, "w").await?;
    let boot = Uuid::new_v4();
    repository::register_boot(&pool, worker.id, boot).await?;
    let (job, _) = leased_job(&pool, "w").await?;
    let waiting = queued_job(&pool, "en-file.mkv").await?;
    let (other, _) = leased_job_of_another_worker(&pool, waiting).await?;

    assert!(
        repository::register_boot(&pool, worker.id, boot)
            .await?
            .is_empty()
    );
    repository::register(&pool, "w").await?;
    assert_eq!(
        repository::get_job(&pool, job).await?.status,
        JobStatus::Running
    );

    let changes = repository::register_boot(&pool, worker.id, Uuid::new_v4()).await?;
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].event, JobEventType::JobRequeued);
    let queued = repository::get_job(&pool, job).await?;
    assert_eq!((queued.status, queued.attempts), (JobStatus::Queued, 1));
    assert_eq!(
        repository::get_job(&pool, other).await?.status,
        JobStatus::Running
    );
    Ok(())
}

async fn leased_job_of_another_worker(
    pool: &sqlx::SqlitePool,
    waiting: Uuid,
) -> anyhow::Result<(Uuid, Uuid)> {
    let other = repository::register(pool, "autre").await?;
    let (job, _) = common::leases::claim(pool, other.id).await?;
    anyhow::ensure!(job == waiting, "tâche inattendue");
    Ok((job, other.id))
}

#[tokio::test]
async fn the_first_known_boot_of_an_upgraded_worker_fails_its_old_jobs() -> anyhow::Result<()> {
    let pool = pool().await?;
    let worker = repository::register(&pool, "ancien").await?;
    let job = queued_job(&pool, "a.mkv").await?;
    repository::claim(&pool, worker.id).await?;
    repository::register_boot(&pool, worker.id, Uuid::new_v4()).await?;
    let failed = repository::get_job(&pool, job).await?;
    assert_eq!(failed.error_code, Some(JobErrorCode::LeaseExpired));
    Ok(())
}

#[tokio::test]
async fn a_restart_during_a_publication_defers_and_frees_the_slot() -> anyhow::Result<()> {
    let pool = pool().await?;
    let (job, lease) = leased_job(&pool, "w").await?;
    let policy = LibraryOutputPolicy::ReplaceInPlace;
    repository::begin_source_publication(&pool, job, &lease, output(), policy).await?;
    let changes = repository::register_boot(&pool, lease.worker_id, Uuid::new_v4()).await?;
    assert!(changes[0].deferred);
    assert!(repository::lease_orphaned(&pool, job).await?);
    assert_eq!(
        repository::get_job(&pool, job).await?.status,
        JobStatus::Running
    );
    queued_job(&pool, "suivante.mkv").await?;
    assert!(repository::claim(&pool, lease.worker_id).await?.is_some());
    // Jamais autorisée : rien n'a été remplacé, la tâche repart.
    let epoch = lease.lease_epoch;
    assert!(repository::abandon_unauthorized_publication(&pool, job, epoch).await?);
    let change = repository::end_orphaned_lease(&pool, (job, epoch)).await?;
    assert_eq!(
        change.map(|change| change.event),
        Some(JobEventType::JobRequeued)
    );
    Ok(())
}

#[tokio::test]
async fn a_restart_after_an_irreversible_action_is_not_requeued() -> anyhow::Result<()> {
    let pool = pool().await?;
    let (job, lease) = leased_job(&pool, "w").await?;
    let call = action(&lease, IrreversibleAction::HttpCall, "webhook");
    repository::authorize_action(&pool, job, &call).await?;
    let changes = repository::register_boot(&pool, lease.worker_id, Uuid::new_v4()).await?;
    assert_eq!(changes[0].event, JobEventType::JobFailed);
    Ok(())
}
