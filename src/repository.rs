use crate::{
    domain::{Claim, Job, LEASE_SECONDS, LeaseRequest, MAX_ATTEMPTS, Worker},
    error::{AppError, AppResult},
};
use sqlx::PgPool;
use uuid::Uuid;

pub async fn register(pool: &PgPool, name: &str) -> AppResult<Worker> {
    Ok(sqlx::query_as("INSERT INTO workers(id,name) VALUES($1,$2) ON CONFLICT(name) DO UPDATE SET last_seen=now() RETURNING *")
        .bind(Uuid::new_v4()).bind(name).fetch_one(pool).await?)
}

pub async fn claim(pool: &PgPool, worker: Uuid) -> AppResult<Option<Claim>> {
    let mut tx = pool.begin().await?;
    if sqlx::query("UPDATE workers SET last_seen=now() WHERE id=$1")
        .bind(worker)
        .execute(&mut *tx)
        .await?
        .rows_affected()
        != 1
    {
        return Err(AppError::NotFound);
    }
    sqlx::query("UPDATE jobs SET status=CASE WHEN attempts >= $1 THEN 'failed' ELSE 'queued' END, error_code='lease_expired', lease_token=NULL, leased_until=NULL, updated_at=now() WHERE status='running' AND leased_until <= now()")
        .bind(MAX_ATTEMPTS).execute(&mut *tx).await?;
    let job: Option<Job> = sqlx::query_as("WITH next AS (SELECT id FROM jobs WHERE status='queued' AND attempts < $1 AND (target_worker_id IS NULL OR target_worker_id=$2) ORDER BY created_at,id FOR UPDATE SKIP LOCKED LIMIT 1) UPDATE jobs SET status='running', worker_id=$2, lease_token=$3, leased_until=now()+make_interval(secs=>$4), attempts=attempts+1, error_code=NULL, updated_at=now() FROM next WHERE jobs.id=next.id RETURNING jobs.*")
        .bind(MAX_ATTEMPTS).bind(worker).bind(Uuid::new_v4()).bind(LEASE_SECONDS as f64).fetch_optional(&mut *tx).await?;
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

pub async fn active(pool: &PgPool, id: Uuid, lease: &LeaseRequest) -> AppResult<Job> {
    sqlx::query_as("SELECT * FROM jobs WHERE id=$1 AND worker_id=$2 AND lease_token=$3 AND status='running' AND leased_until > now()")
        .bind(id).bind(lease.worker_id).bind(lease.lease_token).fetch_optional(pool).await?.ok_or(AppError::Conflict)
}

pub async fn heartbeat(pool: &PgPool, id: Uuid, lease: &LeaseRequest) -> AppResult<()> {
    let count = sqlx::query("UPDATE jobs SET leased_until=now()+make_interval(secs=>$4),updated_at=now() WHERE id=$1 AND worker_id=$2 AND lease_token=$3 AND status='running' AND leased_until>now()")
        .bind(id).bind(lease.worker_id).bind(lease.lease_token).bind(LEASE_SECONDS as f64).execute(pool).await?.rows_affected();
    if count != 1 {
        return Err(AppError::Conflict);
    }
    Ok(())
}

pub async fn fail(pool: &PgPool, id: Uuid, lease: &LeaseRequest, code: &str) -> AppResult<()> {
    let count = sqlx::query("UPDATE jobs SET status='failed',error_code=$4,lease_token=NULL,leased_until=NULL,updated_at=now() WHERE id=$1 AND worker_id=$2 AND lease_token=$3 AND status='running' AND leased_until>now()")
        .bind(id).bind(lease.worker_id).bind(lease.lease_token).bind(code).execute(pool).await?.rows_affected();
    if count != 1 {
        return Err(AppError::Conflict);
    }
    Ok(())
}
