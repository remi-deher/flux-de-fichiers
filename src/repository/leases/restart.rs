//! Redémarrages et tâches sans exécutant : central, worker, tâche désavouée, bail orphelin.

use super::{
    LeaseKey,
    contact::lease_window_seconds,
    end::{end_all, end_one},
    outcome::{LeaseChange, LeaseEnd},
};
use crate::{
    domain::JobStatus,
    error::{AppError, AppResult},
};
use forgearr_shared::jobs::{LeaseProtocol, UNREACHABLE_AFTER_SECONDS};
use sqlx::{SqliteConnection, SqlitePool};
use uuid::Uuid;

/// Démarrage du central : toute tâche en cours devient injoignable, compteur à zéro.
pub async fn restart_contacts(pool: &SqlitePool) -> AppResult<u64> {
    let timeout = crate::runtime_settings::load_unreachable_timeout(pool).await;
    Ok(sqlx::query(
        "UPDATE jobs SET unreachable_since=datetime('now'), lease_contact_at=datetime('now'), \
         leased_until=datetime('now', '+' || CASE WHEN lease_protocol=?1 THEN ?2 ELSE ?3 END || ' seconds') \
         WHERE status=?4",
    )
    .bind(LeaseProtocol::Legacy.number())
    .bind(lease_window_seconds(LeaseProtocol::Legacy, timeout))
    .bind(lease_window_seconds(LeaseProtocol::Reachability, timeout))
    .bind(JobStatus::Running)
    .execute(pool)
    .await?
    .rows_affected())
}

/// Premier enregistrement d'un processus worker : si son démarrage est nouveau, les tâches que
/// l'ancien processus exécutait sont reprises. Un démarrage déjà connu ne change rien.
pub async fn register_boot(
    pool: &SqlitePool,
    worker: Uuid,
    boot_id: Uuid,
) -> AppResult<Vec<LeaseChange>> {
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    let known: Option<Uuid> = sqlx::query_scalar("SELECT boot_id FROM workers WHERE id=?1")
        .bind(worker)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(AppError::NotFound)?;
    if known == Some(boot_id) {
        return Ok(Vec::new());
    }
    sqlx::query("UPDATE workers SET boot_id=?2 WHERE id=?1")
        .bind(worker)
        .bind(boot_id)
        .execute(&mut *tx)
        .await?;
    // Lus avec l'écriture du démarrage : un bail pris ensuite par le nouveau processus n'y est pas.
    let held = running_leases(&mut tx, worker).await?;
    tx.commit().await?;
    Ok(end_all(pool, held, LeaseEnd::HolderStopped).await)
}

/// Démarrage du central : les tâches du worker intégré n'ont plus d'exécutant.
pub async fn end_integrated_leases(pool: &SqlitePool) -> AppResult<Vec<LeaseChange>> {
    let held: Vec<LeaseKey> = sqlx::query_as(
        "SELECT j.id, j.lease_epoch FROM jobs j JOIN workers w ON w.id=j.worker_id \
         WHERE w.serves_central=1 AND j.status=?1",
    )
    .bind(JobStatus::Running)
    .fetch_all(pool)
    .await?;
    Ok(end_all(pool, held, LeaseEnd::HolderStopped).await)
}

/// Tâches attribuées à ce worker qu'il dit ne pas tenir (réponse de réservation perdue, abandon) et
/// restées sans contact : reprises sans attendre l'échéance.
pub async fn end_disowned_leases(
    pool: &SqlitePool,
    worker: Uuid,
    active_jobs: &[Uuid],
) -> AppResult<Vec<LeaseChange>> {
    let silent: Vec<LeaseKey> = sqlx::query_as(
        "SELECT id, lease_epoch FROM jobs WHERE worker_id=?1 AND status=?2 AND lease_protocol<>?3 \
         AND lease_orphaned=0 AND lease_contact_at <= datetime('now', '-' || ?4 || ' seconds')",
    )
    .bind(worker)
    .bind(JobStatus::Running)
    .bind(LeaseProtocol::Legacy.number())
    .bind(UNREACHABLE_AFTER_SECONDS)
    .fetch_all(pool)
    .await?;
    let disowned = silent
        .into_iter()
        .filter(|(job, _)| !active_jobs.contains(job))
        .collect();
    Ok(end_all(pool, disowned, LeaseEnd::HolderStopped).await)
}

/// Conclut un bail orphelin dont la publication a été écartée sans avoir commencé.
pub async fn end_orphaned_lease(
    pool: &SqlitePool,
    (job, epoch): LeaseKey,
) -> AppResult<Option<LeaseChange>> {
    let orphaned: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM jobs WHERE id=?1 AND lease_epoch=?2 AND lease_orphaned=1)",
    )
    .bind(job)
    .bind(epoch)
    .fetch_one(pool)
    .await?;
    if !orphaned {
        return Ok(None);
    }
    end_one(pool, (job, epoch), LeaseEnd::HolderStopped, None).await
}

/// Le titulaire de ce bail a-t-il disparu en laissant une publication à conclure ?
pub async fn lease_orphaned(pool: &SqlitePool, job: Uuid) -> AppResult<bool> {
    Ok(
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM jobs WHERE id=?1 AND lease_orphaned=1)")
            .bind(job)
            .fetch_one(pool)
            .await?,
    )
}

async fn running_leases(conn: &mut SqliteConnection, worker: Uuid) -> AppResult<Vec<LeaseKey>> {
    Ok(
        sqlx::query_as("SELECT id, lease_epoch FROM jobs WHERE worker_id=?1 AND status=?2")
            .bind(worker)
            .bind(JobStatus::Running)
            .fetch_all(conn)
            .await?,
    )
}
