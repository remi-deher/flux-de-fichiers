use super::flows::DEFAULT_FLOW_SQL;
use crate::{
    domain::{
        Claim, Job, JobErrorCode, JobStatus, JobStepEntry, JobStepLog, LEASE_SECONDS, LeaseRequest,
        MAX_ATTEMPTS, Worker,
    },
    error::{AppError, AppResult},
    telemetry::TelemetrySnapshot,
};
use chrono::{DateTime, Utc};
use flux_shared::workers::WorkerTelemetry;
use sqlx::SqlitePool;
use std::future::Future;
use uuid::Uuid;

#[derive(sqlx::FromRow)]
struct WorkerRow {
    id: Uuid,
    name: String,
    last_seen: DateTime<Utc>,
    version: Option<String>,
    remote_addr: Option<String>,
    telemetry_json: Option<String>,
}

impl WorkerRow {
    fn into_worker(self) -> Worker {
        let telemetry = self
            .telemetry_json
            .as_deref()
            .and_then(|raw| serde_json::from_str::<WorkerTelemetry>(raw).ok());
        Worker {
            id: self.id,
            name: self.name,
            last_seen: self.last_seen,
            version: self.version,
            remote_addr: self.remote_addr,
            telemetry,
        }
    }
}

fn serialize_telemetry(snapshot: Option<&TelemetrySnapshot>) -> Option<String> {
    snapshot
        .map(TelemetrySnapshot::to_worker_telemetry)
        .and_then(|t| serde_json::to_string(&t).ok())
}

pub async fn ping(pool: &SqlitePool) -> AppResult<()> {
    sqlx::query("SELECT 1").execute(pool).await?;
    Ok(())
}

pub async fn register_with_metadata(
    pool: &SqlitePool,
    name: &str,
    version: Option<&str>,
    remote_addr: Option<&str>,
    telemetry: Option<&TelemetrySnapshot>,
) -> AppResult<Worker> {
    let telemetry_json = serialize_telemetry(telemetry);
    let row = sqlx::query_as::<_, WorkerRow>(
        "INSERT INTO workers(id, name, version, remote_addr, telemetry_json) \
         VALUES(?1, ?2, ?3, ?4, ?5) \
         ON CONFLICT(name) DO UPDATE SET \
             last_seen=datetime('now'), \
             version=COALESCE(excluded.version, workers.version), \
             remote_addr=COALESCE(excluded.remote_addr, workers.remote_addr), \
             telemetry_json=COALESCE(excluded.telemetry_json, workers.telemetry_json) \
         RETURNING *",
    )
    .bind(Uuid::new_v4())
    .bind(name)
    .bind(version)
    .bind(remote_addr)
    .bind(telemetry_json)
    .fetch_one(pool)
    .await?;
    Ok(row.into_worker())
}

pub async fn register(pool: &SqlitePool, name: &str) -> AppResult<Worker> {
    register_with_metadata(pool, name, None, None, None).await
}

pub async fn touch_worker_telemetry(
    pool: &SqlitePool,
    worker_id: Uuid,
    remote_addr: Option<&str>,
    telemetry: Option<&TelemetrySnapshot>,
) -> AppResult<()> {
    let telemetry_json = serialize_telemetry(telemetry);
    sqlx::query(
        "UPDATE workers SET \
             last_seen=datetime('now'), \
             remote_addr=COALESCE(?2, remote_addr), \
             telemetry_json=COALESCE(?3, telemetry_json) \
         WHERE id=?1",
    )
    .bind(worker_id)
    .bind(remote_addr)
    .bind(telemetry_json)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn worker_exists(pool: &SqlitePool, id: Uuid) -> AppResult<bool> {
    let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM workers WHERE id=?1)")
        .bind(id)
        .fetch_one(pool)
        .await?;
    Ok(exists)
}

pub async fn get_worker(pool: &SqlitePool, id: Uuid) -> AppResult<Worker> {
    let row = sqlx::query_as::<_, WorkerRow>("SELECT * FROM workers WHERE id=?1")
        .bind(id)
        .fetch_optional(pool)
        .await?
        .ok_or(AppError::NotFound)?;
    Ok(row.into_worker())
}

pub async fn find_worker_by_name(pool: &SqlitePool, name: &str) -> AppResult<Option<Worker>> {
    let row = sqlx::query_as::<_, WorkerRow>("SELECT * FROM workers WHERE name=?1")
        .bind(name)
        .fetch_optional(pool)
        .await?;
    Ok(row.map(WorkerRow::into_worker))
}

/// Tâches traitées par ce worker, les plus récentes d'abord.
pub async fn list_jobs_for_worker(
    pool: &SqlitePool,
    worker: Uuid,
    limit: i64,
) -> AppResult<Vec<Job>> {
    Ok(sqlx::query_as(
        "SELECT * FROM jobs WHERE worker_id=?1 ORDER BY created_at DESC, id DESC LIMIT ?2",
    )
    .bind(worker)
    .bind(limit)
    .fetch_all(pool)
    .await?)
}

/// Nombre de tâches de ce worker par statut.
pub async fn count_jobs_for_worker(
    pool: &SqlitePool,
    worker: Uuid,
) -> AppResult<Vec<(JobStatus, i64)>> {
    Ok(
        sqlx::query_as("SELECT status, COUNT(*) FROM jobs WHERE worker_id=?1 GROUP BY status")
            .bind(worker)
            .fetch_all(pool)
            .await?,
    )
}

pub async fn list_workers(pool: &SqlitePool) -> AppResult<Vec<Worker>> {
    let workers = sqlx::query_as::<_, WorkerRow>("SELECT * FROM workers ORDER BY name LIMIT 200")
        .fetch_all(pool)
        .await?;
    Ok(workers.into_iter().map(WorkerRow::into_worker).collect())
}

pub async fn delete_worker(pool: &SqlitePool, id: Uuid) -> AppResult<()> {
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    sqlx::query("DELETE FROM library_workers WHERE worker_id=?1")
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
    insert_job_with_options(
        pool,
        source_name,
        target_worker_id,
        source_sha256,
        source_bytes,
        None,
        None,
    )
    .await
}

/// Crée une tâche en figeant le flow explicitement demandé, sinon celui de la bibliothèque, sinon le défaut.
pub async fn insert_job_with_options(
    pool: &SqlitePool,
    source_name: &str,
    target_worker_id: Option<Uuid>,
    source_sha256: &str,
    source_bytes: i64,
    library_id: Option<Uuid>,
    explicit_flow_id: Option<&str>,
) -> AppResult<Job> {
    let mut chosen_flow_id: Option<String> = explicit_flow_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned);

    if let Some(lib_id) = library_id {
        let row: Option<(Option<String>,)> =
            sqlx::query_as("SELECT flow_id FROM libraries WHERE id=?1")
                .bind(lib_id)
                .fetch_optional(pool)
                .await?;
        let Some((lib_flow_id,)) = row else {
            return Err(AppError::Invalid);
        };
        if chosen_flow_id.is_none() {
            chosen_flow_id = lib_flow_id;
        }
    }

    let resolved_flow: Option<(String, i64)> = if let Some(flow_id) = chosen_flow_id.as_deref() {
        let found: Option<(String, i64)> =
            sqlx::query_as("SELECT id, version FROM flows WHERE id=?1")
                .bind(flow_id)
                .fetch_optional(pool)
                .await?;
        if found.is_none() {
            return Err(AppError::Invalid);
        }
        found
    } else {
        sqlx::query_as::<_, crate::domain::FlowRecord>(DEFAULT_FLOW_SQL)
            .fetch_optional(pool)
            .await?
            .map(|flow| (flow.id, flow.version))
    };

    let (flow_id, flow_version) = match resolved_flow {
        Some((id, version)) => (Some(id), Some(version)),
        None => (None, None),
    };

    let job = sqlx::query_as(
        "INSERT INTO jobs(id, source_name, target_worker_id, source_sha256, source_bytes, \
                          library_id, flow_id, flow_version) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8) RETURNING *",
    )
    .bind(Uuid::new_v4())
    .bind(source_name)
    .bind(target_worker_id)
    .bind(source_sha256)
    .bind(source_bytes)
    .bind(library_id)
    .bind(flow_id)
    .bind(flow_version)
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

/// Indique si la bibliothèque possède déjà une tâche active ou réussie pour ce fichier et cette empreinte.
pub async fn has_library_file_job(
    pool: &SqlitePool,
    library_id: Uuid,
    source_name: &str,
    source_sha256: &str,
) -> AppResult<bool> {
    let found: Option<(i64,)> = sqlx::query_as(
        "SELECT 1 FROM jobs \
         WHERE library_id=?1 AND source_name=?2 AND source_sha256=?3 \
         AND status IN ('queued', 'running', 'succeeded') \
         LIMIT 1",
    )
    .bind(library_id)
    .bind(source_name)
    .bind(source_sha256)
    .fetch_optional(pool)
    .await?;
    Ok(found.is_some())
}

/// Supprime ou annule une tâche qui n'est pas en cours d'exécution (`running`).
pub async fn delete_job(pool: &SqlitePool, id: Uuid) -> AppResult<()> {
    let row: Option<(String,)> = sqlx::query_as("SELECT status FROM jobs WHERE id=?1")
        .bind(id)
        .fetch_optional(pool)
        .await?;
    let Some((status,)) = row else {
        return Err(AppError::NotFound);
    };
    if status == "running" {
        return Err(AppError::Conflict);
    }
    sqlx::query("DELETE FROM jobs WHERE id=?1 AND status != 'running'")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
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
    // Bail expiré : échec explicite, jamais de remise en file silencieuse (relance : `retry_job`).
    sqlx::query(
        "UPDATE jobs \
         SET status=?1, error_code=?2, lease_token=NULL, lease_epoch=lease_epoch+1, \
             leased_until=NULL, updated_at=datetime('now') \
         WHERE status=?3 AND leased_until <= datetime('now')",
    )
    .bind(JobStatus::Failed)
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
             SET status=?2, worker_id=?3, lease_token=?4, lease_epoch=lease_epoch+1, \
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
    match job {
        None => Ok(None),
        Some(j) => {
            let frozen_flow = match (&j.flow_id, j.flow_version) {
                (Some(flow_id), Some(version)) => {
                    Some(super::get_flow_version(pool, flow_id, version).await?)
                }
                _ => None,
            };
            Ok(Some(Claim {
                job_id: j.id,
                lease_token: j.lease_token.ok_or(AppError::Internal)?,
                lease_epoch: j.lease_epoch,
                source_name: j.source_name,
                source_sha256: j.source_sha256.ok_or(AppError::Internal)?,
                source_bytes: j.source_bytes.ok_or(AppError::Internal)?,
                library_id: j.library_id,
                probe_tool_id: None,
                probe_version: None,
                required_tools: Vec::new(),
                frozen_flow,
                operations: Default::default(),
                block_operations: Default::default(),
                plugin_configs: Default::default(),
                sealed_secrets: None,
                allow_private_integrations: None,
                media_root: None,
            }))
        }
    }
}

/// Relance manuelle d'une tâche échouée après perte de bail, dans la limite de `MAX_ATTEMPTS`.
pub async fn retry_job(pool: &SqlitePool, id: Uuid) -> AppResult<Job> {
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM jobs WHERE id=?1)")
        .bind(id)
        .fetch_one(&mut *tx)
        .await?;
    if !exists {
        return Err(AppError::NotFound);
    }
    let job: Option<Job> = sqlx::query_as(
        "UPDATE jobs \
         SET status=?2, worker_id=NULL, lease_token=NULL, lease_epoch=lease_epoch+1, \
             leased_until=NULL, error_code=NULL, updated_at=datetime('now') \
         WHERE id=?1 AND status=?3 AND error_code=?4 AND attempts < ?5 RETURNING *",
    )
    .bind(id)
    .bind(JobStatus::Queued)
    .bind(JobStatus::Failed)
    .bind(JobErrorCode::LeaseExpired)
    .bind(MAX_ATTEMPTS)
    .fetch_optional(&mut *tx)
    .await?;
    let job = job.ok_or(AppError::Conflict)?;
    tx.commit().await?;
    Ok(job)
}

pub async fn active(pool: &SqlitePool, id: Uuid, lease: &LeaseRequest) -> AppResult<Job> {
    sqlx::query_as(
        "SELECT * FROM jobs \
         WHERE id=?1 AND worker_id=?2 AND lease_token=?3 AND lease_epoch=?4 \
           AND status=?5 AND leased_until > datetime('now')",
    )
    .bind(id)
    .bind(lease.worker_id)
    .bind(lease.lease_token)
    .bind(lease.lease_epoch)
    .bind(JobStatus::Running)
    .fetch_optional(pool)
    .await?
    .ok_or(AppError::Conflict)
}

pub async fn heartbeat(pool: &SqlitePool, id: Uuid, lease: &LeaseRequest) -> AppResult<()> {
    let count = sqlx::query(
        "UPDATE jobs \
         SET leased_until=datetime('now', '+' || ?5 || ' seconds'), updated_at=datetime('now') \
         WHERE id=?1 AND worker_id=?2 AND lease_token=?3 AND lease_epoch=?4 \
           AND status=?6 AND leased_until > datetime('now')",
    )
    .bind(id)
    .bind(lease.worker_id)
    .bind(lease.lease_token)
    .bind(lease.lease_epoch)
    .bind(LEASE_SECONDS)
    .bind(JobStatus::Running)
    .execute(pool)
    .await?
    .rows_affected();
    if count != 1 {
        return Err(AppError::Conflict);
    }
    touch_worker_telemetry(pool, lease.worker_id, None, lease.telemetry.as_ref()).await?;
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
         SET status=?5, error_code=?6, lease_token=NULL, lease_epoch=lease_epoch+1, \
             leased_until=NULL, updated_at=datetime('now') \
         WHERE id=?1 AND worker_id=?2 AND lease_token=?3 AND lease_epoch=?4 \
           AND status=?7 AND leased_until > datetime('now')",
    )
    .bind(id)
    .bind(lease.worker_id)
    .bind(lease.lease_token)
    .bind(lease.lease_epoch)
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
         WHERE id=?1 AND worker_id=?2 AND lease_token=?3 AND lease_epoch=?4 \
           AND status=?5 AND leased_until > datetime('now')",
    )
    .bind(id)
    .bind(lease.worker_id)
    .bind(lease.lease_token)
    .bind(lease.lease_epoch)
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
             lease_token=NULL, lease_epoch=lease_epoch+1, leased_until=NULL, error_code=NULL, \
             updated_at=datetime('now') \
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

/// Enregistre de façon idempotente la trace d'exécution par nœud d'un job.
pub async fn save_job_steps(
    pool: &SqlitePool,
    job_id: Uuid,
    steps: &[JobStepEntry],
) -> AppResult<()> {
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM jobs WHERE id=?1)")
        .bind(job_id)
        .fetch_one(&mut *tx)
        .await?;
    if !exists {
        return Err(AppError::NotFound);
    }
    sqlx::query("DELETE FROM job_steps WHERE job_id=?1")
        .bind(job_id)
        .execute(&mut *tx)
        .await?;
    for step in steps {
        sqlx::query(
            "INSERT INTO job_steps (job_id, step_index, node_id, block_kind, operation_id, \
                                    status, message, output_excerpt) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        )
        .bind(job_id)
        .bind(step.step_index)
        .bind(&step.node_id)
        .bind(&step.block_kind)
        .bind(&step.operation_id)
        .bind(step.status)
        .bind(&step.message)
        .bind(&step.output_excerpt)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(())
}

/// Retourne le journal ordonné des étapes d'un job.
pub async fn list_job_steps(pool: &SqlitePool, job_id: Uuid) -> AppResult<Vec<JobStepLog>> {
    let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM jobs WHERE id=?1)")
        .bind(job_id)
        .fetch_one(pool)
        .await?;
    if !exists {
        return Err(AppError::NotFound);
    }
    Ok(sqlx::query_as(
        "SELECT id, job_id, step_index, node_id, block_kind, operation_id, \
                status, message, output_excerpt, created_at \
         FROM job_steps WHERE job_id=?1 ORDER BY step_index, id",
    )
    .bind(job_id)
    .fetch_all(pool)
    .await?)
}

/// Efface la référence au fichier de sortie d'un job non en cours et retourne son ancien nom pour suppression explicite.
pub async fn clear_job_output_name(pool: &SqlitePool, id: Uuid) -> AppResult<Option<String>> {
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    let row: Option<(JobStatus, Option<String>)> =
        sqlx::query_as("SELECT status, output_name FROM jobs WHERE id=?1")
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?;
    let Some((status, output_name)) = row else {
        return Err(AppError::NotFound);
    };
    if status == JobStatus::Running {
        return Err(AppError::Conflict);
    }
    sqlx::query(
        "UPDATE jobs SET output_name=NULL, output_sha256=NULL, output_bytes=NULL, \
         updated_at=datetime('now') WHERE id=?1",
    )
    .bind(id)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(output_name)
}
