use super::AppState;
use crate::{
    auth::authorize,
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

pub async fn files(State(s): State<AppState>, h: HeaderMap) -> AppResult<Json<Vec<MediaFile>>> {
    crate::web_session::authorize_admin(&s.pool, &s.config, &h, &axum::http::Method::GET).await?;
    Ok(Json(
        storage::list_inputs(&s.config.input_dir, s.config.max_file_bytes).await?,
    ))
}

pub async fn create(
    State(s): State<AppState>,
    h: HeaderMap,
    Json(r): Json<NewJob>,
) -> AppResult<(StatusCode, Json<Job>)> {
    crate::web_session::authorize_admin(&s.pool, &s.config, &h, &axum::http::Method::POST).await?;
    let path = storage::input_path(&s.config.input_dir, &r.source_name).await?;
    let (hash, bytes) = storage::fingerprint(&path, s.config.max_file_bytes).await?;
    if let Some(worker) = r.target_worker_id {
        let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM workers WHERE id=?1)")
            .bind(worker)
            .fetch_one(&s.pool)
            .await?;
        if !exists {
            return Err(AppError::Invalid);
        }
    }
    let job: Job = sqlx::query_as("INSERT INTO jobs(id,source_name,target_worker_id,source_sha256,source_bytes) VALUES(?1,?2,?3,?4,?5) RETURNING *")
        .bind(Uuid::new_v4()).bind(r.source_name).bind(r.target_worker_id).bind(hash).bind(bytes as i64).fetch_one(&s.pool).await?;
    s.notify_job_event("job_created", job.id, &job.status, None);
    Ok((StatusCode::CREATED, Json(job)))
}

#[derive(Deserialize)]
pub struct Page {
    limit: Option<i64>,
    before: Option<Uuid>,
}
pub async fn list(
    State(s): State<AppState>,
    h: HeaderMap,
    Query(p): Query<Page>,
) -> AppResult<Json<Vec<Job>>> {
    crate::web_session::authorize_admin(&s.pool, &s.config, &h, &axum::http::Method::GET).await?;
    let limit = p.limit.unwrap_or(50);
    if !(1..=200).contains(&limit) {
        return Err(AppError::Invalid);
    }
    Ok(Json(sqlx::query_as("SELECT * FROM jobs WHERE (?1 IS NULL OR (created_at,id) < (SELECT created_at,id FROM jobs WHERE id=?1)) ORDER BY created_at DESC,id DESC LIMIT ?2")
        .bind(p.before).bind(limit).fetch_all(&s.pool).await?))
}

pub async fn events(
    State(s): State<AppState>,
    h: HeaderMap,
) -> AppResult<Sse<impl Stream<Item = Result<Event, Infallible>>>> {
    crate::web_session::authorize_admin(&s.pool, &s.config, &h, &axum::http::Method::GET).await?;
    let rx = s.events_tx.subscribe();
    let stream = futures_util::stream::unfold(rx, |mut rx| async move {
        loop {
            match rx.recv().await {
                Ok(event) => {
                    let data = serde_json::to_string(&event).unwrap_or_default();
                    let sse_event = Event::default().event(&event.event_type).data(data);
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
    h: HeaderMap,
    Path(id): Path<Uuid>,
) -> AppResult<Json<Job>> {
    crate::web_session::authorize_admin(&s.pool, &s.config, &h, &axum::http::Method::GET).await?;
    Ok(Json(
        sqlx::query_as("SELECT * FROM jobs WHERE id=?1")
            .bind(id)
            .fetch_optional(&s.pool)
            .await?
            .ok_or(AppError::NotFound)?,
    ))
}
pub async fn workers(State(s): State<AppState>, h: HeaderMap) -> AppResult<Json<Vec<Worker>>> {
    crate::web_session::authorize_admin(&s.pool, &s.config, &h, &axum::http::Method::GET).await?;
    Ok(Json(
        sqlx::query_as("SELECT * FROM workers ORDER BY name LIMIT 200")
            .fetch_all(&s.pool)
            .await?,
    ))
}
pub async fn heartbeat(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(id): Path<Uuid>,
    Json(r): Json<LeaseRequest>,
) -> AppResult<StatusCode> {
    authorize(&h, &s.config.worker_key)?;
    repository::heartbeat(&s.pool, id, &r).await?;
    s.notify_job_event("job_heartbeat", id, "running", None);
    Ok(StatusCode::NO_CONTENT)
}
pub async fn failure(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(id): Path<Uuid>,
    Json(r): Json<Failure>,
) -> AppResult<StatusCode> {
    authorize(&h, &s.config.worker_key)?;
    if !["remux_failed", "source_integrity", "transfer_failed"].contains(&r.error_code.as_str()) {
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
        &r.error_code,
    )
    .await?;
    s.notify_job_event("job_failed", id, "failed", Some(r.error_code));
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Serialize)]
pub struct SystemConfigResponse {
    pub worker_key: String,
    pub max_file_bytes: u64,
    pub local_worker: bool,
    pub version: &'static str,
}

pub async fn system_config(
    State(s): State<AppState>,
    h: HeaderMap,
) -> AppResult<Json<SystemConfigResponse>> {
    crate::web_session::authorize_admin(&s.pool, &s.config, &h, &axum::http::Method::GET).await?;
    Ok(Json(SystemConfigResponse {
        worker_key: s.config.worker_key.clone(),
        max_file_bytes: s.config.max_file_bytes,
        local_worker: s.config.local_worker,
        version: env!("CARGO_PKG_VERSION"),
    }))
}
