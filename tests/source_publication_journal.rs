//! Les résultats préparés et les interruptions de publication restent traçables.

use forgearr::{
    domain::{JobErrorCode, JobStatus, LeaseRequest},
    error::AppError,
    repository,
};
use sqlx::{SqlitePool, sqlite::SqlitePoolOptions};
use uuid::Uuid;

async fn fixture() -> anyhow::Result<(SqlitePool, Uuid, LeaseRequest)> {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await?;
    sqlx::migrate!().run(&pool).await?;
    let worker = repository::register(&pool, "publication-worker").await?;
    let job = repository::insert_job(&pool, "film.mkv", None, &"aa".repeat(32), 10).await?;
    let claim = repository::claim(&pool, worker.id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("bail absent"))?;
    let lease = LeaseRequest {
        worker_id: worker.id,
        lease_token: claim.lease_token,
        lease_epoch: claim.lease_epoch,
        telemetry: None,
    };
    Ok((pool, job.id, lease))
}

#[tokio::test]
async fn restart_marks_an_interrupted_publication_uncertain_and_keeps_its_result_reference()
-> anyhow::Result<()> {
    let (pool, job, lease) = fixture().await?;
    repository::begin_source_publication(
        &pool,
        job,
        &lease,
        repository::PublishedOutput {
            name: "result.mkv",
            sha256: &"bb".repeat(32),
            bytes: 9,
            warning: false,
        },
        forgearr::domain::LibraryOutputPolicy::ReplaceInPlace,
    )
    .await?;
    let pending = repository::pending_source_publications(&pool).await?;
    assert_eq!(pending.len(), 1);
    assert_eq!(
        pending[0].output_policy,
        forgearr::domain::LibraryOutputPolicy::ReplaceInPlace
    );
    assert!(!pending[0].output_warning);
    assert!(matches!(
        repository::cancel_job(&pool, job).await,
        Err(AppError::Conflict)
    ));
    assert!(matches!(
        repository::fail(&pool, job, &lease, JobErrorCode::TransferFailed).await,
        Err(AppError::Conflict)
    ));
    assert_eq!(repository::recover_source_publications(&pool).await?, 1);
    let failed = repository::get_job(&pool, job).await?;
    assert_eq!(failed.status, JobStatus::Failed);
    assert_eq!(
        failed.error_code,
        Some(JobErrorCode::SourcePublicationUncertain)
    );
    assert_eq!(
        repository::downloadable_output_name(&pool, job)
            .await?
            .as_deref(),
        Some("result.mkv")
    );
    assert!(matches!(
        repository::retry_job(&pool, job).await,
        Err(AppError::Conflict)
    ));
    let row: (String, String) =
        sqlx::query_as("SELECT output_name,state FROM source_publication_journal WHERE job_id=?1")
            .bind(job)
            .fetch_one(&pool)
            .await?;
    assert_eq!(row, ("result.mkv".into(), "uncertain".into()));
    assert_eq!(repository::recover_source_publications(&pool).await?, 0);
    Ok(())
}

#[tokio::test]
async fn recovery_preserves_lease_expiry_and_success_removes_the_journal() -> anyhow::Result<()> {
    let (pool, job, lease) = fixture().await?;
    repository::begin_source_publication(
        &pool,
        job,
        &lease,
        repository::PublishedOutput {
            name: "result.mkv",
            sha256: &"bb".repeat(32),
            bytes: 9,
            warning: false,
        },
        forgearr::domain::LibraryOutputPolicy::ReplaceInPlace,
    )
    .await?;
    sqlx::query("UPDATE jobs SET leased_until=datetime('now','-1 second') WHERE id=?1")
        .bind(job)
        .execute(&pool)
        .await?;
    repository::sweep(&pool, None, &Default::default()).await?;
    assert_eq!(
        repository::get_job(&pool, job).await?.error_code,
        Some(JobErrorCode::LeaseExpired)
    );
    assert_eq!(
        repository::downloadable_output_name(&pool, job)
            .await?
            .as_deref(),
        Some("result.mkv")
    );

    let (pool, job, lease) = fixture().await?;
    repository::begin_source_publication(
        &pool,
        job,
        &lease,
        repository::PublishedOutput {
            name: "result.mkv",
            sha256: &"bb".repeat(32),
            bytes: 9,
            warning: false,
        },
        forgearr::domain::LibraryOutputPolicy::ReplaceInPlace,
    )
    .await?;
    repository::publish_succeeded(
        &pool,
        job,
        &lease,
        repository::PublishedOutput {
            name: "result.mkv",
            sha256: &"bb".repeat(32),
            bytes: 9,
            warning: false,
        },
        async { Ok(()) },
    )
    .await?;
    let remaining: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM source_publication_journal WHERE job_id=?1")
            .bind(job)
            .fetch_one(&pool)
            .await?;
    assert_eq!(remaining, 0);
    Ok(())
}

#[tokio::test]
async fn an_overdue_but_unswept_lease_is_uncertain_and_not_lost() -> anyhow::Result<()> {
    let (pool, job, lease) = fixture().await?;
    let output = repository::PublishedOutput {
        name: "",
        sha256: &"bb".repeat(32),
        bytes: 9,
        warning: false,
    };
    let policy = forgearr::domain::LibraryOutputPolicy::ReplaceInPlace;
    repository::begin_pulled_publication(&pool, job, &lease, output, "mkv", policy).await?;
    sqlx::query("UPDATE jobs SET leased_until=datetime('now','-1 second') WHERE id=?1")
        .bind(job)
        .execute(&pool)
        .await?;
    let epoch = lease.lease_epoch;
    let code = repository::mark_source_publication_uncertain(&pool, job, epoch).await?;
    assert_eq!(code, Some(JobErrorCode::SourcePublicationUncertain));
    assert!(!repository::get_job(&pool, job).await?.is_retryable());
    Ok(())
}
