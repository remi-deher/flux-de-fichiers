//! Ce que la remise en file oublie et conserve de l'ancien bail (ADR 0027).

mod common;

use common::leases::{claim, declaration, expire, leased_job, pool, queued_job, step};
use forgearr::{
    domain::{JobErrorCode, JobStatus, LeaseRequest},
    repository,
};
use forgearr_shared::{
    jobs::LeaseReachability,
    relay::{RelayRequest, RelayTarget},
    transfer::ArtifactKind,
};
use sqlx::SqlitePool;
use uuid::Uuid;

const MAX_BYTES: u64 = 1024;

async fn relay_to(
    pool: &SqlitePool,
    job: Uuid,
    lease: &LeaseRequest,
    target: Uuid,
) -> anyhow::Result<()> {
    let request = RelayRequest {
        target: RelayTarget::Worker { worker_id: target },
        resume_node_id: "suite".into(),
        variables: Default::default(),
        node_outputs: Default::default(),
    };
    repository::relay_job(pool, job, lease, &request, true).await?;
    Ok(())
}

/// Le worker n'est plus en ligne.
async fn offline(pool: &SqlitePool, worker: Uuid) -> anyhow::Result<()> {
    sqlx::query("UPDATE workers SET last_seen=datetime('now', '-1 day') WHERE id=?1")
        .bind(worker)
        .execute(pool)
        .await?;
    Ok(())
}

#[tokio::test]
async fn artifacts_and_steps_of_the_lost_attempt_are_forgotten() -> anyhow::Result<()> {
    let pool = pool().await?;
    let (job, lease) = leased_job(&pool, "w").await?;
    let (kind, decl) = (ArtifactKind::Result, declaration());
    repository::declare_artifact(&pool, job, &lease, kind, &decl, MAX_BYTES).await?;
    repository::save_job_steps_for_lease(&pool, job, &lease, &[step("perdue")]).await?;
    expire(&pool, job).await?;
    let epoch = lease.lease_epoch;
    assert!(!repository::artifact_retained(&pool, job, kind, lease.worker_id, epoch).await?);
    // Lisibles en file, effacées à la réservation suivante.
    assert_eq!(repository::list_job_steps(&pool, job).await?.len(), 1);
    claim(&pool, lease.worker_id).await?;
    assert!(repository::list_job_steps(&pool, job).await?.is_empty());
    Ok(())
}

#[tokio::test]
async fn an_artifact_relay_restarts_from_the_source_for_any_worker() -> anyhow::Result<()> {
    let pool = pool().await?;
    let (job, origin) = leased_job(&pool, "origine").await?;
    repository::set_transfer_url(&pool, origin.worker_id, Some("http://o.lan:9000")).await?;
    let (kind, decl) = (ArtifactKind::Working, declaration());
    repository::declare_artifact(&pool, job, &origin, kind, &decl, MAX_BYTES).await?;
    let receiver = repository::register(&pool, "repreneur").await?;
    relay_to(&pool, job, &origin, receiver.id).await?;
    claim(&pool, receiver.id).await?;
    expire(&pool, job).await?;
    let queued = repository::get_job(&pool, job).await?;
    assert_eq!(queued.status, JobStatus::Queued);
    assert_eq!(queued.target_worker_id, None);
    assert!(!queued.warning);
    assert!(repository::list_relays(&pool, &[job]).await?.is_empty());
    let again = repository::claim(&pool, origin.worker_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("reprise attendue"))?;
    assert_eq!(again.resume_node_id, None);
    assert!(again.handoff_input.is_none());
    Ok(())
}

#[tokio::test]
async fn a_plain_relay_keeps_its_resume_point_for_an_online_target() -> anyhow::Result<()> {
    let pool = pool().await?;
    let (job, origin) = leased_job(&pool, "origine").await?;
    let target = repository::register(&pool, "cible").await?;
    relay_to(&pool, job, &origin, target.id).await?;
    claim(&pool, target.id).await?;
    expire(&pool, job).await?;
    let queued = repository::get_job(&pool, job).await?;
    assert_eq!(queued.status, JobStatus::Queued);
    assert_eq!(queued.target_worker_id, Some(target.id));
    let again = repository::claim(&pool, target.id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("reprise attendue"))?;
    assert_eq!(again.resume_node_id.as_deref(), Some("suite"));
    Ok(())
}

#[tokio::test]
async fn a_plain_relay_to_a_lost_worker_fails_instead_of_waiting_forever() -> anyhow::Result<()> {
    let pool = pool().await?;
    let (job, origin) = leased_job(&pool, "origine").await?;
    let target = repository::register(&pool, "cible").await?;
    relay_to(&pool, job, &origin, target.id).await?;
    claim(&pool, target.id).await?;
    offline(&pool, target.id).await?;
    expire(&pool, job).await?;
    let failed = repository::get_job(&pool, job).await?;
    assert_eq!(failed.status, JobStatus::Failed);
    assert_eq!(failed.error_code, Some(JobErrorCode::LeaseExpired));
    Ok(())
}

#[tokio::test]
async fn a_worker_chosen_by_the_user_is_kept_and_its_absence_is_visible() -> anyhow::Result<()> {
    let pool = pool().await?;
    let (job, lease) = leased_job(&pool, "choisi").await?;
    sqlx::query("UPDATE jobs SET target_worker_id=?2 WHERE id=?1")
        .bind(job)
        .bind(lease.worker_id)
        .execute(&pool)
        .await?;
    offline(&pool, lease.worker_id).await?;
    expire(&pool, job).await?;
    let queued = repository::get_job(&pool, job).await?;
    assert_eq!(queued.status, JobStatus::Queued);
    assert_eq!(queued.target_worker_id, Some(lease.worker_id));
    let infos = repository::lease_infos(&pool, &[job]).await?;
    assert_eq!(infos[0].reachability, LeaseReachability::AwaitingWorker);
    Ok(())
}

#[tokio::test]
async fn a_relay_by_an_old_worker_forbids_the_requeue_of_its_successor() -> anyhow::Result<()> {
    let pool = pool().await?;
    let origin = repository::register(&pool, "ancien").await?;
    repository::set_transfer_url(&pool, origin.id, Some("http://o.lan:9000")).await?;
    let job = queued_job(&pool, "film.mkv").await?;
    let legacy = repository::claim(&pool, origin.id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("bail attendu"))?;
    let lease = LeaseRequest {
        worker_id: origin.id,
        lease_token: legacy.lease_token,
        lease_epoch: legacy.lease_epoch,
        telemetry: None,
    };
    let (kind, decl) = (ArtifactKind::Working, declaration());
    repository::declare_artifact(&pool, job, &lease, kind, &decl, MAX_BYTES).await?;
    let receiver = repository::register(&pool, "a-jour").await?;
    relay_to(&pool, job, &lease, receiver.id).await?;
    claim(&pool, receiver.id).await?;
    expire(&pool, job).await?;
    let failed = repository::get_job(&pool, job).await?;
    assert_eq!(failed.status, JobStatus::Failed);
    assert_eq!(failed.error_code, Some(JobErrorCode::LeaseExpired));
    // L'administrateur assume la répétition : la perte suivante est reprise.
    repository::retry_job(&pool, job).await?;
    claim(&pool, receiver.id).await?;
    expire(&pool, job).await?;
    let queued = repository::get_job(&pool, job).await?;
    assert_eq!(queued.status, JobStatus::Queued);
    Ok(())
}

#[tokio::test]
async fn an_undelivered_lease_forgets_nothing_of_a_relay() -> anyhow::Result<()> {
    let pool = pool().await?;
    let (job, origin) = leased_job(&pool, "origine").await?;
    repository::set_transfer_url(&pool, origin.worker_id, Some("http://o.lan:9000")).await?;
    let (kind, decl) = (ArtifactKind::Working, declaration());
    repository::declare_artifact(&pool, job, &origin, kind, &decl, MAX_BYTES).await?;
    let receiver = repository::register(&pool, "repreneur").await?;
    relay_to(&pool, job, &origin, receiver.id).await?;
    let (_, lease) = claim(&pool, receiver.id).await?;
    repository::undeliver_lease(&pool, job, &lease).await?;
    let again = repository::claim(&pool, receiver.id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("reprise attendue"))?;
    assert_eq!(again.resume_node_id.as_deref(), Some("suite"));
    assert!(again.handoff_input.is_some());
    Ok(())
}
