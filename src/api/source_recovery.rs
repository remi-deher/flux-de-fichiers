//! Reprise prudente d'une publication interrompue lorsque le central voit la source.

use super::AppState;
use crate::{
    domain::{Job, JobStatus, LeaseRequest},
    error::{AppError, AppResult},
    repository, storage,
};
use std::path::Path;

pub(super) async fn fingerprint_matches(path: &Path, hash: &str, bytes: i64, limit: u64) -> bool {
    matches!(storage::fingerprint(path, limit).await, Ok((actual_hash, actual_bytes)) if actual_hash == hash && i64::try_from(actual_bytes).ok() == Some(bytes))
}

/// Conclut seulement si les octets en place et la sauvegarde éventuelle correspondent au journal.
async fn replacement_matches(
    s: &AppState,
    job: &Job,
    pending: &repository::PendingSourcePublication,
) -> AppResult<bool> {
    if !storage::valid_name(&pending.output_name)
        || !crate::library_scan::valid_relative_name(&job.source_name)
    {
        return Ok(false);
    }
    let Some(extension) = Path::new(&pending.output_name)
        .extension()
        .and_then(|value| value.to_str())
        .filter(|value| storage::valid_output_extension(value))
    else {
        return Ok(false);
    };
    let Some(root) =
        crate::library_scan::central_writable_job_root(&s.pool, &s.config.input_dir, job).await?
    else {
        return Ok(false);
    };
    let output =
        storage::input_path(&s.config.data_dir.join("output"), &pending.output_name).await?;
    if !fingerprint_matches(
        &output,
        &pending.output_sha256,
        pending.output_bytes,
        s.config.max_file_bytes,
    )
    .await
    {
        return Ok(false);
    }
    let (Some(source_sha256), Some(source_bytes)) =
        (job.source_sha256.as_deref(), job.source_bytes)
    else {
        return Ok(false);
    };
    crate::source_publication::published_source_matches(
        &crate::source_publication::PublishedSourceExpectation {
            root: &root,
            source_name: &job.source_name,
            source_sha256,
            source_bytes,
            result_sha256: &pending.output_sha256,
            result_bytes: pending.output_bytes,
            extension,
            policy: pending.output_policy,
        },
        s.config.max_file_bytes,
    )
    .await
}

async fn finalize_if_proven(
    s: &AppState,
    pending: &repository::PendingSourcePublication,
) -> AppResult<bool> {
    let job = repository::get_job(&s.pool, pending.job_id).await?;
    let (Some(worker_id), Some(lease_token)) = (job.worker_id, job.lease_token) else {
        return Ok(false);
    };
    if job.status != JobStatus::Running || job.lease_epoch != pending.lease_epoch {
        return Ok(false);
    }
    let lease = LeaseRequest {
        worker_id,
        lease_token,
        lease_epoch: pending.lease_epoch,
        telemetry: None,
    };
    if repository::active(&s.pool, job.id, &lease).await.is_err()
        || !replacement_matches(s, &job, pending).await?
    {
        return Ok(false);
    }
    finalize(s, &job, &lease, pending).await?;
    Ok(true)
}

pub(super) async fn finalize(
    s: &AppState,
    job: &Job,
    lease: &LeaseRequest,
    pending: &repository::PendingSourcePublication,
) -> AppResult<()> {
    repository::publish_succeeded(
        &s.pool,
        job.id,
        lease,
        repository::PublishedOutput {
            name: &pending.output_name,
            sha256: &pending.output_sha256,
            bytes: pending.output_bytes,
            warning: pending.output_warning,
        },
        async { Ok(()) },
    )
    .await?;
    let destination = s.config.data_dir.join("output").join(&pending.output_name);
    if let Err(error) =
        crate::flows::publication::notify_after_publication(s, job, lease.worker_id, &destination)
            .await
    {
        tracing::warn!(job_id=%job.id, error=%error, "notification après reprise de publication indisponible");
    }
    s.notify_job_event(
        crate::domain::JobEventType::JobSucceeded,
        job.id,
        JobStatus::Succeeded,
        None,
    );
    Ok(())
}

/// Les publications non démontrables sont conservées dans l'état incertain existant.
pub(super) async fn recover(s: &AppState) -> AppResult<()> {
    for pending in repository::pending_source_publications(&s.pool).await? {
        let job = repository::get_job(&s.pool, pending.job_id).await?;
        if crate::library_scan::central_writable_job_root(&s.pool, &s.config.input_dir, &job)
            .await?
            .is_none()
        {
            continue;
        }
        match finalize_if_proven(s, &pending).await {
            Ok(true) => {
                tracing::info!(job_id=%pending.job_id, "publication locale confirmée après redémarrage")
            }
            Ok(false)
            | Err(
                AppError::NotFound | AppError::Invalid | AppError::Conflict | AppError::Storage(_),
            ) => {
                let lease = job
                    .worker_id
                    .zip(job.lease_token)
                    .map(|(worker_id, lease_token)| LeaseRequest {
                        worker_id,
                        lease_token,
                        lease_epoch: pending.lease_epoch,
                        telemetry: None,
                    });
                let active = match lease {
                    Some(lease) => repository::active(&s.pool, job.id, &lease).await.is_ok(),
                    None => false,
                };
                if !active {
                    repository::mark_source_publication_uncertain(
                        &s.pool,
                        pending.job_id,
                        pending.lease_epoch,
                    )
                    .await?;
                }
            }
            Err(error) => return Err(error),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{JobErrorCode, LibraryOutputPolicy};

    async fn interrupted(
        s: &AppState,
        job: &Job,
        lease: &LeaseRequest,
        output_name: &str,
        output: &[u8],
    ) -> anyhow::Result<()> {
        sqlx::query(
            "UPDATE jobs SET source_worker_id=NULL,source_key=NULL,source_writable=1 WHERE id=?1",
        )
        .bind(job.id)
        .execute(&s.pool)
        .await?;
        let result = s.config.data_dir.join("output").join(output_name);
        tokio::fs::write(&result, output).await?;
        let (hash, bytes) = storage::fingerprint(&result, s.config.max_file_bytes).await?;
        repository::begin_source_publication(
            &s.pool,
            job.id,
            lease,
            repository::PublishedOutput {
                name: output_name,
                sha256: &hash,
                bytes: bytes as i64,
                warning: true,
            },
            LibraryOutputPolicy::ReplaceWithBackup,
        )
        .await?;
        Ok(())
    }

    #[tokio::test]
    async fn resumes_only_a_proven_local_replacement_and_preserves_warning() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let (state, job, lease, _) = super::super::transfers::tests::setup(dir.path()).await?;
        interrupted(&state, &job, &lease, "result.mp4", b"result").await?;
        tokio::fs::rename(dir.path().join("film.mkv"), dir.path().join("film.mkv.bak")).await?;
        tokio::fs::write(dir.path().join("film.mp4"), b"result").await?;
        recover(&state).await?;
        let succeeded = repository::get_job(&state.pool, job.id).await?;
        assert_eq!(succeeded.status, JobStatus::Succeeded);
        assert!(succeeded.warning);
        assert_eq!(succeeded.output_name.as_deref(), Some("result.mp4"));
        assert!(
            repository::pending_source_publications(&state.pool)
                .await?
                .is_empty()
        );
        Ok(())
    }

    #[tokio::test]
    async fn missing_backup_or_expired_lease_never_infers_success() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let (state, job, lease, _) = super::super::transfers::tests::setup(dir.path()).await?;
        interrupted(&state, &job, &lease, "result.mkv", b"result").await?;
        tokio::fs::write(dir.path().join("film.mkv"), b"result").await?;
        recover(&state).await?;
        let pending = repository::pending_source_publications(&state.pool)
            .await?
            .pop()
            .ok_or_else(|| anyhow::anyhow!("journal absent"))?;
        super::super::source_recovery_remote::recover_one(&state, pending).await?;
        assert_eq!(
            repository::get_job(&state.pool, job.id).await?.error_code,
            Some(JobErrorCode::SourcePublicationUncertain)
        );

        let dir = tempfile::tempdir()?;
        let (state, job, lease, _) = super::super::transfers::tests::setup(dir.path()).await?;
        interrupted(&state, &job, &lease, "result.mkv", b"result").await?;
        tokio::fs::rename(dir.path().join("film.mkv"), dir.path().join("film.mkv.bak")).await?;
        tokio::fs::write(dir.path().join("film.mkv"), b"result").await?;
        sqlx::query("UPDATE jobs SET leased_until=datetime('now','-1 second') WHERE id=?1")
            .bind(job.id)
            .execute(&state.pool)
            .await?;
        recover(&state).await?;
        assert_eq!(
            repository::get_job(&state.pool, job.id).await?.error_code,
            Some(JobErrorCode::LeaseExpired)
        );
        Ok(())
    }
}
