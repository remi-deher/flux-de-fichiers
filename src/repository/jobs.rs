use super::{
    flow_usage::{TaskFlow, held_json, not_held_sql},
    flows::default_flow_with,
};
use crate::{
    domain::{
        Claim, JOB_PRIORITY_MAX, JOB_PRIORITY_MIN, Job, JobBatchAction, JobBatchResult,
        JobErrorCode, JobEventType, JobStatus, JobStepEntry, JobStepLog, JobStepStatus,
        JobStorageStats, LeaseRequest, LeaseTerms, LibraryJobStats, MAX_ATTEMPTS,
        WORKER_MAX_CONCURRENCY, WORKER_MIN_CONCURRENCY, Worker, WorkerSchedule,
    },
    error::{AppError, AppResult},
    telemetry::TelemetrySnapshot,
};
use chrono::{DateTime, Utc};
use forgearr_shared::jobs::{LIBRARY_DEFAULT_JOB_PRIORITY, LeaseProtocol};
use forgearr_shared::workers::{WorkerTelemetry, worker_slots};
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
            max_cpu_jobs: None,
            max_gpu_jobs: None,
            schedule,
            transfer_enabled: false,
        }
    }
}

/// Worker lu par macro, complété des colonnes lues en runtime (transfert, places par classe).
async fn worker_with_transfer(pool: &SqlitePool, row: WorkerRow) -> AppResult<Worker> {
    super::resource_slots::with_extras(pool, row.into_worker()).await
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
    worker_with_transfer(pool, row).await
}

/// Met à jour la concurrence maximale, les places CPU/GPU et la fenêtre horaire d'un worker.
///
/// `max_cpu_jobs` et `max_gpu_jobs` à `None` gardent le mode historique (voir `WorkerSlots`).
pub async fn update_worker_settings(
    pool: &SqlitePool,
    id: Uuid,
    max_concurrency: i32,
    max_cpu_jobs: Option<i32>,
    max_gpu_jobs: Option<i32>,
    schedule: Option<&WorkerSchedule>,
) -> AppResult<Worker> {
    if !forgearr_shared::workers::class_slots_valid(max_cpu_jobs, max_gpu_jobs) {
        return Err(AppError::Invalid);
    }
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
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    let row = sqlx::query_as!(
        WorkerRow,
        r#"UPDATE workers SET max_concurrency = ?2, schedule_json = ?3 WHERE id = ?1 RETURNING id AS "id!: Uuid", name, last_seen AS "last_seen: DateTime<Utc>", version, remote_addr, telemetry_json, max_concurrency AS "max_concurrency: i32", schedule_json"#,
        id,
        max_concurrency,
        schedule_json,
    )
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(AppError::NotFound)?;
    super::resource_slots::save_class_slots(&mut tx, id, max_cpu_jobs, max_gpu_jobs).await?;
    tx.commit().await?;
    worker_with_transfer(pool, row).await
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
    worker_with_transfer(pool, row).await
}

pub async fn find_worker_by_name(pool: &SqlitePool, name: &str) -> AppResult<Option<Worker>> {
    let row = sqlx::query_as!(
        WorkerRow,
        r#"SELECT id AS "id!: Uuid", name, last_seen AS "last_seen: DateTime<Utc>", version, remote_addr, telemetry_json, max_concurrency AS "max_concurrency: i32", schedule_json FROM workers WHERE name=?1"#,
        name,
    )
    .fetch_optional(pool)
    .await?;
    match row {
        Some(row) => Ok(Some(worker_with_transfer(pool, row).await?)),
        None => Ok(None),
    }
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
    let mut extras = super::resource_slots::all_extras(pool).await?;
    Ok(workers
        .into_iter()
        .map(WorkerRow::into_worker)
        .map(|mut worker| {
            if let Some(extra) = extras.remove(&worker.id) {
                extra.apply(&mut worker);
            }
            worker
        })
        .collect())
}

pub async fn delete_worker(pool: &SqlitePool, id: Uuid) -> AppResult<()> {
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    sqlx::query("DELETE FROM library_transfer_workers WHERE worker_id=?1")
        .bind(id)
        .execute(&mut *tx)
        .await?;
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
        None,
    )
    .await
}

/// Variante avec priorité explicite (`JOB_PRIORITY_MIN..=JOB_PRIORITY_MAX`) ; `None` reprend la
/// priorité par défaut de la bibliothèque.
#[allow(clippy::too_many_arguments)]
pub async fn insert_job_with_priority(
    pool: &SqlitePool,
    source_name: &str,
    target_worker_id: Option<Uuid>,
    source_sha256: &str,
    source_bytes: i64,
    library_id: Option<Uuid>,
    explicit_flow_id: Option<&str>,
    priority: Option<i32>,
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
        None,
        Some(source),
    )
    .await?;
    if !dry_run {
        conn.commit().await?;
    }
    Ok(Some(job))
}

/// Crée une tâche en file. Sans `priority`, elle hérite de la priorité par défaut de sa
/// bibliothèque (sinon `LIBRARY_DEFAULT_JOB_PRIORITY`) ; sa classe de ressource (CPU/GPU) est
/// déduite de tous ses flux figés, sous-flux et flux d'échec compris (`refresh_job_class`).
#[allow(clippy::too_many_arguments)]
pub(super) async fn create_job(
    conn: &mut sqlx::SqliteConnection,
    source_name: &str,
    target_worker_id: Option<Uuid>,
    source_sha256: &str,
    source_bytes: i64,
    library_id: Option<Uuid>,
    explicit_flow_id: Option<&str>,
    priority: Option<i32>,
    source: Option<&JobSource>,
) -> AppResult<Job> {
    let mut library_priority: Option<i32> = None;
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
        library_priority =
            sqlx::query_scalar::<_, i32>("SELECT default_priority FROM libraries WHERE id=?1")
                .bind(lib_id)
                .fetch_optional(&mut *conn)
                .await?;
    }
    let clamped_priority = priority
        .or(library_priority)
        .unwrap_or(LIBRARY_DEFAULT_JOB_PRIORITY)
        .clamp(JOB_PRIORITY_MIN, JOB_PRIORITY_MAX);

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
    super::linked_flows::freeze_linked_flows(
        &mut *conn,
        job.id,
        job.flow_id.as_deref().zip(job.flow_version),
    )
    .await?;
    super::resource_slots::refresh_job_class(&mut *conn, job.id).await?;
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

/// Réserve la prochaine tâche du worker.
///
/// Ordre : fenêtre horaire du worker, puis places libres par classe de ressource (`WorkerSlots` :
/// `max_concurrency` commun en mode historique, sinon `max_cpu_jobs`/`max_gpu_jobs`), puis la
/// tâche admissible de plus haute priorité, la plus ancienne à priorité égale
/// (`priority DESC, created_at ASC, id ASC`). Les capacités d'encodage exigées par un relais
/// restent filtrées à part : les places ne font que limiter le nombre de tâches.
///
/// Réservation d'un worker antérieur à l'ADR 0027 (voir [`claim_with_protocol`]).
pub async fn claim(pool: &SqlitePool, worker: Uuid) -> AppResult<Option<Claim>> {
    claim_with_protocol(pool, worker, LeaseProtocol::Legacy).await
}

/// [`claim`] selon le protocole de bail annoncé. L'attribution est atomique (un seul écrivain, époque
/// et jeton neufs) ; elle n'expire aucun bail, c'est le rôle du balayage (`leases::sweep`).
pub async fn claim_with_protocol(
    pool: &SqlitePool,
    worker: Uuid,
    protocol: LeaseProtocol,
) -> AppResult<Option<Claim>> {
    claim_except(pool, worker, protocol, &[]).await
}

/// [`claim_with_protocol`], sauf les tâches qui se servent d'une version retenue : elles restent en file sans
/// y perdre de tentative.
pub async fn claim_except(
    pool: &SqlitePool,
    worker: Uuid,
    protocol: LeaseProtocol,
    held: &[TaskFlow],
) -> AppResult<Option<Claim>> {
    let held = held_json(held)?;
    let unreachable_timeout = crate::runtime_settings::load_unreachable_timeout(pool).await;
    let lease_seconds = super::leases::lease_window_seconds(protocol, unreachable_timeout);
    let timezone = crate::runtime_settings::load_timezone(pool).await;
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    let worker_cfg = sqlx::query_as::<_, (i32, Option<i32>, Option<i32>, Option<String>)>(
        "UPDATE workers SET last_seen=datetime('now') WHERE id=?1 RETURNING max_concurrency, max_cpu_jobs, max_gpu_jobs, schedule_json",
    )
    .bind(worker)
    .fetch_optional(&mut *tx)
    .await?;
    let Some((max_concurrency, max_cpu_jobs, max_gpu_jobs, schedule_json)) = worker_cfg else {
        return Err(AppError::NotFound);
    };
    let running_status = JobStatus::Running;
    if let Some(sched) = schedule_json
        .as_deref()
        .and_then(|raw| serde_json::from_str::<WorkerSchedule>(raw).ok())
    {
        let (weekday, hour) =
            forgearr_shared::timezone::local_weekday_and_hour(Utc::now(), &timezone);
        if !sched.allows_at(weekday, hour) {
            tx.commit().await?;
            return Ok(None);
        }
    }

    let running = super::resource_slots::running_jobs(&mut tx, worker).await?;
    let admitted =
        worker_slots(max_concurrency, max_cpu_jobs, max_gpu_jobs).admitted_classes(running);
    if admitted.is_empty() {
        tx.commit().await?;
        return Ok(None);
    }
    let admitted_json = serde_json::to_string(&admitted).map_err(|_| AppError::Internal)?;

    let queued_status = JobStatus::Queued;
    let online_seconds = forgearr_shared::workers::WORKER_ONLINE_SECONDS.to_string();
    let claimable_sql = format!(
        "SELECT id FROM jobs WHERE status=?1 AND attempts < ?2
         AND (target_worker_id IS NULL OR target_worker_id=?3)
         AND (NOT EXISTS (SELECT 1 FROM job_relays r WHERE r.job_id=jobs.id AND r.requirement_json IS NOT NULL)
              OR EXISTS (SELECT 1 FROM job_relays r, workers w, json_each(w.telemetry_json, '$.capabilities.hardwareEncoders') e
                         WHERE r.job_id=jobs.id AND w.id=?3 AND r.requirement_json IS NOT NULL
                           AND json_extract(e.value, '$.codec')=json_extract(r.requirement_json, '$.codec')
                           AND json_extract(e.value, '$.acceleration') != ?4
                           AND (json_extract(r.requirement_json, '$.acceleration') IS NULL
                                OR json_extract(e.value, '$.acceleration')=json_extract(r.requirement_json, '$.acceleration')))
              OR EXISTS (SELECT 1 FROM job_relays r, workers w, json_each(w.telemetry_json, '$.capabilities.softwareEncoders') e
                         WHERE r.job_id=jobs.id AND w.id=?3 AND r.requirement_json IS NOT NULL
                           AND e.value=json_extract(r.requirement_json, '$.codec')
                           AND (json_extract(r.requirement_json, '$.acceleration') IS NULL
                                OR json_extract(r.requirement_json, '$.acceleration')=?4)))
         AND resource_class IN (SELECT value FROM json_each(?5))
         AND {not_held}
         AND {access}
         AND ({readable} OR EXISTS (SELECT 1 FROM job_relays r WHERE r.job_id=jobs.id AND r.artifact_relay=1))
         ORDER BY priority DESC, created_at ASC, id ASC LIMIT 1",
        not_held = not_held_sql("?6"),
        access = super::library_access::job_access_sql("?3"),
        readable = super::library_access::source_readable_sql("?3", &online_seconds),
    );
    let next_id: Option<Uuid> = sqlx::query_scalar(&claimable_sql)
        .bind(queued_status)
        .bind(MAX_ATTEMPTS)
        .bind(worker)
        .bind(forgearr_shared::capabilities::EncodeAcceleration::Cpu.as_str())
        .bind(admitted_json)
        .bind(held)
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
            lease_seconds,
        )
        .fetch_optional(&mut *tx)
        .await?
    } else {
        None
    };
    if let Some(job) = &job {
        super::leases::start_lease(&mut tx, job.id, protocol).await?;
        super::failure_flows::reset_for_new_attempt(&mut tx, job.id).await?;
    }
    tx.commit().await?;
    let Some(job) = job else {
        return Ok(None);
    };
    let lease = LeaseRequest {
        worker_id: worker,
        lease_token: job.lease_token.ok_or(AppError::Internal)?,
        lease_epoch: job.lease_epoch,
        telemetry: None,
    };
    let job_id = job.id;
    match build_claim(pool, job, &lease, unreachable_timeout).await {
        Ok(claim) => Ok(Some(claim)),
        Err(error) => {
            // Réservée mais jamais remise : elle n'a pas d'exécutant.
            if let Err(undelivered) = super::undeliver_lease(pool, job_id, &lease).await {
                tracing::error!(%job_id, error=%undelivered, "bail non remis et non rendu");
            }
            Err(error)
        }
    }
}

/// Relit tout ce que le bail transmet à son titulaire, attribution déjà validée.
async fn build_claim(
    pool: &SqlitePool,
    job: Job,
    lease: &LeaseRequest,
    unreachable_timeout: i64,
) -> AppResult<Claim> {
    let worker = lease.worker_id;
    let frozen_flow = match (&job.flow_id, job.flow_version) {
        (Some(flow_id), Some(version)) => {
            Some(super::get_flow_version(pool, flow_id, version).await?)
        }
        _ => None,
    };
    let local_source_root = job_source(pool, job.id)
        .await?
        .and_then(|source| (source.source_worker_id == Some(worker)).then_some(source.source_root));
    let resume = super::relays::resume_state(pool, job.id).await?;
    let library_name = super::step_context::library_name(pool, job.library_id).await?;
    let worker_name = super::step_context::worker_name(pool, worker).await?;
    let handoff_input = super::relays::artifact_input(pool, job.id, worker).await?;
    Ok(Claim {
        job_id: job.id,
        lease_token: lease.lease_token,
        lease_epoch: lease.lease_epoch,
        source_name: job.source_name,
        source_sha256: job.source_sha256.ok_or(AppError::Internal)?,
        source_bytes: job.source_bytes.ok_or(AppError::Internal)?,
        library_id: job.library_id,
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
        resume_node_id: resume.node_id,
        resumed_outputs: resume.outputs,
        handoff_input,
        prior_warning: job.warning,
        source_ticket: None,
        library_name,
        worker_name,
        resume_variables: resume.variables,
        linked_flows: super::linked_flows::frozen_linked_flows(pool, job.id).await?,
        lease_terms: Some(LeaseTerms {
            unreachable_timeout_seconds: unreachable_timeout,
        }),
    })
}

/// Relance manuelle d'une tâche échouée après perte de bail. Tentatives épuisées, le compte repart
/// à zéro ; les actions irréversibles inscrites sont oubliées : l'administrateur assume la répétition.
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
    let unresolved: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM source_publication_journal WHERE job_id=?1)",
    )
    .bind(id)
    .fetch_one(&mut *tx)
    .await?;
    if unresolved {
        return Err(AppError::Conflict);
    }
    let queued_status = JobStatus::Queued;
    let failed_status = JobStatus::Failed;
    let lease_expired_code = JobErrorCode::LeaseExpired;
    let job: Option<Job> = sqlx::query_as(
        "UPDATE jobs SET status=?2, worker_id=NULL, lease_token=NULL, lease_epoch=lease_epoch+1, \
         leased_until=NULL, error_code=NULL, free_releases=0, ungated_lease=0, \
         attempts=CASE WHEN attempts >= ?5 THEN 0 ELSE attempts END, \
         updated_at=datetime('now') \
         WHERE id=?1 AND status=?3 AND error_code=?4 RETURNING *",
    )
    .bind(id)
    .bind(queued_status)
    .bind(failed_status)
    .bind(lease_expired_code)
    .bind(MAX_ATTEMPTS)
    .fetch_optional(&mut *tx)
    .await?;
    let mut job = job.ok_or(AppError::Conflict)?;
    super::irreversible_actions::clear(&mut tx, id).await?;
    super::failure_flows::forget_failure(&mut tx, id).await?;
    if super::relays::restart_from_source(&mut tx, id).await? {
        job.target_worker_id = None;
        job.warning = false;
    }
    tx.commit().await?;
    Ok(job)
}

pub async fn fail(
    pool: &SqlitePool,
    id: Uuid,
    lease: &LeaseRequest,
    code: JobErrorCode,
) -> AppResult<()> {
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    super::leases::ensure_held(&mut tx, id, lease).await?;
    let publishing =
        super::source_publication_journal::publication_in_progress(&mut tx, id, lease.lease_epoch)
            .await?;
    if publishing.is_some() {
        return Err(AppError::Conflict);
    }
    super::leases::set_failed(&mut tx, id, code).await?;
    tx.commit().await?;
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

/// Rend aussi le résultat préservé d'une publication dont l'issue est incertaine.
pub async fn downloadable_output_name(pool: &SqlitePool, id: Uuid) -> AppResult<Option<String>> {
    let name: Option<String> = sqlx::query_scalar("SELECT output_name FROM jobs j WHERE id=?1 AND output_name IS NOT NULL AND (status=?2 OR (status=?3 AND EXISTS(SELECT 1 FROM source_publication_journal p WHERE p.job_id=j.id AND p.state=?4 AND p.output_name=j.output_name)))")
        .bind(id).bind(JobStatus::Succeeded).bind(JobStatus::Failed)
        .bind(super::source_publication_journal::PublicationState::Uncertain.as_str()).fetch_optional(pool).await?;
    Ok(name)
}

#[derive(Clone, Copy)]
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
    super::leases::ensure_held(&mut tx, id, lease).await?;
    super::irreversible_actions::ensure_no_failure_flow(&mut tx, id).await?;
    link_output.await?;
    let succeeded_status = JobStatus::Succeeded;
    // Un nom vide (résultat remplacé chez le détenteur) n'a pas de fichier au central.
    sqlx::query(
        "UPDATE jobs \
         SET status=?2, output_name=NULLIF(?3,''), output_sha256=?4, output_bytes=?5, warning=?6, \
             lease_token=NULL, lease_epoch=lease_epoch+1, leased_until=NULL, error_code=NULL, \
             updated_at=datetime('now') \
         WHERE id=?1",
    )
    .bind(id)
    .bind(succeeded_status)
    .bind(output.name)
    .bind(output.sha256)
    .bind(output.bytes)
    .bind(output.warning)
    .execute(&mut *tx)
    .await?;
    sqlx::query("DELETE FROM source_publication_journal WHERE job_id=?1 AND lease_epoch=?2")
        .bind(id)
        .bind(lease.lease_epoch)
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
    save_job_steps_inner(pool, job_id, None, steps).await
}

/// Vérifie le bail dans la transaction qui remplace le journal du segment courant.
pub async fn save_job_steps_for_lease(
    pool: &SqlitePool,
    job_id: Uuid,
    lease: &LeaseRequest,
    steps: &[JobStepEntry],
) -> AppResult<()> {
    save_job_steps_inner(pool, job_id, Some(lease), steps).await
}

async fn save_job_steps_inner(
    pool: &SqlitePool,
    job_id: Uuid,
    lease: Option<&LeaseRequest>,
    steps: &[JobStepEntry],
) -> AppResult<()> {
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    if let Some(lease) = lease {
        super::leases::ensure_held(&mut tx, job_id, lease).await?;
    }
    let exists = sqlx::query_scalar!(
        r#"SELECT EXISTS(SELECT 1 FROM jobs WHERE id=?1) AS "exists!: bool""#,
        job_id,
    )
    .fetch_one(&mut *tx)
    .await?;
    if !exists {
        return Err(AppError::NotFound);
    }
    super::relays::reset_steps(&mut tx, job_id).await?;
    let prefix: i64 = sqlx::query_scalar(
        "SELECT COALESCE(MAX(step_index), 0) FROM job_handoff_steps WHERE job_id=?1",
    )
    .bind(job_id)
    .fetch_one(&mut *tx)
    .await?;
    for step in steps {
        insert_job_step(&mut tx, job_id, prefix, step).await?;
    }
    tx.commit().await?;
    Ok(())
}

/// Insère une étape du journal, décalée de `prefix` (étapes déjà présentes avant elle).
pub(super) async fn insert_job_step(
    conn: &mut sqlx::SqliteConnection,
    job_id: Uuid,
    prefix: i64,
    step: &JobStepEntry,
) -> AppResult<()> {
    let step_index = i64::from(step.step_index)
        .checked_add(prefix)
        .ok_or(AppError::Invalid)?;
    let variables_json = step
        .variables
        .as_ref()
        .map(serde_json::to_string)
        .transpose()
        .map_err(|_| AppError::Invalid)?;
    sqlx::query(
        "INSERT INTO job_steps (job_id, step_index, node_id, block_kind, operation_id, \
                                status, message, output_excerpt, variables_json) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
    )
    .bind(job_id)
    .bind(step_index)
    .bind(&step.node_id)
    .bind(&step.block_kind)
    .bind(&step.operation_id)
    .bind(step.status)
    .bind(&step.message)
    .bind(&step.output_excerpt)
    .bind(variables_json)
    .execute(conn)
    .await?;
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
    sqlx::query("DELETE FROM source_publication_journal WHERE job_id=?1 AND state=?2")
        .bind(id)
        .bind(super::source_publication_journal::PublicationState::Uncertain.as_str())
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
    let publication_active: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM source_publication_journal p JOIN jobs j ON j.id=p.job_id AND j.lease_epoch=p.lease_epoch WHERE p.job_id=?1 AND p.state=?2)")
        .bind(id).bind(super::source_publication_journal::PublicationState::InProgress.as_str())
        .fetch_one(&mut *tx).await?;
    if publication_active {
        return Err(AppError::Conflict);
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
