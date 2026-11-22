//! Vérification en lecture seule des publications interrompues sur un détenteur distant.

use super::{AppState, source_recovery, transfers};
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
        let result = inspection
            .request(task, job_id, lease, INSPECTION_TIMEOUT)
            .await;
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
    // Le worker qui applique lui-même confirme ou perd son bail : seule sa disparition mérite
    // une vérification.
    if pending.applied_locally && awaits_its_holder(s, &pending).await? {
        return Ok(());
    }
    match try_recover(s, &pending).await {
        Ok(true) => {
            tracing::info!(job_id=%pending.job_id, "publication distante confirmée après redémarrage")
        }
        Ok(false)
        | Err(
            AppError::NotFound
            | AppError::Invalid
            | AppError::Conflict
            | AppError::LeaseLost
            | AppError::Storage(_),
        ) => {
            if drop_unauthorized(s, &pending).await? {
                return Ok(());
            }
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

/// Le titulaire de ce remplacement local est-il encore attendu ?
async fn awaits_its_holder(
    s: &AppState,
    pending: &repository::PendingSourcePublication,
) -> AppResult<bool> {
    let job = repository::get_job(&s.pool, pending.job_id).await?;
    let held = job.status == JobStatus::Running && job.lease_epoch == pending.lease_epoch;
    Ok(held && !repository::lease_orphaned(&s.pool, pending.job_id).await?)
}

/// [`transfers::drop_unauthorized`] pour une intention relue au journal.
async fn drop_unauthorized(
    s: &AppState,
    pending: &repository::PendingSourcePublication,
) -> AppResult<bool> {
    let key = (pending.job_id, pending.lease_epoch);
    transfers::drop_unauthorized(s, key, &pending.output_name).await
}

/// Vérifie la publication d'une tâche dont le worker a redémarré. Sans preuve, l'échéance conclut.
/// Une publication que le central mène encore se conclut d'elle-même.
pub(super) async fn recover_deferred(s: AppState, job: Uuid) {
    if let Err(error) = try_recover_deferred(&s, job).await {
        tracing::warn!(job_id=%job, %error, "vérification de la publication interrompue impossible");
    }
}

async fn try_recover_deferred(s: &AppState, job: Uuid) -> AppResult<()> {
    let pending = repository::pending_source_publications(&s.pool).await?;
    let current = pending.into_iter().find(|pending| pending.job_id == job);
    let Some(pending) = current else {
        return Ok(());
    };
    if s.publications.contains((job, pending.lease_epoch)) || drop_unauthorized(s, &pending).await?
    {
        return Ok(());
    }
    if try_recover(s, &pending).await? {
        tracing::info!(job_id=%job, "publication confirmée après redémarrage du worker");
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

    /// Publication tirée, inscrite puis laissée sans titulaire par le redémarrage de son worker.
    async fn deferred(state: &AppState, job: &Job, lease: &LeaseRequest) -> anyhow::Result<()> {
        transfers::tests::current_protocol(state, job.id).await?;
        let output = repository::PublishedOutput {
            name: "",
            sha256: &"cd".repeat(32),
            bytes: 6,
            warning: false,
        };
        let policy = LibraryOutputPolicy::ReplaceWithBackup;
        repository::begin_pulled_publication(&state.pool, job.id, lease, output, "mkv", policy)
            .await?;
        let boot = Uuid::new_v4();
        repository::register_boot(&state.pool, lease.worker_id, boot).await?;
        Ok(())
    }

    #[tokio::test]
    async fn an_orphaned_intent_never_granted_requeues_the_job() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let (state, job, lease, _) = transfers::tests::setup(dir.path()).await?;
        deferred(&state, &job, &lease).await?;
        recover_deferred(state.clone(), job.id).await;
        let status = repository::get_job(&state.pool, job.id).await?.status;
        assert_eq!(status, JobStatus::Queued);
        Ok(())
    }

    #[tokio::test]
    async fn a_publication_still_running_is_left_alone() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let (state, job, lease, _) = transfers::tests::setup(dir.path()).await?;
        deferred(&state, &job, &lease).await?;
        let key = (job.id, lease.lease_epoch);
        let publications = state.publications.clone();
        let running =
            tokio::spawn(async move { publications.run(key, std::future::pending()).await });
        while !state.publications.contains(key) {
            tokio::task::yield_now().await;
        }
        recover_deferred(state.clone(), job.id).await;
        assert_eq!(
            repository::pending_source_publications(&state.pool)
                .await?
                .len(),
            1
        );
        running.abort();
        Ok(())
    }

    #[tokio::test]
    async fn a_local_replacement_waits_for_its_holder_only() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let (state, job, lease, _) = transfers::tests::setup(dir.path()).await?;
        deferred(&state, &job, &lease).await?;
        let pending = repository::pending_source_publications(&state.pool)
            .await?
            .pop()
            .ok_or_else(|| anyhow::anyhow!("journal absent"))?;
        assert!(!awaits_its_holder(&state, &pending).await?, "orpheline");
        repository::heartbeat(&state.pool, job.id, &lease).await?;
        assert!(awaits_its_holder(&state, &pending).await?);
        Ok(())
    }
}
