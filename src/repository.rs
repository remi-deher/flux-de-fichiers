use crate::{
    domain::{Claim, Job, LEASE_SECONDS, LeaseRequest, MAX_ATTEMPTS, Worker},
    error::{AppError, AppResult},
};
use sqlx::SqlitePool;
use uuid::Uuid;

pub async fn register(pool: &SqlitePool, name: &str) -> AppResult<Worker> {
    Ok(sqlx::query_as("INSERT INTO workers(id,name) VALUES(?1,?2) ON CONFLICT(name) DO UPDATE SET last_seen=datetime('now') RETURNING *")
        .bind(Uuid::new_v4()).bind(name).fetch_one(pool).await?)
}

pub async fn claim(pool: &SqlitePool, worker: Uuid) -> AppResult<Option<Claim>> {
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    if sqlx::query("UPDATE workers SET last_seen=datetime('now') WHERE id=?1")
        .bind(worker)
        .execute(&mut *tx)
        .await?
        .rows_affected()
        != 1
    {
        return Err(AppError::NotFound);
    }
    sqlx::query("UPDATE jobs SET status=CASE WHEN attempts >= ?1 THEN 'failed' ELSE 'queued' END, error_code='lease_expired', lease_token=NULL, leased_until=NULL, updated_at=datetime('now') WHERE status='running' AND leased_until <= datetime('now')")
        .bind(MAX_ATTEMPTS).execute(&mut *tx).await?;
    let next_id: Option<Uuid> = sqlx::query_scalar("SELECT id FROM jobs WHERE status='queued' AND attempts < ?1 AND (target_worker_id IS NULL OR target_worker_id=?2) ORDER BY created_at,id LIMIT 1")
        .bind(MAX_ATTEMPTS).bind(worker).fetch_optional(&mut *tx).await?;
    let job: Option<Job> = if let Some(id) = next_id {
        sqlx::query_as("UPDATE jobs SET status='running', worker_id=?2, lease_token=?3, leased_until=datetime('now', '+' || ?4 || ' seconds'), attempts=attempts+1, error_code=NULL, updated_at=datetime('now') WHERE id=?1 RETURNING *")
            .bind(id).bind(worker).bind(Uuid::new_v4()).bind(LEASE_SECONDS).fetch_optional(&mut *tx).await?
    } else {
        None
    };
    tx.commit().await?;
    job.map(|j| {
        Ok(Claim {
            job_id: j.id,
            lease_token: j.lease_token.ok_or(AppError::Internal)?,
            source_name: j.source_name,
            source_sha256: j.source_sha256.ok_or(AppError::Internal)?,
            source_bytes: j.source_bytes.ok_or(AppError::Internal)?,
        })
    })
    .transpose()
}

pub async fn active(pool: &SqlitePool, id: Uuid, lease: &LeaseRequest) -> AppResult<Job> {
    sqlx::query_as("SELECT * FROM jobs WHERE id=?1 AND worker_id=?2 AND lease_token=?3 AND status='running' AND leased_until > datetime('now')")
        .bind(id).bind(lease.worker_id).bind(lease.lease_token).fetch_optional(pool).await?.ok_or(AppError::Conflict)
}

pub async fn heartbeat(pool: &SqlitePool, id: Uuid, lease: &LeaseRequest) -> AppResult<()> {
    let count = sqlx::query("UPDATE jobs SET leased_until=datetime('now', '+' || ?4 || ' seconds'),updated_at=datetime('now') WHERE id=?1 AND worker_id=?2 AND lease_token=?3 AND status='running' AND leased_until > datetime('now')")
        .bind(id).bind(lease.worker_id).bind(lease.lease_token).bind(LEASE_SECONDS).execute(pool).await?.rows_affected();
    if count != 1 {
        return Err(AppError::Conflict);
    }
    Ok(())
}

pub async fn fail(pool: &SqlitePool, id: Uuid, lease: &LeaseRequest, code: &str) -> AppResult<()> {
    let count = sqlx::query("UPDATE jobs SET status='failed',error_code=?4,lease_token=NULL,leased_until=NULL,updated_at=datetime('now') WHERE id=?1 AND worker_id=?2 AND lease_token=?3 AND status='running' AND leased_until > datetime('now')")
        .bind(id).bind(lease.worker_id).bind(lease.lease_token).bind(code).execute(pool).await?.rows_affected();
    if count != 1 {
        return Err(AppError::Conflict);
    }
    Ok(())
}
