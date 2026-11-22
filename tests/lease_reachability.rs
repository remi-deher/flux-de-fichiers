//! État injoignable, échéance et remise en file (ADR 0027).

mod common;

use chrono::{TimeDelta, Utc};
use common::leases::{
    action, claim, expire, leased_job, output, overdue, pool, queued_job, silent_for, sweep,
    with_global_failure_flow,
};
use forgearr::{
    domain::{
        IrreversibleAction, JobErrorCode, JobEventType, JobStatus, LibraryOutputPolicy,
        MAX_ATTEMPTS,
    },
    error::AppError,
    repository::{self, FailureFlowState, MAX_FREE_RELEASES, SWEEP_PAUSE_SECONDS},
};
use forgearr_shared::jobs::{
    DEFAULT_UNREACHABLE_TIMEOUT_SECONDS, LeaseReachability, UNREACHABLE_AFTER_SECONDS,
};
use sqlx::SqlitePool;
use uuid::Uuid;

async fn reachability(pool: &SqlitePool, job: Uuid) -> anyhow::Result<LeaseReachability> {
    let infos = repository::lease_infos(pool, &[job]).await?;
    let info = infos
        .first()
        .ok_or_else(|| anyhow::anyhow!("bail attendu"))?;
    Ok(info.reachability)
}

#[tokio::test]
async fn a_silent_holder_becomes_unreachable_keeps_its_slot_and_comes_back() -> anyhow::Result<()> {
    let pool = pool().await?;
    let (job, lease) = leased_job(&pool, "w").await?;
    assert!(sweep(&pool).await?.is_empty());
    assert_eq!(
        reachability(&pool, job).await?,
        LeaseReachability::Reachable
    );

    silent_for(&pool, job, UNREACHABLE_AFTER_SECONDS + 1).await?;
    let changes = sweep(&pool).await?;
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].event, JobEventType::JobUnreachable);
    assert!(sweep(&pool).await?.is_empty(), "signalée une seule fois");
    assert_eq!(
        reachability(&pool, job).await?,
        LeaseReachability::Unreachable
    );
    // Ni donnée à un autre, ni place libérée.
    queued_job(&pool, "autre.mkv").await?;
    assert!(repository::claim(&pool, lease.worker_id).await?.is_none());
    assert_eq!(repository::active(&pool, job, &lease).await?.id, job);

    assert!(repository::heartbeat(&pool, job, &lease).await?, "rétablie");
    assert!(!repository::heartbeat(&pool, job, &lease).await?);
    assert_eq!(
        reachability(&pool, job).await?,
        LeaseReachability::Reachable
    );
    Ok(())
}

#[tokio::test]
async fn past_the_deadline_the_job_is_queued_again_without_failure_flow() -> anyhow::Result<()> {
    let pool = pool().await?;
    with_global_failure_flow(&pool).await?;
    let (job, lease) = leased_job(&pool, "w").await?;
    let changes = expire(&pool, job).await?;
    assert_eq!(changes[0].event, JobEventType::JobRequeued);
    let queued = repository::get_job(&pool, job).await?;
    assert_eq!(queued.status, JobStatus::Queued);
    assert_eq!(queued.worker_id, None);
    assert_eq!(queued.error_code, None);
    assert_eq!(repository::failure_flow_state(&pool, job).await?, None);
    assert!(matches!(
        repository::heartbeat(&pool, job, &lease).await,
        Err(AppError::LeaseLost)
    ));
    let next = repository::register(&pool, "suivant").await?;
    let (again, fresh) = claim(&pool, next.id).await?;
    assert_eq!(again, job);
    assert!(fresh.lease_epoch > lease.lease_epoch);
    assert_ne!(fresh.lease_token, lease.lease_token);
    Ok(())
}

#[tokio::test]
async fn attempts_run_out_into_a_definitive_failure_that_can_be_retried() -> anyhow::Result<()> {
    let pool = pool().await?;
    with_global_failure_flow(&pool).await?;
    let (job, lease) = leased_job(&pool, "w").await?;
    for attempt in 1..MAX_ATTEMPTS {
        expire(&pool, job).await?;
        let queued = repository::get_job(&pool, job).await?;
        assert_eq!(
            (queued.status, queued.attempts),
            (JobStatus::Queued, attempt)
        );
        claim(&pool, lease.worker_id).await?;
    }
    expire(&pool, job).await?;
    let failed = repository::get_job(&pool, job).await?;
    assert_eq!(failed.status, JobStatus::Failed);
    assert_eq!(failed.error_code, Some(JobErrorCode::LeaseExpired));
    assert_eq!(
        repository::failure_flow_state(&pool, job).await?,
        Some(FailureFlowState::Pending)
    );
    let steps = repository::list_job_steps(&pool, job).await?;
    assert!(
        steps
            .iter()
            .any(|step| step.node_id == repository::LEASE_STEP)
    );
    assert!(failed.is_retryable());
    assert_eq!(repository::retry_job(&pool, job).await?.attempts, 0);
    claim(&pool, lease.worker_id).await?;
    Ok(())
}

#[tokio::test]
async fn an_overdue_lease_still_belongs_to_its_holder_until_it_is_swept() -> anyhow::Result<()> {
    let pool = pool().await?;
    let (job, lease) = leased_job(&pool, "w").await?;
    overdue(&pool, job).await?;
    // Toutes les routes à bail suivent la même règle, pas seulement le battement.
    repository::active(&pool, job, &lease).await?;
    repository::save_job_steps_for_lease(&pool, job, &lease, &[]).await?;
    let policy = LibraryOutputPolicy::ReplaceInPlace;
    repository::begin_source_publication(&pool, job, &lease, output(), policy).await?;
    assert!(repository::authorize_source_publication(&pool, job, &lease).await?);
    repository::publish_succeeded(&pool, job, &lease, output(), async { Ok(()) }).await?;
    assert!(repository::completed_by_lease(&pool, job, &lease).await?);

    let (job, lease) = leased_job(&pool, "autre").await?;
    overdue(&pool, job).await?;
    repository::heartbeat(&pool, job, &lease).await?;
    assert!(sweep(&pool).await?.is_empty());
    Ok(())
}

#[tokio::test]
async fn the_configured_timeout_sets_the_deadline() -> anyhow::Result<()> {
    let pool = pool().await?;
    let (job, lease) = leased_job(&pool, "w").await?;
    let remaining = |expected: i64| {
        let pool = pool.clone();
        async move {
            let left: i64 = sqlx::query_scalar(
                "SELECT unixepoch(leased_until) - unixepoch('now') FROM jobs WHERE id=?1",
            )
            .bind(job)
            .fetch_one(&pool)
            .await?;
            anyhow::ensure!((expected - 2..=expected).contains(&left), "{left}");
            anyhow::Ok(())
        }
    };
    remaining(UNREACHABLE_AFTER_SECONDS + DEFAULT_UNREACHABLE_TIMEOUT_SECONDS).await?;
    repository::set_setting(
        &pool,
        forgearr::runtime_settings::RUNTIME_SETTINGS_KEY,
        r#"{"unreachable_timeout_seconds":120}"#,
    )
    .await?;
    repository::heartbeat(&pool, job, &lease).await?;
    remaining(UNREACHABLE_AFTER_SECONDS + 120).await?;
    Ok(())
}

#[tokio::test]
async fn a_clean_stop_gives_the_attempt_back_within_a_limit() -> anyhow::Result<()> {
    let pool = pool().await?;
    let (job, mut lease) = leased_job(&pool, "w").await?;
    for _ in 0..MAX_FREE_RELEASES {
        repository::release_lease(&pool, job, &lease).await?;
        assert_eq!(repository::get_job(&pool, job).await?.attempts, 0);
        lease = claim(&pool, lease.worker_id).await?.1;
    }
    repository::release_lease(&pool, job, &lease).await?;
    assert_eq!(repository::get_job(&pool, job).await?.attempts, 1);
    assert!(matches!(
        repository::release_lease(&pool, job, &lease).await,
        Err(AppError::LeaseLost)
    ));
    Ok(())
}

#[tokio::test]
async fn a_publication_in_progress_refuses_a_release_and_expires_uncertain() -> anyhow::Result<()> {
    let pool = pool().await?;
    let (job, lease) = leased_job(&pool, "w").await?;
    let policy = LibraryOutputPolicy::ReplaceInPlace;
    repository::begin_source_publication(&pool, job, &lease, output(), policy).await?;
    assert!(matches!(
        repository::release_lease(&pool, job, &lease).await,
        Err(AppError::Conflict)
    ));
    // Le central publie encore : l'échéance attend.
    overdue(&pool, job).await?;
    let in_flight = [(job, lease.lease_epoch)].into_iter().collect();
    let pass = repository::sweep(&pool, None, &in_flight).await?;
    assert!(pass.changes.is_empty());
    expire(&pool, job).await?;
    let failed = repository::get_job(&pool, job).await?;
    assert_eq!(failed.error_code, Some(JobErrorCode::LeaseExpired));
    let state: String =
        sqlx::query_scalar("SELECT state FROM source_publication_journal WHERE job_id=?1")
            .bind(job)
            .fetch_one(&pool)
            .await?;
    assert_eq!(state, "uncertain");
    Ok(())
}

#[tokio::test]
async fn a_central_restart_resets_counters_and_the_next_beat_restores() -> anyhow::Result<()> {
    let pool = pool().await?;
    let (job, lease) = leased_job(&pool, "w").await?;
    overdue(&pool, job).await?;
    assert_eq!(repository::restart_contacts(&pool).await?, 1);
    assert!(sweep(&pool).await?.is_empty());
    assert_eq!(
        reachability(&pool, job).await?,
        LeaseReachability::Unreachable
    );
    assert!(repository::heartbeat(&pool, job, &lease).await?);
    Ok(())
}

#[tokio::test]
async fn jobs_of_the_integrated_worker_end_with_the_central() -> anyhow::Result<()> {
    let pool = pool().await?;
    let (local, lease) = leased_job(&pool, "local").await?;
    let (remote, _) = leased_job(&pool, "distant").await?;
    repository::set_serves_central(&pool, lease.worker_id, true).await?;
    let changes = repository::end_integrated_leases(&pool).await?;
    assert_eq!(changes.len(), 1);
    assert_eq!(
        repository::get_job(&pool, local).await?.status,
        JobStatus::Queued
    );
    assert_eq!(
        repository::get_job(&pool, remote).await?.status,
        JobStatus::Running
    );
    Ok(())
}

#[tokio::test]
async fn a_suspended_central_shifts_deadlines_instead_of_expiring() -> anyhow::Result<()> {
    let pool = pool().await?;
    let (job, _) = leased_job(&pool, "w").await?;
    let before = Utc::now();
    let after = before + TimeDelta::hours(2);
    let none = Default::default();
    let pass = repository::sweep_with_clock(&pool, Some(before), &none, || after).await?;
    assert!(pass.changes.is_empty(), "aucune expiration au réveil");
    // Le passage suivant reprend le décompte là où il en était.
    let later = after + TimeDelta::seconds(5);
    let pass = repository::sweep_with_clock(&pool, Some(after), &none, || later).await?;
    assert!(
        pass.changes
            .iter()
            .all(|change| change.event == JobEventType::JobUnreachable)
    );
    assert_eq!(
        repository::get_job(&pool, job).await?.status,
        JobStatus::Running
    );
    Ok(())
}

#[tokio::test]
async fn a_clock_moved_back_does_not_postpone_the_deadline() -> anyhow::Result<()> {
    let pool = pool().await?;
    let (job, _) = leased_job(&pool, "w").await?;
    let before = Utc::now();
    let back = before - TimeDelta::hours(2);
    let none = Default::default();
    repository::sweep_with_clock(&pool, Some(before), &none, || back).await?;
    let window = UNREACHABLE_AFTER_SECONDS + DEFAULT_UNREACHABLE_TIMEOUT_SECONDS;
    let due = back + TimeDelta::seconds(window + 2);
    repository::sweep_with_clock(&pool, None, &none, || due).await?;
    assert_eq!(
        repository::get_job(&pool, job).await?.status,
        JobStatus::Queued
    );
    Ok(())
}

#[tokio::test]
async fn slow_passes_still_reach_the_deadline() -> anyhow::Result<()> {
    let pool = pool().await?;
    let (job, _) = leased_job(&pool, "w").await?;
    let none = Default::default();
    let mut previous = Utc::now();
    let window = UNREACHABLE_AFTER_SECONDS + DEFAULT_UNREACHABLE_TIMEOUT_SECONDS;
    for _ in 0..=(window / SWEEP_PAUSE_SECONDS + 1) {
        let now = previous + TimeDelta::seconds(SWEEP_PAUSE_SECONDS);
        repository::sweep_with_clock(&pool, Some(previous), &none, || now).await?;
        previous = now;
    }
    assert_eq!(
        repository::get_job(&pool, job).await?.status,
        JobStatus::Queued
    );
    Ok(())
}

#[tokio::test]
async fn a_poisoned_job_does_not_hold_the_others() -> anyhow::Result<()> {
    let pool = pool().await?;
    let (bad, bad_lease) = leased_job(&pool, "empoisonne").await?;
    let (good, _) = leased_job(&pool, "sain").await?;
    let call = action(&bad_lease, IrreversibleAction::HttpCall, "n1");
    repository::authorize_action(&pool, bad, &call).await?;
    sqlx::query("UPDATE job_irreversible_actions SET error_code='illisible'")
        .execute(&pool)
        .await?;
    overdue(&pool, bad).await?;
    overdue(&pool, good).await?;
    sweep(&pool).await?;
    assert_eq!(
        repository::get_job(&pool, bad).await?.status,
        JobStatus::Running
    );
    assert_eq!(
        repository::get_job(&pool, good).await?.status,
        JobStatus::Queued
    );
    Ok(())
}

#[tokio::test]
async fn jobs_disowned_by_their_worker_are_taken_back_after_silence() -> anyhow::Result<()> {
    let pool = pool().await?;
    let (job, lease) = leased_job(&pool, "w").await?;
    let worker = lease.worker_id;
    assert!(
        repository::end_disowned_leases(&pool, worker, &[])
            .await?
            .is_empty()
    );
    silent_for(&pool, job, UNREACHABLE_AFTER_SECONDS + 1).await?;
    assert!(
        repository::end_disowned_leases(&pool, worker, &[job])
            .await?
            .is_empty()
    );
    let changes = repository::end_disowned_leases(&pool, worker, &[]).await?;
    assert_eq!(changes[0].event, JobEventType::JobRequeued);
    Ok(())
}

#[tokio::test]
async fn a_claim_that_cannot_be_built_is_given_back() -> anyhow::Result<()> {
    let pool = pool().await?;
    let worker = repository::register(&pool, "w").await?;
    let job = queued_job(&pool, "film.mkv").await?;
    sqlx::query("UPDATE jobs SET source_sha256=NULL WHERE id=?1")
        .bind(job)
        .execute(&pool)
        .await?;
    let protocol = forgearr_shared::jobs::LeaseProtocol::LATEST;
    let refused = repository::claim_with_protocol(&pool, worker.id, protocol).await;
    assert!(refused.is_err());
    let queued = repository::get_job(&pool, job).await?;
    assert_eq!((queued.status, queued.attempts), (JobStatus::Queued, 0));
    Ok(())
}

#[tokio::test]
async fn a_publication_that_begins_renews_an_overdue_lease() -> anyhow::Result<()> {
    let pool = pool().await?;
    let (job, lease) = leased_job(&pool, "w").await?;
    overdue(&pool, job).await?;
    let policy = LibraryOutputPolicy::ReplaceInPlace;
    repository::begin_source_publication(&pool, job, &lease, output(), policy).await?;
    // Le balayage a relevé les publications en cours avant celle-ci.
    assert!(sweep(&pool).await?.is_empty());
    assert!(repository::authorize_source_publication(&pool, job, &lease).await?);
    Ok(())
}
