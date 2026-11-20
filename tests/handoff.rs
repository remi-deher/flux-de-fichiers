//! Remise d'un fichier de travail : le bail reste chez l'émetteur jusqu'à l'accusé vérifié.

use forgearr::{
    domain::{JobStatus, JobStepEntry, JobStepStatus, LeaseRequest},
    error::AppError,
    repository,
};
use forgearr_shared::relay::{HandoffAcknowledgement, HandoffArtifact, RelayRequest, RelayTarget};
use sqlx::{SqlitePool, sqlite::SqlitePoolOptions};
use uuid::Uuid;

async fn pool() -> anyhow::Result<SqlitePool> {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await?;
    sqlx::migrate!().run(&pool).await?;
    Ok(pool)
}

async fn pending(pool: &SqlitePool) -> anyhow::Result<(Uuid, LeaseRequest, Uuid, HandoffArtifact)> {
    let sender = repository::register(pool, "sender").await?;
    let receiver = repository::register(pool, "receiver").await?;
    let job = repository::insert_job(pool, "film.mkv", None, &"ab".repeat(32), 10).await?;
    let claim = repository::claim(pool, sender.id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("bail absent"))?;
    let lease = LeaseRequest {
        worker_id: sender.id,
        lease_token: claim.lease_token,
        lease_epoch: claim.lease_epoch,
        telemetry: None,
    };
    let request = RelayRequest {
        target: RelayTarget::Worker {
            worker_id: receiver.id,
        },
        resume_node_id: "next".into(),
    };
    let artifact = HandoffArtifact {
        name: "working.mkv".into(),
        sha256: "cd".repeat(32),
        bytes: 42,
    };
    assert!(
        repository::begin_handoff(pool, job.id, &lease, &request, &artifact, 1024, true).await?
    );
    assert!(
        repository::claim_handoff(pool, receiver.id)
            .await?
            .is_none()
    );
    assert_eq!(
        repository::handoff_upload_artifact(pool, job.id, &lease).await?,
        artifact
    );
    repository::finish_handoff_upload(pool, job.id, &lease).await?;
    Ok((job.id, lease, receiver.id, artifact))
}

#[tokio::test]
async fn verified_ack_transfers_the_job_only_after_receipt() -> anyhow::Result<()> {
    let pool = pool().await?;
    let (job_id, lease, receiver, artifact) = pending(&pool).await?;
    let step = |node: &str| JobStepEntry {
        step_index: 1,
        node_id: node.into(),
        block_kind: "operation".into(),
        operation_id: None,
        status: JobStepStatus::Succeeded,
        message: None,
        output_excerpt: None,
    };
    repository::save_job_steps_for_lease(&pool, job_id, &lease, &[step("before")]).await?;
    assert_eq!(
        repository::get_job(&pool, job_id).await?.status,
        JobStatus::Running
    );
    let offer = repository::claim_handoff(&pool, receiver)
        .await?
        .ok_or_else(|| anyhow::anyhow!("offre absente"))?;
    assert_eq!(offer.artifact, artifact);
    assert!(repository::retained_handoff_needed(&pool, job_id, receiver, lease.lease_epoch).await?);
    assert!(repository::claim_handoff(&pool, receiver).await?.is_none());
    let mut ack = HandoffAcknowledgement {
        worker_id: receiver,
        receipt_token: offer.receipt_token,
        sha256: artifact.sha256.clone(),
        bytes: artifact.bytes,
    };
    assert!(
        repository::handoff_download_artifact(&pool, job_id, receiver, "incorrect")
            .await
            .is_err()
    );
    assert_eq!(
        repository::handoff_download_artifact(&pool, job_id, receiver, &ack.receipt_token)
            .await?
            .1,
        artifact
    );
    ack.sha256 = "ef".repeat(32);
    assert!(matches!(
        repository::acknowledge_handoff(&pool, job_id, &ack).await,
        Err(AppError::Conflict)
    ));
    assert_eq!(
        repository::get_job(&pool, job_id).await?.worker_id,
        Some(lease.worker_id)
    );
    ack.sha256 = artifact.sha256.clone();
    assert!(repository::acknowledge_handoff(&pool, job_id, &ack).await?);
    let queued = repository::get_job(&pool, job_id).await?;
    assert_eq!(queued.status, JobStatus::Queued);
    assert_eq!(queued.target_worker_id, Some(receiver));
    assert!(repository::retained_handoff_needed(&pool, job_id, receiver, lease.lease_epoch).await?);
    assert!(
        !repository::retained_handoff_needed(&pool, job_id, Uuid::new_v4(), lease.lease_epoch)
            .await?
    );
    assert!(!repository::acknowledge_handoff(&pool, job_id, &ack).await?);
    assert!(matches!(
        repository::save_job_steps_for_lease(&pool, job_id, &lease, &[step("stale")]).await,
        Err(AppError::Conflict)
    ));
    let resumed = repository::claim(&pool, receiver)
        .await?
        .ok_or_else(|| anyhow::anyhow!("reprise absente"))?;
    assert_eq!(resumed.resume_node_id.as_deref(), Some("next"));
    assert!(resumed.prior_warning);
    assert_eq!(
        resumed
            .handoff_input
            .as_ref()
            .map(|input| input.artifact.clone()),
        Some(artifact)
    );
    let next_lease = LeaseRequest {
        worker_id: receiver,
        lease_token: resumed.lease_token,
        lease_epoch: resumed.lease_epoch,
        telemetry: None,
    };
    repository::save_job_steps_for_lease(&pool, job_id, &next_lease, &[step("after")]).await?;
    repository::save_job_steps_for_lease(&pool, job_id, &next_lease, &[step("after")]).await?;
    let steps = repository::list_job_steps(&pool, job_id).await?;
    assert_eq!(
        steps
            .iter()
            .map(|entry| (entry.step_index, entry.node_id.as_str()))
            .collect::<Vec<_>>(),
        [(1, "before"), (2, "after")]
    );
    Ok(())
}

#[tokio::test]
async fn an_expired_origin_lease_cannot_be_acknowledged() -> anyhow::Result<()> {
    let pool = pool().await?;
    let (job_id, _, receiver, artifact) = pending(&pool).await?;
    let offer = repository::claim_handoff(&pool, receiver)
        .await?
        .ok_or_else(|| anyhow::anyhow!("offre absente"))?;
    sqlx::query("UPDATE jobs SET leased_until=datetime('now', '-1 second') WHERE id=?1")
        .bind(job_id)
        .execute(&pool)
        .await?;
    let ack = HandoffAcknowledgement {
        worker_id: receiver,
        receipt_token: offer.receipt_token,
        sha256: artifact.sha256,
        bytes: artifact.bytes,
    };
    assert!(matches!(
        repository::acknowledge_handoff(&pool, job_id, &ack).await,
        Err(AppError::Conflict)
    ));
    assert!(repository::claim(&pool, receiver).await?.is_none());
    let failed = repository::get_job(&pool, job_id).await?;
    assert_eq!(failed.status, JobStatus::Failed);
    assert_eq!(
        failed.error_code,
        Some(forgearr::domain::JobErrorCode::LeaseExpired)
    );
    Ok(())
}

#[tokio::test]
async fn manual_retry_after_receiver_lease_loss_starts_from_original_source() -> anyhow::Result<()>
{
    let pool = pool().await?;
    let (job_id, _, receiver, artifact) = pending(&pool).await?;
    let offer = repository::claim_handoff(&pool, receiver)
        .await?
        .ok_or_else(|| anyhow::anyhow!("offre absente"))?;
    let ack = HandoffAcknowledgement {
        worker_id: receiver,
        receipt_token: offer.receipt_token,
        sha256: artifact.sha256,
        bytes: artifact.bytes,
    };
    assert!(repository::acknowledge_handoff(&pool, job_id, &ack).await?);
    let _claim = repository::claim(&pool, receiver)
        .await?
        .ok_or_else(|| anyhow::anyhow!("bail absent"))?;
    sqlx::query("UPDATE jobs SET leased_until=datetime('now', '-1 second') WHERE id=?1")
        .bind(job_id)
        .execute(&pool)
        .await?;
    assert!(repository::claim(&pool, receiver).await?.is_none());
    let retry_job = repository::retry_job(&pool, job_id).await?;
    assert!(retry_job.target_worker_id.is_none());
    assert!(!retry_job.warning);
    let retry = repository::claim(&pool, receiver)
        .await?
        .ok_or_else(|| anyhow::anyhow!("relance absente"))?;
    assert!(retry.resume_node_id.is_none());
    assert!(retry.handoff_input.is_none());
    Ok(())
}
