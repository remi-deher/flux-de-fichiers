//! Routes du repli par le central pour le partage des sources (ADR 0019).

use super::AppState;
use crate::{
    domain::Job,
    error::{AppError, AppResult},
    repository,
    source_relay::SOURCE_RELAY_START_TIMEOUT,
};
use axum::{
    Json,
    body::Body,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};
use forgearr_shared::transfer::SourceRelayTask;
use futures_util::StreamExt;
use serde::Deserialize;
use std::{io, time::Duration};
use uuid::Uuid;

/// Attente longue maximale d'un worker source qui interroge ses demandes en attente.
const MAX_PENDING_WAIT_MS: u64 = 15_000;

#[derive(Debug, Deserialize)]
pub struct PendingQuery {
    pub worker_id: Uuid,
    #[serde(default)]
    pub wait_ms: Option<u64>,
}

#[derive(Debug, Deserialize)]
pub struct SendQuery {
    pub worker_id: Uuid,
}

/// Relaie vers le worker traitant la source détenue par un autre worker (réponse de
/// `GET /api/v1/jobs/{id}/source` quand le central n'a pas le fichier).
pub async fn relay_source_from_worker(s: &AppState, job: &Job) -> AppResult<Response> {
    let source = repository::job_source(&s.pool, job.id)
        .await?
        .ok_or(AppError::NotFound)?;
    let owner = source.source_worker_id.ok_or(AppError::NotFound)?;
    let mut stream = s
        .source_relay
        .request(
            owner,
            source.source_root,
            job.source_name.clone(),
            SOURCE_RELAY_START_TIMEOUT,
        )
        .await?;
    let body = Body::from_stream(futures_util::stream::poll_fn(move |cx| {
        stream.poll_recv(cx)
    }));
    let mut response = Response::new(body);
    if let Some(bytes) = job.source_bytes {
        response
            .headers_mut()
            .insert(header::CONTENT_LENGTH, bytes.into());
    }
    Ok(response)
}

/// Un worker source interroge les envois que le central lui demande.
pub async fn pending(
    State(s): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<PendingQuery>,
) -> AppResult<Json<Vec<SourceRelayTask>>> {
    crate::auth::authorize_worker(&s.pool, &headers).await?;
    let wait = Duration::from_millis(query.wait_ms.unwrap_or(0).min(MAX_PENDING_WAIT_MS));
    Ok(Json(
        s.source_relay.take_for_worker(query.worker_id, wait).await,
    ))
}

/// Un worker source envoie le fichier demandé ; le central le relaie sans le stocker.
pub async fn send(
    State(s): State<AppState>,
    headers: HeaderMap,
    Path(request_id): Path<Uuid>,
    Query(query): Query<SendQuery>,
    body: Body,
) -> AppResult<Response> {
    crate::auth::authorize_worker(&s.pool, &headers).await?;
    let sink = s
        .source_relay
        .begin(request_id, query.worker_id)
        .await
        .ok_or(AppError::NotFound)?;
    let mut chunks = body.into_data_stream();
    while let Some(chunk) = chunks.next().await {
        let chunk = chunk.map_err(|error| io::Error::other(error.to_string()));
        let failed = chunk.is_err();
        if !sink.send(chunk).await {
            return Ok(StatusCode::GONE.into_response());
        }
        if failed {
            return Err(AppError::CentralUnavailable);
        }
    }
    Ok(StatusCode::NO_CONTENT.into_response())
}
