//! Vérification en lecture seule des publications interrompues sur un détenteur distant.

use super::{AppState, source_recovery};
use crate::{
    domain::{Job, JobStatus, LeaseRequest},
    error::{AppError, AppResult},
    repository, storage,
};
use forgearr_shared::transfer::{SourcePublicationOutcome, SourcePublicationTask};
use futures_util::{StreamExt, stream};
use std::time::Duration;
use uuid::Uuid;

const INSPECTION_TIMEOUT: Duration = Duration::from_secs(15);
const RETRY_DELAY: Duration = Duration::from_secs(2);
const MAX_RECOVERY_DURATION: Duration = Duration::from_secs(120);
const MAX_RECOVERY_CONCURRENCY: usize = 8;

async fn inspect_holders(
    s: &AppState,
    job: &Job,
    lease: &LeaseRequest,
    pending: &repository::PendingSourcePublication,
) -> AppResult<bool> {
    let extension = pending.output_extension.as_str();
    if !storage::valid_output_extension(extension) {
        return Ok(false);
    }
    let Some(source_sha256) = job.source_sha256.as_ref() else {
        return Ok(false);
    };
    let Some(source_bytes) = job.source_bytes else {
        return Ok(false);
    };
    let output = pending
        .has_central_copy()
        .then(|| s.config.data_dir.join("output").join(&pending.output_name));
    for holder in repository::writable_source_holders(&s.pool, job.id, Uuid::nil()).await? {
        let task = SourcePublicationTask {
            request_id: Uuid::new_v4(),
            worker_id: holder.worker_id,
            root: holder.root.clone(),
            source_name: job.source_name.clone(),
            source_sha256: source_sha256.clone(),
            source_bytes,
            result_sha256: pending.output_sha256.clone(),
            result_bytes: pending.output_bytes,
            extension: extension.to_owned(),
            policy: pending.output_policy,
            result_ticket: None,
        };
        let (job_id, lease) = (job.id, lease.clone());
        let inspection = &s.source_inspection;
        let result = match output.clone() {
            Some(staged) => {
                inspection
                    .request(task, staged, job_id, lease, INSPECTION_TIMEOUT)
                    .await
            }
            None => {
                inspection
                    .request_pulled(task, job_id, lease, INSPECTION_TIMEOUT)
                    .await
            }
        };
        if matches!(result, Ok(SourcePublicationOutcome::Published)) {
            let still_authorized =
                repository::writable_source_holders(&s.pool, job.id, Uuid::nil())
                    .await?
                    .iter()
                    .any(|current| {
                        current.worker_id == holder.worker_id && current.root == holder.root
                    });
            if still_authorized && source_recovery::central_copy_matches(s, pending).await? {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

async fn try_recover(
    s: &AppState,
    pending: &repository::PendingSourcePublication,
) -> AppResult<bool> {
    let job = repository::get_job(&s.pool, pending.job_id).await?;
    let (Some(worker_id), Some(lease_token)) = (job.worker_id, job.lease_token) else {
        return Ok(false);
    };
    if job.status != JobStatus::Running
        || job.lease_epoch != pending.lease_epoch
        || (pending.has_central_copy() && !storage::valid_name(&pending.output_name))
    {
        return Ok(false);
    }
    let lease = LeaseRequest {
        worker_id,
        lease_token,
        lease_epoch: pending.lease_epoch,
        telemetry: None,
    };
    if !source_recovery::central_copy_matches(s, pending).await? {
        return Ok(false);
    }
    if repository::job_source(&s.pool, job.id)
        .await?
        .is_none_or(|source| source.source_key.is_none() && source.source_worker_id.is_none())
    {
        return Ok(false);
    }
    let deadline = tokio::time::Instant::now() + MAX_RECOVERY_DURATION;
    loop {
        if repository::active(&s.pool, job.id, &lease).await.is_err() {
            return Ok(false);
        }
        if inspect_holders(s, &job, &lease, pending).await? {
            source_recovery::finalize(s, &job, &lease, pending).await?;
            return Ok(true);
        }
        if tokio::time::Instant::now() >= deadline {
            return Ok(false);
        }
        tokio::time::sleep(RETRY_DELAY).await;
    }
}

pub(super) async fn recover_one(
    s: &AppState,
    pending: repository::PendingSourcePublication,
) -> AppResult<()> {
    match try_recover(s, &pending).await {
        Ok(true) => {
            tracing::info!(job_id=%pending.job_id, "publication distante confirmée après redémarrage")
        }
        Ok(false)
        | Err(AppError::NotFound | AppError::Invalid | AppError::Conflict | AppError::Storage(_)) =>
        {
            repository::mark_source_publication_uncertain(
                &s.pool,
                pending.job_id,
                pending.lease_epoch,
            )
            .await?;
        }
        Err(error) => return Err(error),
    }
    Ok(())
}

/// Ne traite que les intentions capturées au démarrage, jamais les publications nouvelles.
pub(super) async fn recover(s: AppState, pending: Vec<repository::PendingSourcePublication>) {
    let mut tasks = stream::iter(pending.into_iter().map(|pending| {
        let state = s.clone();
        async move {
            let deadline = tokio::time::Instant::now() + MAX_RECOVERY_DURATION;
            loop {
                match recover_one(&state, pending.clone()).await {
                    Ok(()) => return Ok(()),
                    Err(error) if tokio::time::Instant::now() < deadline => {
                        tracing::warn!(job_id=%pending.job_id, error=%error, "reprise distante temporairement indisponible");
                        tokio::time::sleep(RETRY_DELAY).await;
                    }
                    Err(error) => return Err(error),
                }
            }
        }
    }))
    .buffer_unordered(MAX_RECOVERY_CONCURRENCY);
    while let Some(result) = tasks.next().await {
        if let Err(error) = result {
            tracing::error!(error=%error, "reprise de publication distante impossible");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::LibraryOutputPolicy;
    use forgearr_shared::transfer::SourcePublicationReport;

    #[tokio::test]
    async fn holder_confirmation_finishes_only_the_recorded_active_job() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let (state, job, lease, holder) = super::super::transfers::tests::setup(dir.path()).await?;
        let output = state.config.data_dir.join("output/result.mkv");
        tokio::fs::write(&output, b"result").await?;
        let (hash, bytes) = storage::fingerprint(&output, state.config.max_file_bytes).await?;
        repository::begin_source_publication(
            &state.pool,
            job.id,
            &lease,
            repository::PublishedOutput {
                name: "result.mkv",
                sha256: &hash,
                bytes: bytes as i64,
                warning: false,
            },
            LibraryOutputPolicy::ReplaceWithBackup,
        )
        .await?;
        let pending = repository::pending_source_publications(&state.pool)
            .await?
            .pop()
            .ok_or_else(|| anyhow::anyhow!("journal absent"))?;
        let recovering_state = state.clone();
        let recovering = tokio::spawn(async move { recover_one(&recovering_state, pending).await });
        let tasks = state
            .source_inspection
            .take_for_worker(holder, Duration::from_secs(5))
            .await;
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].result_sha256, hash);
        assert!(
            state
                .source_inspection
                .report(SourcePublicationReport {
                    request_id: tasks[0].request_id,
                    worker_id: holder,
                    outcome: SourcePublicationOutcome::Published,
                })
                .await
        );
        recovering.await??;
        assert_eq!(
            repository::get_job(&state.pool, job.id).await?.status,
            JobStatus::Succeeded
        );
        Ok(())
    }
}
