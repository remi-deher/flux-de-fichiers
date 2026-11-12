//! Couverture directe des fonctions du dépôt jobs et flux.

use flux_de_fichiers::{
    domain::{JobErrorCode, JobStatus, LeaseRequest, MAX_ATTEMPTS, UpdateFlowRequest},
    error::AppError,
    repository,
};
use sqlx::sqlite::SqlitePoolOptions;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use uuid::Uuid;

async fn pool() -> anyhow::Result<sqlx::SqlitePool> {
    let pool = SqlitePoolOptions::new().connect("sqlite::memory:").await?;
    sqlx::migrate!().run(&pool).await?;
    Ok(pool)
}

#[tokio::test]
async fn job_lease_publication_and_flow_queries() -> anyhow::Result<()> {
    let pool = pool().await?;
    repository::ping(&pool).await?;

    let missing = Uuid::nil();
    assert!(!repository::worker_exists(&pool, missing).await?);
    assert!(matches!(
        repository::delete_worker(&pool, missing).await,
        Err(AppError::NotFound)
    ));
    assert!(matches!(
        repository::claim(&pool, missing).await,
        Err(AppError::NotFound)
    ));

    let worker = repository::register(&pool, "worker-a").await?;
    let again = repository::register(&pool, "worker-a").await?;
    assert_eq!(again.id, worker.id);
    assert!(repository::worker_exists(&pool, worker.id).await?);
    assert_eq!(repository::list_workers(&pool).await?.len(), 1);

    let job = repository::insert_job(&pool, "film.mkv", None, &"ab".repeat(32), 12).await?;
    assert_eq!(job.status, JobStatus::Queued);
    assert_eq!(job.error_code, None);
    assert_eq!(repository::get_job(&pool, job.id).await?.id, job.id);
    assert_eq!(repository::list_jobs(&pool, None, 10).await?.len(), 1);
    assert!(
        repository::list_jobs(&pool, Some(job.id), 10)
            .await?
            .is_empty()
    );

    let claim = repository::claim(&pool, worker.id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("bail attendu"))?;
    assert_eq!(claim.job_id, job.id);
    let lease = LeaseRequest {
        worker_id: worker.id,
        lease_token: claim.lease_token,
        telemetry: None,
    };
    repository::heartbeat(&pool, job.id, &lease).await?;
    assert_eq!(
        repository::active(&pool, job.id, &lease).await?.status,
        JobStatus::Running
    );

    sqlx::query("UPDATE jobs SET leased_until=datetime('now', '-1 seconds') WHERE id=?1")
        .bind(job.id)
        .execute(&pool)
        .await?;
    let renewed = repository::claim(&pool, worker.id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("reprise du bail"))?;
    let running = repository::get_job(&pool, job.id).await?;
    assert_eq!(running.status, JobStatus::Running);
    assert_eq!(running.attempts, 2);
    assert_eq!(running.error_code, None);

    sqlx::query(
        "UPDATE jobs SET attempts=?1, leased_until=datetime('now', '-1 seconds') WHERE id=?2",
    )
    .bind(MAX_ATTEMPTS)
    .bind(job.id)
    .execute(&pool)
    .await?;
    assert!(repository::claim(&pool, worker.id).await?.is_none());
    let expired = repository::get_job(&pool, job.id).await?;
    assert_eq!(expired.status, JobStatus::Failed);
    assert_eq!(expired.error_code, Some(JobErrorCode::LeaseExpired));

    let second =
        repository::insert_job(&pool, "autre.mkv", Some(worker.id), &"cd".repeat(32), 8).await?;
    let claim = repository::claim(&pool, worker.id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("second bail"))?;
    assert_eq!(claim.job_id, second.id);
    let lease = LeaseRequest {
        worker_id: worker.id,
        lease_token: claim.lease_token,
        telemetry: None,
    };
    let stale = LeaseRequest {
        worker_id: worker.id,
        lease_token: renewed.lease_token,
        telemetry: None,
    };
    assert!(matches!(
        repository::heartbeat(&pool, second.id, &stale).await,
        Err(AppError::Conflict)
    ));
    let linked = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&linked);
    assert!(matches!(
        repository::publish_succeeded(
            &pool,
            second.id,
            &stale,
            repository::PublishedOutput {
                name: "out.mkv",
                sha256: &"ef".repeat(32),
                bytes: 8,
                warning: false,
            },
            async move {
                flag.store(true, Ordering::SeqCst);
                Ok(())
            },
        )
        .await,
        Err(AppError::Conflict)
    ));
    assert!(!linked.load(Ordering::SeqCst));

    repository::publish_succeeded(
        &pool,
        second.id,
        &lease,
        repository::PublishedOutput {
            name: "out.mkv",
            sha256: &"ef".repeat(32),
            bytes: 8,
            warning: true,
        },
        async { Ok(()) },
    )
    .await?;
    let published = repository::get_job(&pool, second.id).await?;
    assert_eq!(published.status, JobStatus::Succeeded);
    assert_eq!(published.output_name.as_deref(), Some("out.mkv"));
    assert!(published.warning);
    assert_eq!(
        repository::succeeded_output_name(&pool, second.id).await?,
        Some("out.mkv".into())
    );
    assert_eq!(
        repository::succeeded_output_name(&pool, job.id).await?,
        None
    );

    let third = repository::insert_job(&pool, "fail.mkv", None, &"11".repeat(32), 4).await?;
    let claim = repository::claim(&pool, worker.id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("bail d'échec"))?;
    let lease = LeaseRequest {
        worker_id: worker.id,
        lease_token: claim.lease_token,
        telemetry: None,
    };
    repository::fail(&pool, third.id, &lease, JobErrorCode::RemuxFailed).await?;
    assert_eq!(
        repository::get_job(&pool, third.id).await?.error_code,
        Some(JobErrorCode::RemuxFailed)
    );
    assert!(matches!(
        repository::fail(&pool, third.id, &lease, JobErrorCode::TransferFailed).await,
        Err(AppError::Conflict)
    ));
    assert!(matches!(
        repository::active(&pool, third.id, &lease).await,
        Err(AppError::Conflict)
    ));

    assert!(repository::default_flow(&pool).await?.is_none());
    let saved = repository::save_flow(
        &pool,
        "flow.test",
        &UpdateFlowRequest {
            name: "Test".into(),
            description: None,
            file_mask: "*.mkv".into(),
            is_default: true,
            definition: "{\"nodes\":[],\"connections\":[]}".into(),
        },
    )
    .await?;
    assert!(saved.is_default);
    assert_eq!(saved.file_mask, "*.mkv");
    let loaded = repository::default_flow(&pool)
        .await?
        .ok_or_else(|| anyhow::anyhow!("flux par défaut"))?;
    assert_eq!(loaded.id, "flow.test");
    assert_eq!(repository::list_flows(&pool).await?.len(), 1);
    assert_eq!(repository::get_flow(&pool, "flow.test").await?.name, "Test");
    assert!(matches!(
        repository::delete_flow(&pool, "flow.test").await,
        Err(AppError::Conflict)
    ));
    repository::save_flow(
        &pool,
        "flow.custom",
        &UpdateFlowRequest {
            name: "Custom".into(),
            description: Some("note".into()),
            file_mask: "*.mkv".into(),
            is_default: false,
            definition: "{}".into(),
        },
    )
    .await?;
    repository::delete_flow(&pool, "flow.custom").await?;
    assert!(matches!(
        repository::get_flow(&pool, "flow.custom").await,
        Err(AppError::NotFound)
    ));
    assert!(matches!(
        repository::delete_flow(&pool, "absent").await,
        Err(AppError::NotFound)
    ));

    let spare = repository::register(&pool, "worker-b").await?;
    repository::delete_worker(&pool, spare.id).await?;
    assert!(!repository::worker_exists(&pool, spare.id).await?);
    Ok(())
}
