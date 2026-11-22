//! Fin de bail sans demande du titulaire, et restitutions : reprise ou échec, une transaction par tâche.

use super::{
    LeaseKey,
    contact::ensure_held,
    outcome::{LeaseChange, LeaseEnd, LeaseOutcome},
    stamp,
};
use crate::{
    domain::{JobErrorCode, JobStatus, JobStepEntry, JobStepStatus, LeaseRequest, MAX_ATTEMPTS},
    error::{AppError, AppResult},
    repository::{irreversible_actions, jobs, relays, source_publication_journal as journal},
};
use chrono::{DateTime, Utc};
use forgearr_shared::{jobs::LeaseProtocol, workers::WORKER_ONLINE_SECONDS};
use sqlx::{SqliteConnection, SqlitePool};
use uuid::Uuid;

/// Restitutions (arrêt propre, bail non remis) qui rendent leur tentative, par tâche.
pub const MAX_FREE_RELEASES: i64 = 3;
/// Nœud et type de l'étape qui explique une fin de bail sans reprise.
pub const LEASE_STEP: &str = "lease";
/// Rang de l'étape ajoutée (numérotée après le journal existant).
const APPENDED_STEP_INDEX: i32 = 1;

/// Le titulaire rend sa tâche (arrêt propre). Refusé (`Conflict`) pendant une publication.
pub async fn release_lease(
    pool: &SqlitePool,
    job: Uuid,
    lease: &LeaseRequest,
) -> AppResult<LeaseChange> {
    end_held_lease(pool, job, lease, LeaseEnd::Released).await
}

/// Le bail réservé n'a pas pu être remis au worker.
pub async fn undeliver_lease(
    pool: &SqlitePool,
    job: Uuid,
    lease: &LeaseRequest,
) -> AppResult<LeaseChange> {
    end_held_lease(pool, job, lease, LeaseEnd::NotDelivered).await
}

async fn end_held_lease(
    pool: &SqlitePool,
    job: Uuid,
    lease: &LeaseRequest,
    reason: LeaseEnd,
) -> AppResult<LeaseChange> {
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    ensure_held(&mut tx, job, lease).await?;
    let outcome = end_lease(&mut tx, (job, lease.lease_epoch), reason, None)
        .await?
        .ok_or(AppError::LeaseLost)?;
    tx.commit().await?;
    Ok(LeaseChange::from_outcome(job, outcome))
}

pub(in crate::repository) async fn end_all(
    pool: &SqlitePool,
    leases: Vec<LeaseKey>,
    reason: LeaseEnd,
) -> Vec<LeaseChange> {
    let mut changes = Vec::with_capacity(leases.len());
    for key in leases {
        changes.extend(end_or_log(pool, key, reason, None).await);
    }
    changes
}

/// Une tâche illisible n'empêche pas de conclure les autres.
pub(in crate::repository) async fn end_or_log(
    pool: &SqlitePool,
    key: LeaseKey,
    reason: LeaseEnd,
    due: Option<DateTime<Utc>>,
) -> Option<LeaseChange> {
    match end_one(pool, key, reason, due).await {
        Ok(change) => change,
        Err(error) => {
            tracing::error!(job_id=%key.0, ?reason, %error, "fin de bail impossible");
            None
        }
    }
}

pub(in crate::repository) async fn end_one(
    pool: &SqlitePool,
    key: LeaseKey,
    reason: LeaseEnd,
    due: Option<DateTime<Utc>>,
) -> AppResult<Option<LeaseChange>> {
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    let outcome = end_lease(&mut tx, key, reason, due).await?;
    tx.commit().await?;
    Ok(outcome.map(|outcome| LeaseChange::from_outcome(key.0, outcome)))
}

/// Ce qui décide de la suite d'un bail terminé.
#[derive(sqlx::FromRow)]
struct EndingLease {
    lease_protocol: i64,
    ungated_lease: bool,
    attempts: i32,
    free_releases: i64,
    failure_flow_taken: bool,
    publishing: bool,
    relay_target_offline: bool,
}

async fn read_ending_lease(
    conn: &mut SqliteConnection,
    (job, epoch): LeaseKey,
    due: Option<DateTime<Utc>>,
) -> AppResult<Option<EndingLease>> {
    Ok(sqlx::query_as(
        "SELECT lease_protocol, ungated_lease, attempts, free_releases, \
           failure_flow_state IS NOT NULL AS failure_flow_taken, \
           EXISTS(SELECT 1 FROM source_publication_journal p WHERE p.job_id=jobs.id \
                  AND p.lease_epoch=jobs.lease_epoch AND p.state=?5) AS publishing, \
           EXISTS(SELECT 1 FROM job_relays r JOIN workers w ON w.id=jobs.target_worker_id \
                  WHERE r.job_id=jobs.id AND r.artifact_relay=0 AND r.requirement_json IS NULL \
                  AND w.last_seen <= datetime('now', '-' || ?6 || ' seconds')) AS relay_target_offline \
         FROM jobs WHERE id=?1 AND lease_epoch=?2 AND status=?3 AND (?4 IS NULL OR leased_until <= ?4)",
    )
    .bind(job)
    .bind(epoch)
    .bind(JobStatus::Running)
    .bind(due.map(stamp))
    .bind(journal::PublicationState::InProgress.as_str())
    .bind(WORKER_ONLINE_SECONDS)
    .fetch_optional(conn)
    .await?)
}

/// Seule fonction qui retire un bail à son titulaire sans sa demande. `None` : ce bail n'est plus
/// en cours (ou, avec `due`, son échéance a été repoussée entre-temps).
async fn end_lease(
    conn: &mut SqliteConnection,
    key: LeaseKey,
    reason: LeaseEnd,
    due: Option<DateTime<Utc>>,
) -> AppResult<Option<LeaseOutcome>> {
    let Some(lease) = read_ending_lease(conn, key, due).await? else {
        return Ok(None);
    };
    let job = key.0;
    if lease.publishing {
        return match reason {
            LeaseEnd::HolderStopped => defer(conn, job).await.map(Some),
            LeaseEnd::Released | LeaseEnd::NotDelivered => Err(AppError::Conflict),
            LeaseEnd::Expired => {
                let outcome = fail_job(conn, job, JobErrorCode::LeaseExpired, None).await?;
                journal::reconcile_expired_publications(conn).await?;
                Ok(Some(outcome))
            }
        };
    }
    let refunded = reason.refunds_attempt() && lease.free_releases < MAX_FREE_RELEASES;
    // Un bail jamais remis n'a rien exécuté : rien à répéter, rien à oublier.
    let undelivered = reason == LeaseEnd::NotDelivered;
    if !undelivered && let Some((code, message)) = exclusion(conn, job, &lease).await? {
        let failed = fail_job(conn, job, code, message.as_deref()).await?;
        return Ok(Some(failed));
    }
    if lease.attempts - i32::from(refunded) >= MAX_ATTEMPTS {
        let message = format!("bail perdu : {MAX_ATTEMPTS} tentatives épuisées, relance manuelle");
        let failed = fail_job(conn, job, JobErrorCode::LeaseExpired, Some(&message)).await?;
        return Ok(Some(failed));
    }
    let requeued = requeue(conn, job, refunded).await?;
    if !undelivered {
        forget_lost_attempt(conn, job).await?;
    }
    Ok(Some(requeued))
}

/// Ce qui exclut la reprise automatique d'une tâche dont le bail a pu être exécuté : code de l'échec
/// et explication éventuelle.
async fn exclusion(
    conn: &mut SqliteConnection,
    job: Uuid,
    lease: &EndingLease,
) -> AppResult<Option<(JobErrorCode, Option<String>)>> {
    if let Some(blocking) = irreversible_actions::blocking_action(conn, job).await? {
        let code = blocking.error_code.unwrap_or(JobErrorCode::LeaseExpired);
        return Ok(Some((code, Some(blocking.refusal_message()))));
    }
    let message = if lease.failure_flow_taken {
        Some("reprise automatique exclue : le flux d'échec a été autorisé")
    } else if lease.lease_protocol == LeaseProtocol::Legacy.number() {
        None
    } else if lease.ungated_lease {
        Some("reprise automatique exclue : tâche relayée par un worker d'ancien protocole")
    } else if lease.relay_target_offline {
        Some("bail perdu : le worker visé par le relais est hors ligne")
    } else {
        return Ok(None);
    };
    Ok(Some((
        JobErrorCode::LeaseExpired,
        message.map(str::to_owned),
    )))
}

async fn defer(conn: &mut SqliteConnection, job: Uuid) -> AppResult<LeaseOutcome> {
    sqlx::query(
        "UPDATE jobs SET lease_orphaned=1, \
         unreachable_since=COALESCE(unreachable_since, datetime('now')) WHERE id=?1",
    )
    .bind(job)
    .execute(conn)
    .await?;
    Ok(LeaseOutcome::Deferred)
}

/// Échec définitif d'un bail perdu ; `message` l'explique au journal des étapes.
async fn fail_job(
    conn: &mut SqliteConnection,
    job: Uuid,
    code: JobErrorCode,
    message: Option<&str>,
) -> AppResult<LeaseOutcome> {
    if let Some(message) = message {
        let prefix: i64 = sqlx::query_scalar(
            "SELECT COALESCE(MAX(step_index), 0) FROM job_steps WHERE job_id=?1",
        )
        .bind(job)
        .fetch_one(&mut *conn)
        .await?;
        let step = JobStepEntry {
            step_index: APPENDED_STEP_INDEX,
            node_id: LEASE_STEP.into(),
            block_kind: LEASE_STEP.into(),
            operation_id: None,
            status: JobStepStatus::Failed,
            message: Some(message.to_owned()),
            output_excerpt: None,
            variables: None,
        };
        jobs::insert_job_step(conn, job, prefix, &step).await?;
    }
    set_failed(conn, job, code).await?;
    Ok(LeaseOutcome::Failed(code))
}

/// Passe la tâche à `failed` et annule son bail ; à appeler bail vérifié, dans la même transaction.
pub(in crate::repository) async fn set_failed(
    conn: &mut SqliteConnection,
    job: Uuid,
    code: JobErrorCode,
) -> AppResult<()> {
    sqlx::query(
        "UPDATE jobs SET status=?2, error_code=?3, lease_token=NULL, lease_epoch=lease_epoch+1, \
         leased_until=NULL, unreachable_since=NULL, lease_orphaned=0, updated_at=datetime('now') \
         WHERE id=?1",
    )
    .bind(job)
    .bind(JobStatus::Failed)
    .bind(code)
    .execute(conn)
    .await?;
    Ok(())
}

/// Remet la tâche en file. `worker_id` est effacé avec le
/// jeton : l'ancien titulaire ne peut plus rien écrire avant même la réservation suivante.
async fn requeue(
    conn: &mut SqliteConnection,
    job: Uuid,
    refunded: bool,
) -> AppResult<LeaseOutcome> {
    sqlx::query(
        "UPDATE jobs SET status=?2, worker_id=NULL, lease_token=NULL, lease_epoch=lease_epoch+1, \
         leased_until=NULL, lease_contact_at=NULL, unreachable_since=NULL, lease_orphaned=0, \
         error_code=NULL, attempts=MAX(attempts-?3, 0), free_releases=free_releases+?3, \
         updated_at=datetime('now') WHERE id=?1",
    )
    .bind(job)
    .bind(JobStatus::Queued)
    .bind(i64::from(refunded))
    .execute(&mut *conn)
    .await?;
    sqlx::query("DELETE FROM source_tickets WHERE job_id=?1")
        .bind(job)
        .execute(conn)
        .await?;
    Ok(LeaseOutcome::Requeued)
}

/// Oublie ce que l'ancien titulaire retenait et un point de reprise qu'il a pu consommer.
async fn forget_lost_attempt(conn: &mut SqliteConnection, job: Uuid) -> AppResult<()> {
    relays::restart_from_source(conn, job).await?;
    sqlx::query("DELETE FROM job_artifacts WHERE job_id=?1")
        .bind(job)
        .execute(conn)
        .await?;
    Ok(())
}
