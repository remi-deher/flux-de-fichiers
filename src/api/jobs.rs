use super::{
    AdminAuth, AppState, ViewJobsAuth, ViewLibrariesAuth, ViewOverviewAuth, ViewWorkersAuth,
};
use crate::{
    domain::*,
    error::{AppError, AppResult},
    repository, storage,
};
use axum::{
    Json,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::sse::{Event, KeepAlive, Sse},
};
use futures_util::stream::Stream;
use serde::Deserialize;
use std::convert::Infallible;
use uuid::Uuid;

pub async fn files(
    State(s): State<AppState>,
    _auth: ViewLibrariesAuth,
) -> AppResult<Json<Vec<MediaFile>>> {
    Ok(Json(
        storage::list_inputs(&s.config.input_dir, s.config.max_file_bytes).await?,
    ))
}

pub async fn create(
    State(s): State<AppState>,
    _auth: AdminAuth,
    Json(r): Json<NewJob>,
) -> AppResult<(StatusCode, Json<Job>)> {
    let path = storage::input_path(&s.config.input_dir, &r.source_name).await?;
    let (hash, bytes) = storage::fingerprint(&path, s.config.max_file_bytes).await?;
    if let Some(worker) = r.target_worker_id
        && !repository::worker_exists(&s.pool, worker).await?
    {
        return Err(AppError::Invalid);
    }
    let job = repository::insert_job_with_options(
        &s.pool,
        &r.source_name,
        r.target_worker_id,
        &hash,
        bytes as i64,
        r.library_id,
        r.flow_id.as_deref(),
    )
    .await?;
    s.notify_job_event(JobEventType::JobCreated, job.id, JobStatus::Queued, None);
    Ok((StatusCode::CREATED, Json(job)))
}

#[derive(Deserialize)]
pub struct Page {
    limit: Option<i64>,
    before: Option<Uuid>,
}
pub async fn list(
    State(s): State<AppState>,
    Query(p): Query<Page>,
    _auth: ViewJobsAuth,
) -> AppResult<Json<Vec<Job>>> {
    let limit = p.limit.unwrap_or(50);
    if !(1..=200).contains(&limit) {
        return Err(AppError::Invalid);
    }
    Ok(Json(repository::list_jobs(&s.pool, p.before, limit).await?))
}

struct EventsCursor {
    replay: std::collections::VecDeque<(u64, JobEvent)>,
    rx: tokio::sync::broadcast::Receiver<(u64, JobEvent)>,
    last_seq: u64,
}

fn format_sse_event(seq: u64, event: &JobEvent) -> Event {
    let data = serde_json::to_string(event).unwrap_or_default();
    Event::default()
        .id(seq.to_string())
        .event(event.event_type.as_str())
        .data(data)
}

pub async fn events(
    State(s): State<AppState>,
    h: HeaderMap,
    _auth: ViewJobsAuth,
) -> AppResult<Sse<impl Stream<Item = Result<Event, Infallible>>>> {
    let last_event_id = h
        .get("last-event-id")
        .and_then(|value| value.to_str().ok())
        .and_then(|raw| raw.trim().parse::<u64>().ok());
    let (replay, rx) = s.event_history.subscribe_from(last_event_id).await;
    let cursor = EventsCursor {
        replay: replay.into(),
        rx,
        last_seq: last_event_id.unwrap_or(0),
    };
    let stream = futures_util::stream::unfold(cursor, |mut cursor| async move {
        if let Some((seq, event)) = cursor.replay.pop_front() {
            cursor.last_seq = cursor.last_seq.max(seq);
            return Some((Ok(format_sse_event(seq, &event)), cursor));
        }
        loop {
            match cursor.rx.recv().await {
                Ok((seq, event)) => {
                    if seq <= cursor.last_seq {
                        continue;
                    }
                    cursor.last_seq = seq;
                    return Some((Ok(format_sse_event(seq, &event)), cursor));
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(tokio::sync::broadcast::error::RecvError::Closed) => return None,
            }
        }
    });

    Ok(Sse::new(stream).keep_alive(KeepAlive::new().interval(std::time::Duration::from_secs(15))))
}

/// Journal ordonné des étapes exécutées pour une tâche.
pub async fn steps(
    State(s): State<AppState>,
    Path(id): Path<Uuid>,
    _auth: ViewJobsAuth,
) -> AppResult<Json<Vec<JobStepLog>>> {
    Ok(Json(repository::list_job_steps(&s.pool, id).await?))
}

/// Remontée par un worker de la trace d'exécution des nœuds d'une tâche.
pub async fn report_steps(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(id): Path<Uuid>,
    Json(r): Json<StepsReport>,
) -> AppResult<StatusCode> {
    crate::auth::authorize_worker(&s.pool, &h).await?;
    repository::active(
        &s.pool,
        id,
        &LeaseRequest {
            worker_id: r.worker_id,
            lease_token: r.lease_token,
            lease_epoch: r.lease_epoch,
            telemetry: None,
        },
    )
    .await?;
    repository::save_job_steps(&s.pool, id, &r.steps).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn get_job(
    State(s): State<AppState>,
    Path(id): Path<Uuid>,
    _auth: ViewJobsAuth,
) -> AppResult<Json<Job>> {
    Ok(Json(repository::get_job(&s.pool, id).await?))
}
/// Relance manuelle d'une tâche échouée après perte de bail (aucune relance automatique).
pub async fn retry(
    State(s): State<AppState>,
    Path(id): Path<Uuid>,
    _auth: AdminAuth,
) -> AppResult<Json<Job>> {
    let job = repository::retry_job(&s.pool, id).await?;
    s.notify_job_event(JobEventType::JobRetried, job.id, JobStatus::Queued, None);
    Ok(Json(job))
}
pub async fn workers(
    State(s): State<AppState>,
    _auth: ViewWorkersAuth,
) -> AppResult<Json<Vec<Worker>>> {
    Ok(Json(repository::list_workers(&s.pool).await?))
}

pub async fn delete_worker(
    State(s): State<AppState>,
    Path(id): Path<Uuid>,
    _auth: AdminAuth,
) -> AppResult<StatusCode> {
    repository::delete_worker(&s.pool, id).await?;
    Ok(StatusCode::NO_CONTENT)
}
pub async fn heartbeat(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(id): Path<Uuid>,
    Json(r): Json<LeaseRequest>,
) -> AppResult<StatusCode> {
    crate::auth::authorize_worker(&s.pool, &h).await?;
    repository::heartbeat(&s.pool, id, &r).await?;
    s.notify_job_event(JobEventType::JobHeartbeat, id, JobStatus::Running, None);
    Ok(StatusCode::NO_CONTENT)
}

/// Progression éphémère d'un worker distant → bus SSE `job_progress` (même contrat que le local).
pub async fn progress(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(id): Path<Uuid>,
    Json(r): Json<ProgressReport>,
) -> AppResult<StatusCode> {
    crate::auth::authorize_worker(&s.pool, &h).await?;
    if r.progress > 100 {
        return Err(AppError::Invalid);
    }
    repository::active(
        &s.pool,
        id,
        &LeaseRequest {
            worker_id: r.worker_id,
            lease_token: r.lease_token,
            lease_epoch: r.lease_epoch,
            telemetry: None,
        },
    )
    .await?;
    s.notify_job_progress(id, r.progress, r.message);
    Ok(StatusCode::NO_CONTENT)
}

pub async fn failure(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(id): Path<Uuid>,
    Json(r): Json<Failure>,
) -> AppResult<StatusCode> {
    crate::auth::authorize_worker(&s.pool, &h).await?;
    let error_code: JobErrorCode = r.error_code.parse().map_err(|_| AppError::Invalid)?;
    if !error_code.is_worker_reportable() {
        return Err(AppError::Invalid);
    }
    repository::fail(
        &s.pool,
        id,
        &LeaseRequest {
            worker_id: r.worker_id,
            lease_token: r.lease_token,
            lease_epoch: r.lease_epoch,
            telemetry: None,
        },
        error_code,
    )
    .await?;
    s.notify_job_event(
        JobEventType::JobFailed,
        id,
        JobStatus::Failed,
        Some(error_code),
    );
    Ok(StatusCode::NO_CONTENT)
}

pub async fn system_config(
    State(s): State<AppState>,
    _auth: ViewOverviewAuth,
) -> AppResult<Json<SystemConfig>> {
    Ok(Json(SystemConfig {
        max_file_bytes: s.config.max_file_bytes,
        local_worker: s.config.local_worker,
        version: env!("CARGO_PKG_VERSION").to_owned(),
    }))
}
