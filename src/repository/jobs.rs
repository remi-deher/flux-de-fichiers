use super::flows::default_flow_with;
use crate::{
    domain::{
        Claim, JOB_PRIORITY_MAX, JOB_PRIORITY_MIN, Job, JobBatchAction, JobBatchResult,
        JobErrorCode, JobEventType, JobStatus, JobStepEntry, JobStepLog, JobStepStatus,
        JobStorageStats, LEASE_SECONDS, LeaseRequest, LibraryJobStats, MAX_ATTEMPTS,
        WORKER_MAX_CONCURRENCY, WORKER_MIN_CONCURRENCY, Worker, WorkerSchedule,
    },
    error::{AppError, AppResult},
    telemetry::TelemetrySnapshot,
};
use chrono::{DateTime, Datelike, Timelike, Utc};
use forgearr_shared::workers::WorkerTelemetry;
use sqlx::SqlitePool;
use std::future::Future;
use uuid::Uuid;

const MAX_BATCH_IDS: usize = 500;

struct WorkerRow {
    id: Uuid,
    name: String,
    last_seen: DateTime<Utc>,
    version: Option<String>,
    remote_addr: Option<String>,
    telemetry_json: Option<String>,
    max_concurrency: i32,
    schedule_json: Option<String>,
}

impl WorkerRow {
    fn into_worker(self) -> Worker {
        let telemetry = self
            .telemetry_json
            .as_deref()
            .and_then(|raw| serde_json::from_str::<WorkerTelemetry>(raw).ok());
        let schedule = self
            .schedule_json
            .as_deref()
            .and_then(|raw| serde_json::from_str::<WorkerSchedule>(raw).ok());
        Worker {
            id: self.id,
            name: self.name,
            last_seen: self.last_seen,
            version: self.version,
            remote_addr: self.remote_addr,
            telemetry,
            max_concurrency: self
                .max_concurrency
                .clamp(WORKER_MIN_CONCURRENCY, WORKER_MAX_CONCURRENCY),
            schedule,
        }
    }
}

fn serialize_telemetry(snapshot: Option<&TelemetrySnapshot>) -> Option<String> {
    snapshot
        .map(TelemetrySnapshot::to_worker_telemetry)
        .and_then(|t| serde_json::to_string(&t).ok())
}

pub async fn ping(pool: &SqlitePool) -> AppResult<()> {
    sqlx::query!("SELECT 1 AS one").fetch_one(pool).await?;
    Ok(())
}

pub async fn register_with_metadata(
    pool: &SqlitePool,
    name: &str,
    version: Option<&str>,
    remote_addr: Option<&str>,
    telemetry: Option<&TelemetrySnapshot>,
) -> AppResult<Worker> {
    let id = Uuid::new_v4();
    let telemetry_json = serialize_telemetry(telemetry);
    let row = sqlx::query_as!(
        WorkerRow,
        r#"INSERT INTO workers(id, name, version, remote_addr, telemetry_json) VALUES(?1, ?2, ?3, ?4, ?5) ON CONFLICT(name) DO UPDATE SET last_seen=datetime('now'), version=COALESCE(excluded.version, workers.version), remote_addr=COALESCE(excluded.remote_addr, workers.remote_addr), telemetry_json=COALESCE(excluded.telemetry_json, workers.telemetry_json) RETURNING id AS "id!: Uuid", name, last_seen AS "last_seen: DateTime<Utc>", version, remote_addr, telemetry_json, max_concurrency AS "max_concurrency: i32", schedule_json"#,
        id,
        name,
        version,
        remote_addr,
        telemetry_json,
    )
    .fetch_one(pool)
    .await?;
    Ok(row.into_worker())
}

/// Met à jour la concurrence maximale et la fenêtre horaire d'un worker.
pub async fn update_worker_settings(
    pool: &SqlitePool,
    id: Uuid,
    max_concurrency: i32,
    schedule: Option<&WorkerSchedule>,
) -> AppResult<Worker> {
    if !(WORKER_MIN_CONCURRENCY..=WORKER_MAX_CONCURRENCY).contains(&max_concurrency) {
        return Err(AppError::Invalid);
    }
    if let Some(sched) = schedule
        && (sched.start_hour >= 24
            || sched.end_hour >= 24
            || sched.days.iter().any(|day| !(1..=7).contains(day)))
    {
        return Err(AppError::Invalid);
    }
    let schedule_json = schedule
        .filter(|s| s.enabled || !s.days.is_empty() || s.start_hour != 0 || s.end_hour != 0)
        .map(serde_json::to_string)
        .transpose()
        .map_err(|_| AppError::Invalid)?;
    let row = sqlx::query_as!(
        WorkerRow,
        r#"UPDATE workers SET max_concurrency = ?2, schedule_json = ?3 WHERE id = ?1 RETURNING id AS "id!: Uuid", name, last_seen AS "last_seen: DateTime<Utc>", version, remote_addr, telemetry_json, max_concurrency AS "max_concurrency: i32", schedule_json"#,
        id,
        max_concurrency,
        schedule_json,
    )
    .fetch_optional(pool)
    .await?
    .ok_or(AppError::NotFound)?;
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
    sqlx::query!(
        "UPDATE workers SET \
             last_seen=datetime('now'), \
             remote_addr=COALESCE(?2, remote_addr), \
             telemetry_json=COALESCE(?3, telemetry_json) \
         WHERE id=?1",
        worker_id,
        remote_addr,
        telemetry_json,
    )
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn worker_exists(pool: &SqlitePool, id: Uuid) -> AppResult<bool> {
    let exists = sqlx::query_scalar!(
        r#"SELECT EXISTS(SELECT 1 FROM workers WHERE id=?1) AS "exists!: bool""#,
        id,
    )
    .fetch_one(pool)
    .await?;
    Ok(exists)
}

pub async fn get_worker(pool: &SqlitePool, id: Uuid) -> AppResult<Worker> {
    let row = sqlx::query_as!(
        WorkerRow,
        r#"SELECT id AS "id!: Uuid", name, last_seen AS "last_seen: DateTime<Utc>", version, remote_addr, telemetry_json, max_concurrency AS "max_concurrency: i32", schedule_json FROM workers WHERE id=?1"#,
        id,
    )
    .fetch_optional(pool)
    .await?
    .ok_or(AppError::NotFound)?;
    Ok(row.into_worker())
}

pub async fn find_worker_by_name(pool: &SqlitePool, name: &str) -> AppResult<Option<Worker>> {
    let row = sqlx::query_as!(
        WorkerRow,
        r#"SELECT id AS "id!: Uuid", name, last_seen AS "last_seen: DateTime<Utc>", version, remote_addr, telemetry_json, max_concurrency AS "max_concurrency: i32", schedule_json FROM workers WHERE name=?1"#,
        name,
    )
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
    Ok(sqlx::query_as!(
        Job,
        r#"SELECT id AS "id!: Uuid", source_name, source_sha256, source_bytes, output_sha256, output_bytes, warning AS "warning: bool", target_worker_id AS "target_worker_id: Uuid", worker_id AS "worker_id: Uuid", status AS "status: JobStatus", attempts AS "attempts: i32", lease_epoch, lease_token AS "lease_token: Uuid", leased_until AS "leased_until: DateTime<Utc>", output_name, error_code AS "error_code: JobErrorCode", library_id AS "library_id: Uuid", flow_id, flow_version, priority AS "priority: i32", created_at AS "created_at: DateTime<Utc>", updated_at AS "updated_at: DateTime<Utc>" FROM jobs WHERE worker_id=?1 ORDER BY created_at DESC, id DESC LIMIT ?2"#,
        worker,
        limit,
    )
    .fetch_all(pool)
    .await?)
}

/// Nombre de tâches de ce worker par statut.
pub async fn count_jobs_for_worker(
    pool: &SqlitePool,
    worker: Uuid,
) -> AppResult<Vec<(JobStatus, i64)>> {
    let rows = sqlx::query!(
        r#"SELECT status AS "status: JobStatus", COUNT(*) AS "count!: i64" FROM jobs WHERE worker_id=?1 GROUP BY status"#,
        worker,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(|r| (r.status, r.count)).collect())
}

pub async fn list_workers(pool: &SqlitePool) -> AppResult<Vec<Worker>> {
    let workers = sqlx::query_as!(
        WorkerRow,
        r#"SELECT id AS "id!: Uuid", name, last_seen AS "last_seen: DateTime<Utc>", version, remote_addr, telemetry_json, max_concurrency AS "max_concurrency: i32", schedule_json FROM workers ORDER BY name LIMIT 200"#,
    )
    .fetch_all(pool)
    .await?;
    Ok(workers.into_iter().map(WorkerRow::into_worker).collect())
}

pub async fn delete_worker(pool: &SqlitePool, id: Uuid) -> AppResult<()> {
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    sqlx::query("DELETE FROM library_folders WHERE worker_id=?1")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    let affected = sqlx::query!("DELETE FROM workers WHERE id=?1", id)
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
    insert_job_with_priority(
        pool,
        source_name,
        target_worker_id,
        source_sha256,
        source_bytes,
        library_id,
        explicit_flow_id,
        0,
    )
    .await
}

/// Variante avec priorité explicite (`JOB_PRIORITY_MIN..=JOB_PRIORITY_MAX`).
#[allow(clippy::too_many_arguments)]
pub async fn insert_job_with_priority(
    pool: &SqlitePool,
    source_name: &str,
    target_worker_id: Option<Uuid>,
    source_sha256: &str,
    source_bytes: i64,
    library_id: Option<Uuid>,
    explicit_flow_id: Option<&str>,
    priority: i32,
) -> AppResult<Job> {
    let mut conn = pool.acquire().await?;
    create_job(
        &mut conn,
        source_name,
        target_worker_id,
        source_sha256,
        source_bytes,
        library_id,
        explicit_flow_id,
        priority,
        None,
    )
    .await
}

/// Racine de la source, indépendamment du dossier de travail éphémère.
#[derive(sqlx::FromRow)]
pub struct JobSource {
    pub source_key: Option<String>,
    pub source_root: String,
    pub source_worker_id: Option<Uuid>,
    pub source_writable: bool,
    pub source_mtime_ns: Option<i64>,
}

pub async fn job_source(pool: &SqlitePool, id: Uuid) -> AppResult<Option<JobSource>> {
    Ok(sqlx::query_as(
        "SELECT source_root, source_worker_id, source_writable, source_mtime_ns, source_key FROM jobs WHERE id=? AND source_root IS NOT NULL",
    ).bind(id).fetch_optional(pool).await?)
}

pub async fn known_library_file_signatures(
    pool: &SqlitePool,
    library: Uuid,
    source: &JobSource,
) -> AppResult<std::collections::HashMap<String, (u64, i64)>> {
    let rows: Vec<(String, i64, i64)> = sqlx::query_as(
        "SELECT source_name, source_bytes, source_mtime_ns FROM jobs
         WHERE library_id=? AND source_root=? AND source_worker_id IS ?
           AND source_mtime_ns IS NOT NULL AND source_bytes IS NOT NULL AND source_key IS ?
         ORDER BY created_at ASC, id ASC",
    )
    .bind(library)
    .bind(&source.source_root)
    .bind(source.source_worker_id)
    .bind(&source.source_key)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .filter_map(|(name, bytes, modified)| {
            u64::try_from(bytes)
                .ok()
                .map(|size| (name, (size, modified)))
        })
        .collect())
}

#[allow(clippy::too_many_arguments)]
pub async fn enqueue_library_source(
    pool: &SqlitePool,
    library_id: Uuid,
    source: &JobSource,
    name: &str,
    hash: &str,
    bytes: i64,
    worker: Option<Uuid>,
    flow_id: Option<&str>,
    dry_run: bool,
) -> AppResult<Option<Job>> {
    let mut conn = pool.begin_with("BEGIN IMMEDIATE").await?;
    let existing: Option<(Uuid, bool)> = sqlx::query_as(
        "SELECT id, coalesce(source_sha256=?5, 0) FROM jobs WHERE library_id=?1 AND source_name=?4
         AND ((?6 IS NOT NULL AND source_key=?6) OR (source_root=?2 AND source_worker_id IS ?3))
         AND (source_sha256=?5 OR (status='succeeded' AND source_writable=1 AND output_sha256=?5))
         ORDER BY created_at DESC, id DESC LIMIT 1",
    )
    .bind(library_id)
    .bind(&source.source_root)
    .bind(source.source_worker_id)
    .bind(name)
    .bind(hash)
    .bind(&source.source_key)
    .fetch_optional(&mut *conn)
    .await?;
    if let Some((id, exact_source)) = existing {
        if !dry_run && source.source_key.is_some() {
            sqlx::query("UPDATE jobs SET source_key=? WHERE id=? AND source_key IS NULL")
                .bind(&source.source_key)
                .bind(id)
                .execute(&mut *conn)
                .await?;
            if exact_source {
                record_source_mount(&mut conn, id, source).await?;
            }
            conn.commit().await?;
        }
        return Ok(None);
    }
    let job = create_job(
        &mut conn,
        name,
        worker,
        hash,
        bytes,
        Some(library_id),
        flow_id,
        0,
        Some(source),
    )
    .await?;
    if !dry_run {
        conn.commit().await?;
    }
    Ok(Some(job))
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn create_job(
    conn: &mut sqlx::SqliteConnection,
    source_name: &str,
    target_worker_id: Option<Uuid>,
    source_sha256: &str,
    source_bytes: i64,
    library_id: Option<Uuid>,
    explicit_flow_id: Option<&str>,
    priority: i32,
    source: Option<&JobSource>,
) -> AppResult<Job> {
    let clamped_priority = priority.clamp(JOB_PRIORITY_MIN, JOB_PRIORITY_MAX);
    let mut chosen_flow_id: Option<String> = explicit_flow_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned);

    if let Some(lib_id) = library_id {
        let row = sqlx::query!("SELECT flow_id FROM libraries WHERE id=?1", lib_id)
            .fetch_optional(&mut *conn)
            .await?;
        let Some(row) = row else {
            return Err(AppError::Invalid);
        };
        if chosen_flow_id.is_none() {
            chosen_flow_id = row.flow_id;
        }
    }

    let resolved_flow: Option<(String, i64)> = if let Some(flow_id) = chosen_flow_id.as_deref() {
        let found = sqlx::query!(
            r#"SELECT id AS "id!: String", version FROM flows WHERE id=?1"#,
            flow_id
        )
        .fetch_optional(&mut *conn)
        .await?;
        let Some(found) = found else {
            return Err(AppError::Invalid);
        };
        Some((found.id, found.version))
    } else {
        default_flow_with(&mut *conn)
            .await?
            .map(|flow| (flow.id, flow.version))
    };

    let (flow_id, flow_version) = match resolved_flow {
        Some((id, version)) => (Some(id), Some(version)),
        None => (None, None),
    };

    let job_id = Uuid::new_v4();
    let job = sqlx::query_as!(
        Job,
        r#"INSERT INTO jobs(id, source_name, target_worker_id, source_sha256, source_bytes, library_id, flow_id, flow_version, priority) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9) RETURNING id AS "id!: Uuid", source_name, source_sha256, source_bytes, output_sha256, output_bytes, warning AS "warning: bool", target_worker_id AS "target_worker_id: Uuid", worker_id AS "worker_id: Uuid", status AS "status: JobStatus", attempts AS "attempts: i32", lease_epoch, lease_token AS "lease_token: Uuid", leased_until AS "leased_until: DateTime<Utc>", output_name, error_code AS "error_code: JobErrorCode", library_id AS "library_id: Uuid", flow_id, flow_version, priority AS "priority: i32", created_at AS "created_at: DateTime<Utc>", updated_at AS "updated_at: DateTime<Utc>""#,
        job_id,
        source_name,
        target_worker_id,
        source_sha256,
        source_bytes,
        library_id,
        flow_id,
        flow_version,
        clamped_priority,
    )
    .fetch_one(&mut *conn)
    .await?;
    if let Some(source) = source {
        sqlx::query(
            "UPDATE jobs SET source_root=?, source_worker_id=?, source_writable=?, source_mtime_ns=?, source_key=? WHERE id=?",
        )
        .bind(&source.source_root)
        .bind(source.source_worker_id)
        .bind(source.source_writable)
        .bind(source.source_mtime_ns)
        .bind(&source.source_key)
        .bind(job.id)
        .execute(&mut *conn)
        .await?;
        if source.source_key.is_some() {
            record_source_mount(conn, job.id, source).await?;
        }
    }
    Ok(job)
}

async fn record_source_mount(
    conn: &mut sqlx::SqliteConnection,
    job: Uuid,
    source: &JobSource,
) -> AppResult<()> {
    sqlx::query("INSERT OR REPLACE INTO job_source_mounts (job_id, worker_id, local_path, writable) VALUES (?,?,?,?)")
        .bind(job).bind(source.source_worker_id.unwrap_or_default()).bind(&source.source_root)
        .bind(source.source_writable).execute(conn).await?;
    Ok(())
}

pub async fn get_job(pool: &SqlitePool, id: Uuid) -> AppResult<Job> {
    sqlx::query_as!(
        Job,
        r#"SELECT id AS "id!: Uuid", source_name, source_sha256, source_bytes, output_sha256, output_bytes, warning AS "warning: bool", target_worker_id AS "target_worker_id: Uuid", worker_id AS "worker_id: Uuid", status AS "status: JobStatus", attempts AS "attempts: i32", lease_epoch, lease_token AS "lease_token: Uuid", leased_until AS "leased_until: DateTime<Utc>", output_name, error_code AS "error_code: JobErrorCode", library_id AS "library_id: Uuid", flow_id, flow_version, priority AS "priority: i32", created_at AS "created_at: DateTime<Utc>", updated_at AS "updated_at: DateTime<Utc>" FROM jobs WHERE id=?1"#,
        id,
    )
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
    let found = sqlx::query_scalar!(
        r#"SELECT EXISTS(SELECT 1 FROM jobs WHERE library_id=?1 AND source_name=?2 AND source_sha256=?3 AND status IN ('queued', 'running', 'succeeded')) AS "exists!: bool""#,
        library_id,
        source_name,
        source_sha256,
    )
    .fetch_one(pool)
    .await?;
    Ok(found)
}

/// Supprime ou annule une tâche qui n'est pas en cours d'exécution (`running`).
pub async fn delete_job(pool: &SqlitePool, id: Uuid) -> AppResult<()> {
    let status: Option<String> = sqlx::query_scalar!("SELECT status FROM jobs WHERE id=?1", id)
        .fetch_optional(pool)
        .await?;
    let Some(status) = status else {
        return Err(AppError::NotFound);
    };
    if status == "running" {
        return Err(AppError::Conflict);
    }
    sqlx::query!("DELETE FROM jobs WHERE id=?1 AND status != 'running'", id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn list_jobs(pool: &SqlitePool, before: Option<Uuid>, limit: i64) -> AppResult<Vec<Job>> {
    let jobs = sqlx::query_as!(
        Job,
        r#"SELECT id AS "id!: Uuid", source_name, source_sha256, source_bytes, output_sha256, output_bytes, warning AS "warning: bool", target_worker_id AS "target_worker_id: Uuid", worker_id AS "worker_id: Uuid", status AS "status: JobStatus", attempts AS "attempts: i32", lease_epoch, lease_token AS "lease_token: Uuid", leased_until AS "leased_until: DateTime<Utc>", output_name, error_code AS "error_code: JobErrorCode", library_id AS "library_id: Uuid", flow_id, flow_version, priority AS "priority: i32", created_at AS "created_at: DateTime<Utc>", updated_at AS "updated_at: DateTime<Utc>" FROM jobs WHERE (?1 IS NULL OR (created_at, id) < (SELECT created_at, id FROM jobs WHERE id=?1)) ORDER BY created_at DESC, id DESC LIMIT ?2"#,
        before,
        limit,
    )
    .fetch_all(pool)
    .await?;
    Ok(jobs)
}

pub async fn claim(pool: &SqlitePool, worker: Uuid) -> AppResult<Option<Claim>> {
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    let worker_cfg = sqlx::query!(
        r#"UPDATE workers SET last_seen=datetime('now') WHERE id=?1 RETURNING max_concurrency AS "max_concurrency: i32", schedule_json"#,
        worker,
    )
    .fetch_optional(&mut *tx)
    .await?;
    let Some(worker_cfg) = worker_cfg else {
        return Err(AppError::NotFound);
    };
    let max_concurrency = worker_cfg.max_concurrency;
    let schedule_json = worker_cfg.schedule_json;
    // Bail expiré : échec explicite, jamais de remise en file silencieuse (relance : `retry_job`).
    let failed_status = JobStatus::Failed;
    let lease_expired_code = JobErrorCode::LeaseExpired;
    let running_status = JobStatus::Running;
    sqlx::query!(
        "UPDATE jobs \
         SET status=?1, error_code=?2, lease_token=NULL, lease_epoch=lease_epoch+1, \
             leased_until=NULL, updated_at=datetime('now') \
         WHERE status=?3 AND leased_until <= datetime('now')",
        failed_status,
        lease_expired_code,
        running_status,
    )
    .execute(&mut *tx)
    .await?;

    if let Some(sched) = schedule_json
        .as_deref()
        .and_then(|raw| serde_json::from_str::<WorkerSchedule>(raw).ok())
    {
        let now_local = chrono::Local::now();
        let weekday = now_local.weekday().number_from_monday() as u8;
        let hour = now_local.hour() as u8;
        if !sched.allows_at(weekday, hour) {
            tx.commit().await?;
            return Ok(None);
        }
    }

    let running_count: i64 = sqlx::query_scalar!(
        r#"SELECT COUNT(*) AS "count!: i64" FROM jobs WHERE worker_id=?1 AND status=?2"#,
        worker,
        running_status,
    )
    .fetch_one(&mut *tx)
    .await?;
    if running_count
        >= i64::from(max_concurrency.clamp(WORKER_MIN_CONCURRENCY, WORKER_MAX_CONCURRENCY))
    {
        tx.commit().await?;
        return Ok(None);
    }

    let queued_status = JobStatus::Queued;
    let next_id: Option<Uuid> = sqlx::query_scalar(
        "SELECT id FROM jobs WHERE status=?1 AND attempts < ?2
         AND (target_worker_id IS NULL OR target_worker_id=?3)
         AND ((source_key IS NULL AND (source_worker_id IS NULL OR source_worker_id=?3)) OR EXISTS (
             SELECT 1 FROM verified_job_source_mounts m WHERE m.job_id=jobs.id
                 AND (m.worker_id=?3 OR m.worker_id=zeroblob(16))
         ))
         ORDER BY priority DESC, created_at ASC, id ASC LIMIT 1",
    )
    .bind(queued_status)
    .bind(MAX_ATTEMPTS)
    .bind(worker)
    .fetch_optional(&mut *tx)
    .await?;
    let job: Option<Job> = if let Some(id) = next_id {
        let lease_token = Uuid::new_v4();
        if let Some((path, owner, read_write)) = sqlx::query_as::<_, (String, Uuid, bool)>(
            "SELECT local_path, worker_id, writable FROM verified_job_source_mounts
             WHERE job_id=?1 AND (worker_id=?2 OR worker_id=zeroblob(16))
             ORDER BY worker_id=?2 DESC, local_path LIMIT 1",
        )
        .bind(id)
        .bind(worker)
        .fetch_optional(&mut *tx)
        .await?
        {
            let owner = (!owner.is_nil()).then_some(owner);
            sqlx::query("UPDATE jobs SET source_root=?1, source_worker_id=?2, source_writable=?3,
                   source_mtime_ns=CASE WHEN source_root=?1 AND source_worker_id IS ?2 THEN source_mtime_ns END WHERE id=?4")
                .bind(path).bind(owner).bind(read_write).bind(id).execute(&mut *tx).await?;
        }
        sqlx::query_as!(
            Job,
            r#"UPDATE jobs SET status=?2, worker_id=?3, lease_token=?4, lease_epoch=lease_epoch+1, leased_until=datetime('now', '+' || ?5 || ' seconds'), attempts=attempts+1, error_code=NULL, updated_at=datetime('now') WHERE id=?1 RETURNING id AS "id!: Uuid", source_name, source_sha256, source_bytes, output_sha256, output_bytes, warning AS "warning: bool", target_worker_id AS "target_worker_id: Uuid", worker_id AS "worker_id: Uuid", status AS "status: JobStatus", attempts AS "attempts: i32", lease_epoch, lease_token AS "lease_token: Uuid", leased_until AS "leased_until: DateTime<Utc>", output_name, error_code AS "error_code: JobErrorCode", library_id AS "library_id: Uuid", flow_id, flow_version, priority AS "priority: i32", created_at AS "created_at: DateTime<Utc>", updated_at AS "updated_at: DateTime<Utc>""#,
            id,
            running_status,
            worker,
            lease_token,
            LEASE_SECONDS,
        )
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
            let local_source_root = job_source(pool, j.id).await?.and_then(|source| {
                (source.source_worker_id == Some(worker)).then_some(source.source_root)
            });
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
                local_source_root,
            }))
        }
    }
}

/// Relance manuelle d'une tâche échouée après perte de bail, dans la limite de `MAX_ATTEMPTS`.
pub async fn retry_job(pool: &SqlitePool, id: Uuid) -> AppResult<Job> {
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    let exists = sqlx::query_scalar!(
        r#"SELECT EXISTS(SELECT 1 FROM jobs WHERE id=?1) AS "exists!: bool""#,
        id,
    )
    .fetch_one(&mut *tx)
    .await?;
    if !exists {
        return Err(AppError::NotFound);
    }
    let queued_status = JobStatus::Queued;
    let failed_status = JobStatus::Failed;
    let lease_expired_code = JobErrorCode::LeaseExpired;
    let job = sqlx::query_as!(
        Job,
        r#"UPDATE jobs SET status=?2, worker_id=NULL, lease_token=NULL, lease_epoch=lease_epoch+1, leased_until=NULL, error_code=NULL, updated_at=datetime('now') WHERE id=?1 AND status=?3 AND error_code=?4 AND attempts < ?5 RETURNING id AS "id!: Uuid", source_name, source_sha256, source_bytes, output_sha256, output_bytes, warning AS "warning: bool", target_worker_id AS "target_worker_id: Uuid", worker_id AS "worker_id: Uuid", status AS "status: JobStatus", attempts AS "attempts: i32", lease_epoch, lease_token AS "lease_token: Uuid", leased_until AS "leased_until: DateTime<Utc>", output_name, error_code AS "error_code: JobErrorCode", library_id AS "library_id: Uuid", flow_id, flow_version, priority AS "priority: i32", created_at AS "created_at: DateTime<Utc>", updated_at AS "updated_at: DateTime<Utc>""#,
        id,
        queued_status,
        failed_status,
        lease_expired_code,
        MAX_ATTEMPTS,
    )
    .fetch_optional(&mut *tx)
    .await?;
    let job = job.ok_or(AppError::Conflict)?;
    tx.commit().await?;
    Ok(job)
}

pub async fn active(pool: &SqlitePool, id: Uuid, lease: &LeaseRequest) -> AppResult<Job> {
    let running_status = JobStatus::Running;
    sqlx::query_as!(
        Job,
        r#"SELECT id AS "id!: Uuid", source_name, source_sha256, source_bytes, output_sha256, output_bytes, warning AS "warning: bool", target_worker_id AS "target_worker_id: Uuid", worker_id AS "worker_id: Uuid", status AS "status: JobStatus", attempts AS "attempts: i32", lease_epoch, lease_token AS "lease_token: Uuid", leased_until AS "leased_until: DateTime<Utc>", output_name, error_code AS "error_code: JobErrorCode", library_id AS "library_id: Uuid", flow_id, flow_version, priority AS "priority: i32", created_at AS "created_at: DateTime<Utc>", updated_at AS "updated_at: DateTime<Utc>" FROM jobs WHERE id=?1 AND worker_id=?2 AND lease_token=?3 AND lease_epoch=?4 AND status=?5 AND leased_until > datetime('now')"#,
        id,
        lease.worker_id,
        lease.lease_token,
        lease.lease_epoch,
        running_status,
    )
    .fetch_optional(pool)
    .await?
    .ok_or(AppError::Conflict)
}

pub async fn heartbeat(pool: &SqlitePool, id: Uuid, lease: &LeaseRequest) -> AppResult<()> {
    let running_status = JobStatus::Running;
    let count = sqlx::query!(
        "UPDATE jobs \
         SET leased_until=datetime('now', '+' || ?5 || ' seconds'), updated_at=datetime('now') \
         WHERE id=?1 AND worker_id=?2 AND lease_token=?3 AND lease_epoch=?4 \
           AND status=?6 AND leased_until > datetime('now')",
        id,
        lease.worker_id,
        lease.lease_token,
        lease.lease_epoch,
        LEASE_SECONDS,
        running_status,
    )
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
    let failed_status = JobStatus::Failed;
    let running_status = JobStatus::Running;
    let count = sqlx::query!(
        "UPDATE jobs \
         SET status=?5, error_code=?6, lease_token=NULL, lease_epoch=lease_epoch+1, \
             leased_until=NULL, updated_at=datetime('now') \
         WHERE id=?1 AND worker_id=?2 AND lease_token=?3 AND lease_epoch=?4 \
           AND status=?7 AND leased_until > datetime('now')",
        id,
        lease.worker_id,
        lease.lease_token,
        lease.lease_epoch,
        failed_status,
        code,
        running_status,
    )
    .execute(pool)
    .await?
    .rows_affected();
    if count != 1 {
        return Err(AppError::Conflict);
    }
    Ok(())
}

pub async fn succeeded_output_name(pool: &SqlitePool, id: Uuid) -> AppResult<Option<String>> {
    let succeeded_status = JobStatus::Succeeded;
    let name: Option<Option<String>> = sqlx::query_scalar!(
        "SELECT output_name FROM jobs WHERE id=?1 AND status=?2",
        id,
        succeeded_status,
    )
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
    let running_status = JobStatus::Running;
    let active: Option<Uuid> = sqlx::query_scalar!(
        r#"SELECT id AS "id!: Uuid" FROM jobs WHERE id=?1 AND worker_id=?2 AND lease_token=?3 AND lease_epoch=?4 AND status=?5 AND leased_until > datetime('now')"#,
        id,
        lease.worker_id,
        lease.lease_token,
        lease.lease_epoch,
        running_status,
    )
    .fetch_optional(&mut *tx)
    .await?;
    if active.is_none() {
        return Err(AppError::Conflict);
    }
    link_output.await?;
    let succeeded_status = JobStatus::Succeeded;
    sqlx::query!(
        "UPDATE jobs \
         SET status=?2, output_name=?3, output_sha256=?4, output_bytes=?5, warning=?6, \
             lease_token=NULL, lease_epoch=lease_epoch+1, leased_until=NULL, error_code=NULL, \
             updated_at=datetime('now') \
         WHERE id=?1",
        id,
        succeeded_status,
        output.name,
        output.sha256,
        output.bytes,
        output.warning,
    )
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
    let exists = sqlx::query_scalar!(
        r#"SELECT EXISTS(SELECT 1 FROM jobs WHERE id=?1) AS "exists!: bool""#,
        job_id,
    )
    .fetch_one(&mut *tx)
    .await?;
    if !exists {
        return Err(AppError::NotFound);
    }
    sqlx::query!("DELETE FROM job_steps WHERE job_id=?1", job_id)
        .execute(&mut *tx)
        .await?;
    for step in steps {
        sqlx::query!(
            "INSERT INTO job_steps (job_id, step_index, node_id, block_kind, operation_id, \
                                    status, message, output_excerpt) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            job_id,
            step.step_index,
            step.node_id,
            step.block_kind,
            step.operation_id,
            step.status,
            step.message,
            step.output_excerpt,
        )
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(())
}

/// Retourne le journal ordonné des étapes d'un job.
pub async fn list_job_steps(pool: &SqlitePool, job_id: Uuid) -> AppResult<Vec<JobStepLog>> {
    let exists = sqlx::query_scalar!(
        r#"SELECT EXISTS(SELECT 1 FROM jobs WHERE id=?1) AS "exists!: bool""#,
        job_id,
    )
    .fetch_one(pool)
    .await?;
    if !exists {
        return Err(AppError::NotFound);
    }
    Ok(sqlx::query_as!(
        JobStepLog,
        r#"SELECT id AS "id!: i64", job_id AS "job_id!: Uuid", step_index AS "step_index: i32", node_id, block_kind, operation_id, status AS "status: JobStepStatus", message, output_excerpt, created_at AS "created_at: DateTime<Utc>" FROM job_steps WHERE job_id=?1 ORDER BY step_index, id"#,
        job_id,
    )
    .fetch_all(pool)
    .await?)
}

/// Efface la référence au fichier de sortie d'un job non en cours et retourne son ancien nom pour suppression explicite.
pub async fn clear_job_output_name(pool: &SqlitePool, id: Uuid) -> AppResult<Option<String>> {
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    let row = sqlx::query!(
        r#"SELECT status AS "status: JobStatus", output_name FROM jobs WHERE id=?1"#,
        id,
    )
    .fetch_optional(&mut *tx)
    .await?;
    let Some(row) = row else {
        return Err(AppError::NotFound);
    };
    if row.status == JobStatus::Running {
        return Err(AppError::Conflict);
    }
    sqlx::query!(
        "UPDATE jobs SET output_name=NULL, output_sha256=NULL, output_bytes=NULL, \
         updated_at=datetime('now') WHERE id=?1",
        id,
    )
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(row.output_name)
}

/// Annule une tâche en attente (`queued`) ou en cours (`running`), en invalidant son bail.
pub async fn cancel_job(pool: &SqlitePool, id: Uuid) -> AppResult<Job> {
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    let exists = sqlx::query_scalar!(
        r#"SELECT EXISTS(SELECT 1 FROM jobs WHERE id=?1) AS "exists!: bool""#,
        id,
    )
    .fetch_one(&mut *tx)
    .await?;
    if !exists {
        return Err(AppError::NotFound);
    }
    let canceled_status = JobStatus::Canceled;
    let job = sqlx::query_as!(
        Job,
        r#"UPDATE jobs SET status=?2, lease_token=NULL, lease_epoch=lease_epoch+1, leased_until=NULL, error_code=NULL, updated_at=datetime('now') WHERE id=?1 AND status IN ('queued', 'running') RETURNING id AS "id!: Uuid", source_name, source_sha256, source_bytes, output_sha256, output_bytes, warning AS "warning: bool", target_worker_id AS "target_worker_id: Uuid", worker_id AS "worker_id: Uuid", status AS "status: JobStatus", attempts AS "attempts: i32", lease_epoch, lease_token AS "lease_token: Uuid", leased_until AS "leased_until: DateTime<Utc>", output_name, error_code AS "error_code: JobErrorCode", library_id AS "library_id: Uuid", flow_id, flow_version, priority AS "priority: i32", created_at AS "created_at: DateTime<Utc>", updated_at AS "updated_at: DateTime<Utc>""#,
        id,
        canceled_status,
    )
    .fetch_optional(&mut *tx)
    .await?;
    let job = job.ok_or(AppError::Conflict)?;
    tx.commit().await?;
    Ok(job)
}

/// Met à jour la priorité d'une tâche encore en attente (`queued`).
pub async fn update_job_priority(pool: &SqlitePool, id: Uuid, priority: i32) -> AppResult<Job> {
    if !(JOB_PRIORITY_MIN..=JOB_PRIORITY_MAX).contains(&priority) {
        return Err(AppError::Invalid);
    }
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    let exists = sqlx::query_scalar!(
        r#"SELECT EXISTS(SELECT 1 FROM jobs WHERE id=?1) AS "exists!: bool""#,
        id,
    )
    .fetch_one(&mut *tx)
    .await?;
    if !exists {
        return Err(AppError::NotFound);
    }
    let job = sqlx::query_as!(
        Job,
        r#"UPDATE jobs SET priority=?2, updated_at=datetime('now') WHERE id=?1 AND status='queued' RETURNING id AS "id!: Uuid", source_name, source_sha256, source_bytes, output_sha256, output_bytes, warning AS "warning: bool", target_worker_id AS "target_worker_id: Uuid", worker_id AS "worker_id: Uuid", status AS "status: JobStatus", attempts AS "attempts: i32", lease_epoch, lease_token AS "lease_token: Uuid", leased_until AS "leased_until: DateTime<Utc>", output_name, error_code AS "error_code: JobErrorCode", library_id AS "library_id: Uuid", flow_id, flow_version, priority AS "priority: i32", created_at AS "created_at: DateTime<Utc>", updated_at AS "updated_at: DateTime<Utc>""#,
        id,
        priority,
    )
    .fetch_optional(&mut *tx)
    .await?;
    let job = job.ok_or(AppError::Conflict)?;
    tx.commit().await?;
    Ok(job)
}

/// Applique une action par lot (`retry`, `cancel`, `delete`) à une sélection bornée de tâches.
pub async fn batch_jobs(
    pool: &SqlitePool,
    ids: &[Uuid],
    action: JobBatchAction,
) -> AppResult<(JobBatchResult, Vec<(JobEventType, Uuid, JobStatus)>)> {
    if ids.is_empty() || ids.len() > MAX_BATCH_IDS {
        return Err(AppError::Invalid);
    }
    let mut seen = std::collections::HashSet::with_capacity(ids.len());
    let mut updated = 0_usize;
    let mut skipped = 0_usize;
    let mut events = Vec::new();
    for &id in ids {
        if !seen.insert(id) {
            continue;
        }
        let outcome = match action {
            JobBatchAction::Retry => retry_job(pool, id)
                .await
                .map(|j| Some((JobEventType::JobRetried, j.id, j.status))),
            JobBatchAction::Cancel => cancel_job(pool, id)
                .await
                .map(|j| Some((JobEventType::JobCanceled, j.id, j.status))),
            JobBatchAction::Delete => delete_job(pool, id).await.map(|()| None),
        };
        match outcome {
            Ok(Some(ev)) => {
                updated += 1;
                events.push(ev);
            }
            Ok(None) => {
                updated += 1;
            }
            Err(AppError::NotFound | AppError::Conflict) => {
                skipped += 1;
            }
            Err(err) => return Err(err),
        }
    }
    Ok((JobBatchResult { updated, skipped }, events))
}

/// Agrège les statistiques globales et les économies d'espace disque par bibliothèque.
pub async fn job_storage_stats(pool: &SqlitePool) -> AppResult<JobStorageStats> {
    let totals = sqlx::query!(
        r#"SELECT COUNT(*) AS "total_jobs!: i64", COALESCE(SUM(CASE WHEN status = 'succeeded' THEN 1 ELSE 0 END), 0) AS "succeeded_jobs!: i64", COALESCE(SUM(CASE WHEN status = 'failed' THEN 1 ELSE 0 END), 0) AS "failed_jobs!: i64", COALESCE(SUM(CASE WHEN status = 'canceled' THEN 1 ELSE 0 END), 0) AS "canceled_jobs!: i64", COALESCE(SUM(CASE WHEN status = 'succeeded' AND source_bytes IS NOT NULL AND output_bytes IS NOT NULL THEN source_bytes ELSE 0 END), 0) AS "total_source_bytes!: i64", COALESCE(SUM(CASE WHEN status = 'succeeded' AND source_bytes IS NOT NULL AND output_bytes IS NOT NULL THEN output_bytes ELSE 0 END), 0) AS "total_output_bytes!: i64" FROM jobs"#,
    )
    .fetch_one(pool)
    .await?;

    let rows = sqlx::query!(
        r#"SELECT j.library_id AS "library_id: Uuid", COALESCE(l.name, 'Sans bibliothèque') AS "library_name!: String", COALESCE(SUM(CASE WHEN j.status = 'succeeded' THEN 1 ELSE 0 END), 0) AS "succeeded_count!: i64", COALESCE(SUM(CASE WHEN j.status = 'failed' THEN 1 ELSE 0 END), 0) AS "failed_count!: i64", COALESCE(SUM(CASE WHEN j.status = 'succeeded' AND j.source_bytes IS NOT NULL AND j.output_bytes IS NOT NULL THEN j.source_bytes ELSE 0 END), 0) AS "source_bytes!: i64", COALESCE(SUM(CASE WHEN j.status = 'succeeded' AND j.source_bytes IS NOT NULL AND j.output_bytes IS NOT NULL THEN j.output_bytes ELSE 0 END), 0) AS "output_bytes!: i64" FROM jobs j LEFT JOIN libraries l ON l.id = j.library_id GROUP BY j.library_id, COALESCE(l.name, 'Sans bibliothèque') ORDER BY (COALESCE(SUM(CASE WHEN j.status = 'succeeded' AND j.source_bytes IS NOT NULL AND j.output_bytes IS NOT NULL THEN j.source_bytes ELSE 0 END), 0) - COALESCE(SUM(CASE WHEN j.status = 'succeeded' AND j.source_bytes IS NOT NULL AND j.output_bytes IS NOT NULL THEN j.output_bytes ELSE 0 END), 0)) DESC, 2 ASC"#,
    )
    .fetch_all(pool)
    .await?;

    let by_library = rows
        .into_iter()
        .map(|row| LibraryJobStats {
            library_id: row.library_id,
            library_name: row.library_name,
            succeeded_count: row.succeeded_count,
            failed_count: row.failed_count,
            source_bytes: row.source_bytes,
            output_bytes: row.output_bytes,
            saved_bytes: row.source_bytes - row.output_bytes,
        })
        .collect();

    let active_jobs =
        sqlx::query_scalar("SELECT count(*) FROM jobs WHERE status IN ('queued','running')")
            .fetch_one(pool)
            .await?;
    let by_volume = sqlx::query_as::<_, (Option<Uuid>, String, String, i64, i64, i64)>(
        "SELECT library_id, coalesce(source_key, source_root, ''),
            CASE WHEN source_key IS NOT NULL THEN 'source:'||source_key
                 WHEN source_root IS NOT NULL THEN 'mount:'||coalesce(hex(source_worker_id),'')||':'||source_root ELSE '' END,
             sum(source_bytes-output_bytes), sum(max(output_bytes-source_bytes, 0)), sum(output_bytes > source_bytes)
         FROM jobs WHERE status='succeeded' AND source_bytes IS NOT NULL AND output_bytes IS NOT NULL
         GROUP BY library_id, 3 ORDER BY 4 DESC, 2 ASC",
    ).fetch_all(pool).await?.into_iter().map(|(library_id, volume, key, saved_bytes, grown_bytes, grown_count)| {
        forgearr_shared::jobs::VolumeJobStats { library_id, volume, key, saved_bytes, grown_bytes, grown_count }
    }).collect();

    Ok(JobStorageStats {
        total_jobs: totals.total_jobs,
        succeeded_jobs: totals.succeeded_jobs,
        failed_jobs: totals.failed_jobs,
        canceled_jobs: totals.canceled_jobs,
        total_source_bytes: totals.total_source_bytes,
        total_output_bytes: totals.total_output_bytes,
        saved_bytes: totals.total_source_bytes - totals.total_output_bytes,
        by_library,
        by_volume,
        active_jobs,
    })
}

pub async fn filtered_jobs(
    pool: &SqlitePool,
    before: Option<Uuid>,
    limit: i64,
    filter: &forgearr_shared::jobs::JobListFilter,
) -> AppResult<Vec<Job>> {
    use forgearr_shared::jobs::JobListState;
    let (first, second) = match filter.state {
        None => (None, None),
        Some(JobListState::Active) => (Some(JobStatus::Queued), Some(JobStatus::Running)),
        Some(JobListState::Completed) => (Some(JobStatus::Succeeded), Some(JobStatus::Canceled)),
        Some(JobListState::Succeeded) => (Some(JobStatus::Succeeded), None),
        Some(JobListState::Failed) => (Some(JobStatus::Failed), None),
    };
    Ok(sqlx::query_as::<_, Job>(
        "SELECT * FROM jobs WHERE (?1 IS NULL OR (created_at,id) < (SELECT created_at,id FROM jobs WHERE id=?1))
           AND (?2 IS NULL OR status IN (?2, ?3))
           AND (?4 IS NULL OR library_id=?4)
           AND (?5 IS NULL OR (CASE WHEN source_key IS NOT NULL THEN 'source:'||source_key WHEN source_root IS NOT NULL THEN 'mount:'||coalesce(hex(source_worker_id),'')||':'||source_root ELSE '' END)=?5)
           AND (?6 IS NULL OR error_code=?6)
         ORDER BY created_at DESC,id DESC LIMIT ?7",
    ).bind(before).bind(first).bind(second).bind(filter.library).bind(&filter.volume).bind(filter.error).bind(limit)
        .fetch_all(pool).await?)
}
