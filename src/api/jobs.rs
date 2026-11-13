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
use serde::{Deserialize, Serialize};
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
    let job = repository::insert_job(
        &s.pool,
        &r.source_name,
        r.target_worker_id,
        &hash,
        bytes as i64,
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

pub async fn events(
    State(s): State<AppState>,
    _auth: ViewJobsAuth,
) -> AppResult<Sse<impl Stream<Item = Result<Event, Infallible>>>> {
    let rx = s.events_tx.subscribe();
    let stream = futures_util::stream::unfold(rx, |mut rx| async move {
        loop {
            match rx.recv().await {
                Ok(event) => {
                    let data = serde_json::to_string(&event).unwrap_or_default();
                    let sse_event = Event::default().event(event.event_type.as_str()).data(data);
                    return Some((Ok(sse_event), rx));
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(tokio::sync::broadcast::error::RecvError::Closed) => return None,
            }
        }
    });

    Ok(Sse::new(stream).keep_alive(KeepAlive::new().interval(std::time::Duration::from_secs(15))))
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

#[derive(Serialize)]
pub struct SystemConfigResponse {
    pub max_file_bytes: u64,
    pub local_worker: bool,
    pub version: &'static str,
}

pub async fn system_config(
    State(s): State<AppState>,
    _auth: ViewOverviewAuth,
) -> AppResult<Json<SystemConfigResponse>> {
    Ok(Json(SystemConfigResponse {
        max_file_bytes: s.config.max_file_bytes,
        local_worker: s.config.local_worker,
        version: env!("CARGO_PKG_VERSION"),
    }))
}
