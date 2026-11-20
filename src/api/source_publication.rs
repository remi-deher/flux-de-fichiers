//! Routes de remise du résultat au worker détenteur de la source.

use super::AppState;
use crate::{
    error::{AppError, AppResult},
    repository,
};
use axum::{
    Json,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::Response,
};
use forgearr_shared::transfer::{SourcePublicationReport, SourcePublicationTask};
use serde::Deserialize;
use std::time::Duration;
use uuid::Uuid;

const MAX_PENDING_WAIT_MS: u64 = 15_000;

#[derive(Deserialize)]
pub struct HolderQuery {
    worker_id: Uuid,
    #[serde(default)]
    wait_ms: Option<u64>,
}

/// Une clé de worker n'autorise pas à agir pour un autre worker.
async fn authorize_holder(s: &AppState, headers: &HeaderMap, worker_id: Uuid) -> AppResult<()> {
    let principal = crate::auth::authorize_worker(&s.pool, headers).await?;
    let worker = repository::get_worker(&s.pool, worker_id).await?;
    if principal.name != worker.name {
        return Err(AppError::Forbidden);
    }
    Ok(())
}

pub async fn pending(
    State(s): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<HolderQuery>,
) -> AppResult<Json<Vec<SourcePublicationTask>>> {
    authorize_holder(&s, &headers, query.worker_id).await?;
    let wait = Duration::from_millis(query.wait_ms.unwrap_or(0).min(MAX_PENDING_WAIT_MS));
    Ok(Json(
        s.source_publication
            .take_for_worker(query.worker_id, wait)
            .await,
    ))
}

pub async fn staged(
    State(s): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Query(query): Query<HolderQuery>,
) -> AppResult<Response> {
    authorize_holder(&s, &headers, query.worker_id).await?;
    let access = s
        .source_publication
        .access_for_worker(id, query.worker_id)
        .await
        .ok_or(AppError::NotFound)?;
    repository::active(&s.pool, access.job_id, &access.lease).await?;
    super::transfers::stream_file(&access.staged, None, "application/octet-stream").await
}

/// Vérifie le bail juste avant que le détenteur ne remplace la source.
pub async fn authorize_commit(
    State(s): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Query(query): Query<HolderQuery>,
) -> AppResult<StatusCode> {
    authorize_holder(&s, &headers, query.worker_id).await?;
    let access = s
        .source_publication
        .access_for_worker(id, query.worker_id)
        .await
        .ok_or(AppError::NotFound)?;
    repository::active(&s.pool, access.job_id, &access.lease).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn report(
    State(s): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(report): Json<SourcePublicationReport>,
) -> AppResult<StatusCode> {
    authorize_holder(&s, &headers, report.worker_id).await?;
    if id != report.request_id {
        return Err(AppError::Invalid);
    }
    let access = s
        .source_publication
        .access_for_worker(id, report.worker_id)
        .await
        .ok_or(AppError::NotFound)?;
    repository::active(&s.pool, access.job_id, &access.lease).await?;
    if !s.source_publication.report(report).await {
        return Err(AppError::NotFound);
    }
    Ok(StatusCode::NO_CONTENT)
}
