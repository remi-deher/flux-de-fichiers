//! Accords avant une action irréversible et flux d'échec à exécutant unique (ADR 0027).

mod common;

use common::leases::{action, claim, expire, leased_job, output, pool, with_global_failure_flow};
use forgearr::{
    domain::{
        AuthorizeAction, IrreversibleAction, JobErrorCode, JobStatus, LeaseRequest,
        LibraryOutputPolicy,
    },
    error::AppError,
    repository::{self, FailureFlowState},
};
use forgearr_shared::jobs::ACTION_NODE_ID_MAX_CHARS;

const POLICY: LibraryOutputPolicy = LibraryOutputPolicy::ReplaceInPlace;

fn failure_flow(lease: &LeaseRequest) -> AuthorizeAction {
    let mut request = action(lease, IrreversibleAction::FailureFlow, "");
    request.error_code = Some(JobErrorCode::OperationFailed);
    request
}

#[tokio::test]
async fn an_http_call_is_recorded_once_and_forbids_the_requeue() -> anyhow::Result<()> {
    let pool = pool().await?;
    let (job, lease) = leased_job(&pool, "w").await?;
    let call = action(&lease, IrreversibleAction::HttpCall, "webhook");
    repository::authorize_action(&pool, job, &call).await?;
    repository::authorize_action(&pool, job, &call).await?;
    let recorded: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM job_irreversible_actions")
        .fetch_one(&pool)
        .await?;
    assert_eq!(recorded, 1);
    expire(&pool, job).await?;
    let failed = repository::get_job(&pool, job).await?;
    assert_eq!(failed.status, JobStatus::Failed);
    assert_eq!(failed.error_code, Some(JobErrorCode::LeaseExpired));
    let steps = repository::list_job_steps(&pool, job).await?;
    let reason = steps.last().and_then(|step| step.message.as_deref());
    assert!(reason.is_some_and(|message| message.contains("webhook")));
    // La relance manuelle assume la répétition : la perte suivante est de nouveau reprise.
    repository::retry_job(&pool, job).await?;
    claim(&pool, lease.worker_id).await?;
    expire(&pool, job).await?;
    assert_eq!(
        repository::get_job(&pool, job).await?.status,
        JobStatus::Queued
    );
    Ok(())
}

#[tokio::test]
async fn an_in_place_operation_does_not_forbid_the_requeue() -> anyhow::Result<()> {
    let pool = pool().await?;
    let (job, lease) = leased_job(&pool, "w").await?;
    let request = action(&lease, IrreversibleAction::InPlace, "props");
    repository::authorize_action(&pool, job, &request).await?;
    expire(&pool, job).await?;
    assert_eq!(
        repository::get_job(&pool, job).await?.status,
        JobStatus::Queued
    );
    Ok(())
}

#[tokio::test]
async fn an_unrecordable_action_is_never_granted() -> anyhow::Result<()> {
    let pool = pool().await?;
    let (job, lease) = leased_job(&pool, "w").await?;
    for node in [String::new(), "x".repeat(ACTION_NODE_ID_MAX_CHARS + 1)] {
        let request = action(&lease, IrreversibleAction::HttpCall, &node);
        let refused = repository::authorize_action(&pool, job, &request).await;
        assert!(matches!(refused, Err(AppError::Invalid)));
    }
    let without_code = action(&lease, IrreversibleAction::FailureFlow, "");
    let refused = repository::authorize_action(&pool, job, &without_code).await;
    assert!(matches!(refused, Err(AppError::Invalid)));
    Ok(())
}

#[tokio::test]
async fn the_failure_flow_has_one_executor_even_if_its_worker_is_lost() -> anyhow::Result<()> {
    let pool = pool().await?;
    with_global_failure_flow(&pool).await?;
    let (job, lease) = leased_job(&pool, "w").await?;
    repository::authorize_action(&pool, job, &failure_flow(&lease)).await?;
    repository::authorize_action(&pool, job, &failure_flow(&lease)).await?;
    expire(&pool, job).await?;
    let failed = repository::get_job(&pool, job).await?;
    assert_eq!(failed.status, JobStatus::Failed);
    assert_eq!(failed.error_code, Some(JobErrorCode::OperationFailed));
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
async fn a_lost_lease_before_the_grant_leaves_the_failure_flow_to_the_central() -> anyhow::Result<()>
{
    let pool = pool().await?;
    with_global_failure_flow(&pool).await?;
    let (job, lease) = leased_job(&pool, "w").await?;
    repository::fail(&pool, job, &lease, JobErrorCode::TransferFailed).await?;
    let refused = repository::authorize_action(&pool, job, &failure_flow(&lease)).await;
    assert!(matches!(refused, Err(AppError::LeaseLost)));
    assert!(
        repository::take_pending_failure_flow(&pool)
            .await?
            .is_some()
    );
    Ok(())
}

#[tokio::test]
async fn failure_flow_and_publication_exclude_each_other_in_both_orders() -> anyhow::Result<()> {
    let pool = pool().await?;
    let (job, lease) = leased_job(&pool, "publie").await?;
    repository::begin_source_publication(&pool, job, &lease, output(), POLICY).await?;
    let refused = repository::authorize_action(&pool, job, &failure_flow(&lease)).await;
    assert!(matches!(refused, Err(AppError::Conflict)));

    let (job, lease) = leased_job(&pool, "echoue").await?;
    repository::authorize_action(&pool, job, &failure_flow(&lease)).await?;
    let begun = repository::begin_source_publication(&pool, job, &lease, output(), POLICY).await;
    assert!(matches!(begun, Err(AppError::Conflict)));
    let published =
        repository::publish_succeeded(&pool, job, &lease, output(), async { Ok(()) }).await;
    assert!(matches!(published, Err(AppError::Conflict)));
    let call = action(&lease, IrreversibleAction::HttpCall, "n1");
    let called = repository::authorize_action(&pool, job, &call).await;
    assert!(matches!(called, Err(AppError::Conflict)));
    repository::fail(&pool, job, &lease, JobErrorCode::OperationFailed).await?;
    Ok(())
}

#[tokio::test]
async fn an_unresolved_publication_blocks_a_replay_of_the_same_source() -> anyhow::Result<()> {
    let pool = pool().await?;
    let library = repository::insert_library(
        &pool,
        forgearr::domain::LibraryWrite {
            name: "Films".into(),
            ..Default::default()
        },
    )
    .await?;
    let worker = repository::register(&pool, "w").await?;
    let job = repository::insert_job_with_options(
        &pool,
        "film.mkv",
        None,
        &"ab".repeat(32),
        1,
        Some(library.id),
        None,
    )
    .await?;
    let lease = LeaseRequest {
        worker_id: worker.id,
        lease_token: uuid::Uuid::new_v4(),
        lease_epoch: 1,
        telemetry: None,
    };
    sqlx::query(
        "UPDATE jobs SET status='running', worker_id=?2, lease_token=?3, lease_epoch=1, \
         leased_until=datetime('now', '+1 hour') WHERE id=?1",
    )
    .bind(job.id)
    .bind(lease.worker_id)
    .bind(lease.lease_token)
    .execute(&pool)
    .await?;
    repository::begin_source_publication(&pool, job.id, &lease, output(), POLICY).await?;
    expire(&pool, job.id).await?;
    let replayed = repository::library_history::reprocess(&pool, library.id, job.id, None).await;
    assert!(matches!(replayed, Err(AppError::Conflict)));
    // L'administrateur lève l'incertitude : la relance redevient possible.
    repository::clear_job_output_name(&pool, job.id).await?;
    repository::library_history::reprocess(&pool, library.id, job.id, None).await?;
    Ok(())
}
