use crate::{
    domain::{
        Claim, Job, JobErrorCode, JobStatus, LEASE_SECONDS, LeaseRequest, MAX_ATTEMPTS, Worker,
    },
    error::{AppError, AppResult},
};
use sqlx::SqlitePool;
use std::future::Future;
use uuid::Uuid;

pub async fn ping(pool: &SqlitePool) -> AppResult<()> {
    sqlx::query("SELECT 1").execute(pool).await?;
    Ok(())
}

pub async fn register(pool: &SqlitePool, name: &str) -> AppResult<Worker> {
    let worker = sqlx::query_as(
        "INSERT INTO workers(id, name) VALUES(?1, ?2) \
         ON CONFLICT(name) DO UPDATE SET last_seen=datetime('now') RETURNING *",
    )
    .bind(Uuid::new_v4())
    .bind(name)
    .fetch_one(pool)
    .await?;
    Ok(worker)
}

pub async fn worker_exists(pool: &SqlitePool, id: Uuid) -> AppResult<bool> {
    let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM workers WHERE id=?1)")
        .bind(id)
        .fetch_one(pool)
        .await?;
    Ok(exists)
}

pub async fn list_workers(pool: &SqlitePool) -> AppResult<Vec<Worker>> {
    let workers = sqlx::query_as("SELECT * FROM workers ORDER BY name LIMIT 200")
        .fetch_all(pool)
        .await?;
    Ok(workers)
}

pub async fn delete_worker(pool: &SqlitePool, id: Uuid) -> AppResult<()> {
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    sqlx::query("DELETE FROM library_nodes WHERE worker_id=?1")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    let affected = sqlx::query("DELETE FROM workers WHERE id=?1")
        .bind(id)
        .execute(&mut *tx)
        .await?
        .rows_affected();
    if affected == 0 {
        return Err(AppError::NotFound);
    }
    tx.commit().await?;
    Ok(())
}

pub async fn insert_job(
    pool: &SqlitePool,
    source_name: &str,
    target_worker_id: Option<Uuid>,
    source_sha256: &str,
    source_bytes: i64,
) -> AppResult<Job> {
    let job = sqlx::query_as(
        "INSERT INTO jobs(id, source_name, target_worker_id, source_sha256, source_bytes) \
         VALUES(?1, ?2, ?3, ?4, ?5) RETURNING *",
    )
    .bind(Uuid::new_v4())
    .bind(source_name)
    .bind(target_worker_id)
    .bind(source_sha256)
    .bind(source_bytes)
    .fetch_one(pool)
    .await?;
    Ok(job)
}

pub async fn get_job(pool: &SqlitePool, id: Uuid) -> AppResult<Job> {
    sqlx::query_as("SELECT * FROM jobs WHERE id=?1")
        .bind(id)
        .fetch_optional(pool)
        .await?
        .ok_or(AppError::NotFound)
}

pub async fn list_jobs(pool: &SqlitePool, before: Option<Uuid>, limit: i64) -> AppResult<Vec<Job>> {
    let jobs = sqlx::query_as(
        "SELECT * FROM jobs \
         WHERE (?1 IS NULL OR (created_at, id) < (SELECT created_at, id FROM jobs WHERE id=?1)) \
         ORDER BY created_at DESC, id DESC LIMIT ?2",
    )
    .bind(before)
    .bind(limit)
    .fetch_all(pool)
    .await?;
    Ok(jobs)
}

pub async fn claim(pool: &SqlitePool, worker: Uuid) -> AppResult<Option<Claim>> {
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    let touched = sqlx::query("UPDATE workers SET last_seen=datetime('now') WHERE id=?1")
        .bind(worker)
        .execute(&mut *tx)
        .await?
        .rows_affected();
    if touched != 1 {
        return Err(AppError::NotFound);
    }
    sqlx::query(
        "UPDATE jobs \
         SET status=CASE WHEN attempts >= ?1 THEN ?2 ELSE ?3 END, \
             error_code=?4, lease_token=NULL, leased_until=NULL, updated_at=datetime('now') \
         WHERE status=?5 AND leased_until <= datetime('now')",
    )
    .bind(MAX_ATTEMPTS)
    .bind(JobStatus::Failed)
    .bind(JobStatus::Queued)
    .bind(JobErrorCode::LeaseExpired)
    .bind(JobStatus::Running)
    .execute(&mut *tx)
    .await?;
    let next_id: Option<Uuid> = sqlx::query_scalar(
        "SELECT id FROM jobs \
         WHERE status=?1 AND attempts < ?2 \
           AND (target_worker_id IS NULL OR target_worker_id=?3) \
         ORDER BY created_at, id LIMIT 1",
    )
    .bind(JobStatus::Queued)
    .bind(MAX_ATTEMPTS)
    .bind(worker)
    .fetch_optional(&mut *tx)
    .await?;
    let job: Option<Job> = if let Some(id) = next_id {
        sqlx::query_as(
            "UPDATE jobs \
             SET status=?2, worker_id=?3, lease_token=?4, \
                 leased_until=datetime('now', '+' || ?5 || ' seconds'), \
                 attempts=attempts+1, error_code=NULL, updated_at=datetime('now') \
             WHERE id=?1 RETURNING *",
        )
        .bind(id)
        .bind(JobStatus::Running)
        .bind(worker)
        .bind(Uuid::new_v4())
        .bind(LEASE_SECONDS)
        .fetch_optional(&mut *tx)
        .await?
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
            probe_tool_id: None,
            probe_version: None,
        })
    })
    .transpose()
}

pub async fn active(pool: &SqlitePool, id: Uuid, lease: &LeaseRequest) -> AppResult<Job> {
    sqlx::query_as(
        "SELECT * FROM jobs \
         WHERE id=?1 AND worker_id=?2 AND lease_token=?3 AND status=?4 \
           AND leased_until > datetime('now')",
    )
    .bind(id)
    .bind(lease.worker_id)
    .bind(lease.lease_token)
    .bind(JobStatus::Running)
    .fetch_optional(pool)
    .await?
    .ok_or(AppError::Conflict)
}

pub async fn heartbeat(pool: &SqlitePool, id: Uuid, lease: &LeaseRequest) -> AppResult<()> {
    let count = sqlx::query(
        "UPDATE jobs \
         SET leased_until=datetime('now', '+' || ?4 || ' seconds'), updated_at=datetime('now') \
         WHERE id=?1 AND worker_id=?2 AND lease_token=?3 AND status=?5 \
           AND leased_until > datetime('now')",
    )
    .bind(id)
    .bind(lease.worker_id)
    .bind(lease.lease_token)
    .bind(LEASE_SECONDS)
    .bind(JobStatus::Running)
    .execute(pool)
    .await?
    .rows_affected();
    if count != 1 {
        return Err(AppError::Conflict);
    }
    Ok(())
}

pub async fn fail(
    pool: &SqlitePool,
    id: Uuid,
    lease: &LeaseRequest,
    code: JobErrorCode,
) -> AppResult<()> {
    let count = sqlx::query(
        "UPDATE jobs \
         SET status=?4, error_code=?5, lease_token=NULL, leased_until=NULL, \
             updated_at=datetime('now') \
         WHERE id=?1 AND worker_id=?2 AND lease_token=?3 AND status=?6 \
           AND leased_until > datetime('now')",
    )
    .bind(id)
    .bind(lease.worker_id)
    .bind(lease.lease_token)
    .bind(JobStatus::Failed)
    .bind(code)
    .bind(JobStatus::Running)
    .execute(pool)
    .await?
    .rows_affected();
    if count != 1 {
        return Err(AppError::Conflict);
    }
    Ok(())
}

pub async fn succeeded_output_name(pool: &SqlitePool, id: Uuid) -> AppResult<Option<String>> {
    let name: Option<Option<String>> =
        sqlx::query_scalar("SELECT output_name FROM jobs WHERE id=?1 AND status=?2")
            .bind(id)
            .bind(JobStatus::Succeeded)
            .fetch_optional(pool)
            .await?;
    Ok(name.flatten())
}

pub struct PublishedOutput<'a> {
    pub name: &'a str,
    pub sha256: &'a str,
    pub bytes: i64,
    pub warning: bool,
}

/// Vérifie le bail puis publie le résultat dans la même transaction immédiate.
/// `link_output` n'est attendu qu'après le contrôle du bail, verrou d'écriture tenu.
pub async fn publish_succeeded(
    pool: &SqlitePool,
    id: Uuid,
    lease: &LeaseRequest,
    output: PublishedOutput<'_>,
    link_output: impl Future<Output = AppResult<()>>,
) -> AppResult<()> {
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    let active: Option<Uuid> = sqlx::query_scalar(
        "SELECT id FROM jobs \
         WHERE id=?1 AND worker_id=?2 AND lease_token=?3 AND status=?4 \
           AND leased_until > datetime('now')",
    )
    .bind(id)
    .bind(lease.worker_id)
    .bind(lease.lease_token)
    .bind(JobStatus::Running)
    .fetch_optional(&mut *tx)
    .await?;
    if active.is_none() {
        return Err(AppError::Conflict);
    }
    link_output.await?;
    sqlx::query(
        "UPDATE jobs \
         SET status=?2, output_name=?3, output_sha256=?4, output_bytes=?5, warning=?6, \
             lease_token=NULL, leased_until=NULL, error_code=NULL, updated_at=datetime('now') \
         WHERE id=?1",
    )
    .bind(id)
    .bind(JobStatus::Succeeded)
    .bind(output.name)
    .bind(output.sha256)
    .bind(output.bytes)
    .bind(output.warning)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(())
}
