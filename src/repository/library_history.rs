use super::{get_job, job_source};
use crate::{
    domain::{Job, JobStatus},
    error::{AppError, AppResult},
};
use forgearr_shared::libraries::{LibraryFileHistory, PlacementStatus, WorkerPlacement};
use sqlx::SqlitePool;
use uuid::Uuid;

const MAX_FILE_ATTEMPTS_SHOWN: i64 = 25;

pub async fn files(
    pool: &SqlitePool,
    library_id: Uuid,
    offset: i64,
) -> AppResult<Vec<LibraryFileHistory>> {
    super::get_library(pool, library_id).await?;
    let rows: Vec<(Option<String>, Option<Uuid>, String, Option<String>)> = sqlx::query_as(
        "SELECT source_root, source_worker_id, source_name, source_key FROM jobs WHERE library_id=?
         GROUP BY source_key, CASE WHEN source_key IS NULL THEN source_root END, CASE WHEN source_key IS NULL THEN source_worker_id END, source_name
         ORDER BY max(created_at) DESC, source_root, source_worker_id, source_name LIMIT ? OFFSET ?",
    ).bind(library_id).bind(forgearr_shared::libraries::LIBRARY_HISTORY_PAGE_SIZE as i64).bind(offset.max(0)).fetch_all(pool).await?;
    let mut files = Vec::with_capacity(rows.len());
    for (root, worker, name, key) in rows {
        let jobs = sqlx::query_as::<_, Job>(
            "SELECT * FROM jobs WHERE library_id=?1 AND source_name=?4 AND (source_key=?6 OR (?6 IS NULL AND source_key IS NULL AND source_root IS ?2 AND source_worker_id IS ?3))
             ORDER BY created_at DESC, id DESC LIMIT ?5",
        ).bind(library_id).bind(&root).bind(worker).bind(name).bind(MAX_FILE_ATTEMPTS_SHOWN).bind(key).fetch_all(pool).await?;
        files.push(LibraryFileHistory {
            source_root: root,
            source_worker_id: worker,
            jobs,
        });
    }
    Ok(files)
}

pub async fn reprocess(
    pool: &SqlitePool,
    library_id: Uuid,
    job_id: Uuid,
    version: Option<i64>,
) -> AppResult<Job> {
    let previous = get_job(pool, job_id).await?;
    if previous.library_id != Some(library_id)
        || matches!(previous.status, JobStatus::Queued | JobStatus::Running)
    {
        return Err(AppError::Conflict);
    }
    let source = job_source(pool, previous.id).await?;
    let worker = source
        .as_ref()
        .and_then(|location| location.source_worker_id);
    let shared = source
        .as_ref()
        .is_some_and(|source| source.source_key.is_some());
    if let Some(worker) = worker
        && !shared
        && !super::worker_exists(pool, worker).await?
    {
        return Err(AppError::NotFound);
    }
    // Rejoue la même empreinte. Un fichier modifié passe par un nouveau scan.
    let hash = previous.source_sha256.as_deref().ok_or(AppError::Invalid)?;
    let size = previous.source_bytes.ok_or(AppError::Invalid)?;
    let requested = match (
        previous.flow_id.as_deref(),
        version.or(previous.flow_version),
    ) {
        (Some(id), Some(version)) => {
            super::get_flow_version(pool, id, version).await?;
            Some(version)
        }
        (_, Some(_)) => return Err(AppError::Invalid),
        (_, None) => None,
    };
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM jobs WHERE library_id=?1 AND source_name=?2
         AND (source_key=?5 OR (?5 IS NULL AND source_key IS NULL AND source_root IS ?3 AND source_worker_id IS ?4))
         AND status IN ('queued','running'))",
    )
    .bind(library_id)
    .bind(&previous.source_name)
    .bind(source.as_ref().map(|s| s.source_root.as_str()))
    .bind(worker)
    .bind(source.as_ref().and_then(|source| source.source_key.as_deref()))
    .fetch_one(&mut *tx)
    .await?;
    if exists {
        return Err(AppError::Conflict);
    }
    let mut job = super::jobs::create_job(
        &mut tx,
        &previous.source_name,
        if shared {
            previous.target_worker_id
        } else {
            worker
        },
        hash,
        size,
        Some(library_id),
        previous.flow_id.as_deref(),
        previous.priority,
        source.as_ref(),
    )
    .await?;
    if shared {
        sqlx::query(
            "INSERT OR IGNORE INTO job_source_mounts (job_id,worker_id,local_path,writable)
                     SELECT ?,worker_id,local_path,writable FROM job_source_mounts WHERE job_id=?",
        )
        .bind(job.id)
        .bind(previous.id)
        .execute(&mut *tx)
        .await?;
    }
    if let Some(version) = requested {
        sqlx::query("UPDATE jobs SET flow_version=? WHERE id=?")
            .bind(version)
            .bind(job.id)
            .execute(&mut *tx)
            .await?;
        job.flow_version = Some(version);
    }
    tx.commit().await?;
    Ok(job)
}

pub async fn placement(
    pool: &SqlitePool,
    job: &Job,
    hardware_requested: bool,
) -> AppResult<Vec<WorkerPlacement>> {
    use chrono::{Datelike, Timelike};
    let source = job_source(pool, job.id).await?;
    let source_worker = source.as_ref().and_then(|source| source.source_worker_id);
    let mounts: std::collections::HashMap<Uuid, String> = sqlx::query_as::<_, (Uuid, String)>(
        "SELECT worker_id, local_path FROM verified_job_source_mounts WHERE job_id=?",
    )
    .bind(job.id)
    .fetch_all(pool)
    .await?
    .into_iter()
    .collect();
    let shared = source
        .as_ref()
        .is_some_and(|source| source.source_key.is_some());
    let central_path = mounts.get(&Uuid::nil());
    let now = chrono::Local::now();
    let mut candidates = Vec::new();
    for worker in super::list_workers(pool).await? {
        let has_source = if shared {
            mounts.contains_key(&worker.id) || central_path.is_some()
        } else {
            source_worker.is_none_or(|id| id == worker.id)
        };
        if !has_source || job.target_worker_id.is_some_and(|id| id != worker.id) {
            continue;
        }
        let status = if !worker.is_online(now.to_utc()) {
            PlacementStatus::Offline
        } else if worker.schedule.as_ref().is_some_and(|schedule| {
            !schedule.allows_at(now.weekday().number_from_monday() as u8, now.hour() as u8)
        }) {
            PlacementStatus::OutsideSchedule
        } else {
            let count: i64 = sqlx::query_scalar("SELECT count(*) FROM jobs WHERE worker_id=? AND status='running' AND leased_until>datetime('now')")
                .bind(worker.id).fetch_one(pool).await?;
            if count >= i64::from(worker.max_concurrency) {
                PlacementStatus::AtCapacity
            } else {
                PlacementStatus::Ready
            }
        };
        candidates.push(WorkerPlacement {
            worker_id: worker.id,
            name: worker.name,
            status,
            source_path: mounts
                .get(&worker.id)
                .or(central_path)
                .cloned()
                .or_else(|| source.as_ref().map(|source| source.source_root.clone())),
            gpu_not_reported: hardware_requested
                && worker
                    .telemetry
                    .as_ref()
                    .is_some_and(|telemetry| telemetry.gpu_devices.is_empty()),
        });
    }
    Ok(candidates)
}
