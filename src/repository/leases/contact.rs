//! Contact du titulaire : contrôle du bail, échéance, battement.

use crate::{
    domain::{Job, JobErrorCode, JobStatus, LEASE_SECONDS, LeaseRequest},
    error::{AppError, AppResult},
};
use forgearr_shared::jobs::{LeaseProtocol, UNREACHABLE_AFTER_SECONDS};
use sqlx::{SqliteConnection, SqlitePool};
use uuid::Uuid;

/// Durée d'un bail entre deux contacts, selon le protocole de son titulaire.
pub(in crate::repository) fn lease_window_seconds(
    protocol: LeaseProtocol,
    unreachable_timeout: i64,
) -> i64 {
    match protocol {
        LeaseProtocol::Legacy => LEASE_SECONDS,
        LeaseProtocol::Reachability => UNREACHABLE_AFTER_SECONDS + unreachable_timeout,
    }
}

/// Refuse (`LeaseLost`) tout appelant qui n'est pas le titulaire actuel de la tâche.
pub(in crate::repository) async fn ensure_held(
    conn: &mut SqliteConnection,
    job: Uuid,
    lease: &LeaseRequest,
) -> AppResult<()> {
    let held: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM jobs WHERE id=?1 AND worker_id=?2 AND lease_token=?3 \
         AND lease_epoch=?4 AND status=?5)",
    )
    .bind(job)
    .bind(lease.worker_id)
    .bind(lease.lease_token)
    .bind(lease.lease_epoch)
    .bind(JobStatus::Running)
    .fetch_one(conn)
    .await?;
    if held {
        Ok(())
    } else {
        Err(AppError::LeaseLost)
    }
}

/// La tâche si l'appelant en est le titulaire.
pub async fn active(pool: &SqlitePool, id: Uuid, lease: &LeaseRequest) -> AppResult<Job> {
    sqlx::query_as::<_, Job>(
        "SELECT * FROM jobs WHERE id=?1 AND worker_id=?2 AND lease_token=?3 AND lease_epoch=?4 \
         AND status=?5",
    )
    .bind(id)
    .bind(lease.worker_id)
    .bind(lease.lease_token)
    .bind(lease.lease_epoch)
    .bind(JobStatus::Running)
    .fetch_optional(pool)
    .await?
    .ok_or(AppError::LeaseLost)
}

/// Ce bail s'est-il terminé par un succès ? Une réponse perdue se retrouve ainsi.
pub async fn completed_by_lease(
    pool: &SqlitePool,
    job: Uuid,
    lease: &LeaseRequest,
) -> AppResult<bool> {
    ended_by_lease(pool, job, lease, JobStatus::Succeeded, None).await
}

/// Ce bail s'est-il terminé par cet échec ?
pub async fn failed_by_lease(
    pool: &SqlitePool,
    job: Uuid,
    lease: &LeaseRequest,
    code: JobErrorCode,
) -> AppResult<bool> {
    ended_by_lease(pool, job, lease, JobStatus::Failed, Some(code)).await
}

async fn ended_by_lease(
    pool: &SqlitePool,
    job: Uuid,
    lease: &LeaseRequest,
    status: JobStatus,
    code: Option<JobErrorCode>,
) -> AppResult<bool> {
    Ok(sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM jobs WHERE id=?1 AND worker_id=?2 AND lease_epoch=?3+1 \
         AND status=?4 AND error_code IS ?5)",
    )
    .bind(job)
    .bind(lease.worker_id)
    .bind(lease.lease_epoch)
    .bind(status)
    .bind(code)
    .fetch_one(pool)
    .await?)
}

/// Ouvre le contact d'un bail qui vient d'être attribué.
pub(in crate::repository) async fn start_lease(
    conn: &mut SqliteConnection,
    job: Uuid,
    protocol: LeaseProtocol,
) -> AppResult<()> {
    sqlx::query(
        "UPDATE jobs SET lease_contact_at=datetime('now'), unreachable_since=NULL, \
         lease_orphaned=0, lease_protocol=?2 WHERE id=?1",
    )
    .bind(job)
    .bind(protocol.number())
    .execute(conn)
    .await?;
    Ok(())
}

/// Contact du titulaire : échéance repoussée, injoignabilité effacée. Rend `true` si la tâche était
/// injoignable. À appeler après [`ensure_held`], dans la même transaction.
pub(in crate::repository) async fn refresh_contact(
    conn: &mut SqliteConnection,
    job: Uuid,
    unreachable_timeout: i64,
) -> AppResult<bool> {
    let (protocol, unreachable): (i64, bool) = sqlx::query_as(
        "SELECT lease_protocol, unreachable_since IS NOT NULL FROM jobs WHERE id=?1",
    )
    .bind(job)
    .fetch_one(&mut *conn)
    .await?;
    let window = lease_window_seconds(LeaseProtocol::from_number(protocol), unreachable_timeout);
    sqlx::query(
        "UPDATE jobs SET lease_contact_at=datetime('now'), unreachable_since=NULL, \
         lease_orphaned=0, leased_until=datetime('now', '+' || ?2 || ' seconds'), \
         updated_at=datetime('now') WHERE id=?1",
    )
    .bind(job)
    .bind(window)
    .execute(&mut *conn)
    .await?;
    Ok(unreachable)
}

/// Contact du titulaire hors de toute autre écriture ; `true` si la tâche était injoignable.
pub async fn renew_contact(pool: &SqlitePool, id: Uuid, lease: &LeaseRequest) -> AppResult<bool> {
    let timeout = crate::runtime_settings::load_unreachable_timeout(pool).await;
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    ensure_held(&mut tx, id, lease).await?;
    let restored = refresh_contact(&mut tx, id, timeout).await?;
    tx.commit().await?;
    Ok(restored)
}

/// Battement du titulaire ; `true` si la tâche était injoignable et vient d'être rétablie.
pub async fn heartbeat(pool: &SqlitePool, id: Uuid, lease: &LeaseRequest) -> AppResult<bool> {
    let restored = renew_contact(pool, id, lease).await?;
    let telemetry = lease.telemetry.as_ref();
    crate::repository::touch_worker_telemetry(pool, lease.worker_id, None, telemetry).await?;
    Ok(restored)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_old_worker_keeps_its_short_lease() {
        assert_eq!(
            lease_window_seconds(LeaseProtocol::Legacy, 600),
            LEASE_SECONDS
        );
        assert_eq!(
            lease_window_seconds(LeaseProtocol::Reachability, 600),
            UNREACHABLE_AFTER_SECONDS + 600
        );
    }
}
