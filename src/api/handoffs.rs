//! Remise d'un fichier de travail par le central, avec accusé du repreneur.

use super::AppState;
use crate::{
    domain::{JobErrorCode, JobEventType, JobStatus, LeaseRequest},
    error::{AppError, AppResult},
    repository, storage,
};
use axum::{
    Json,
    body::Body,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::Response,
};
use forgearr_shared::relay::{
    HandoffAcknowledgement, HandoffArtifact, HandoffBegin, HandoffOffer, HandoffState, RelayRequest,
};
use futures_util::StreamExt;
use serde::Deserialize;
use tokio::io::AsyncWriteExt;
use uuid::Uuid;

const CLEANUP_INTERVAL: std::time::Duration = std::time::Duration::from_secs(60);
const CLEANUP_BATCH_SIZE: i64 = 128;

#[derive(Deserialize)]
pub struct WorkerQuery {
    worker_id: Uuid,
}

#[derive(Deserialize)]
pub struct StateQuery {
    worker_id: Uuid,
    lease_epoch: i64,
}

pub(crate) fn artifact_path(s: &AppState, job: Uuid, epoch: i64) -> std::path::PathBuf {
    s.config
        .data_dir
        .join("staging")
        .join(format!("handoff-{job}-{epoch}"))
}

/// Fait expirer les remises même si aucun worker n'est connecté et purge les copies achevées.
pub(crate) async fn cleanup_loop(s: AppState) {
    loop {
        if let Err(error) = cleanup(&s).await {
            tracing::warn!(error=%error, "handoff cleanup failed");
        }
        tokio::select! {
            _ = tokio::time::sleep(CLEANUP_INTERVAL) => {},
            _ = super::shutdown_signal() => return,
        }
    }
}

async fn cleanup(s: &AppState) -> AppResult<()> {
    repository::expire_handoffs(&s.pool).await?;
    let rows: Vec<(Uuid, i64)> = sqlx::query_as("SELECT job_id,source_lease_epoch FROM job_handoffs WHERE staging_cleaned=0 AND state IN (?1,?2) LIMIT ?3")
        .bind(HandoffState::Acknowledged.as_str()).bind(HandoffState::Failed.as_str()).bind(CLEANUP_BATCH_SIZE)
        .fetch_all(&s.pool).await?;
    for (job, epoch) in rows {
        match tokio::fs::remove_file(artifact_path(s, job, epoch)).await {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                tracing::warn!(job_id=%job, error=%error, "handoff staging cleanup failed");
                continue;
            }
        }
        sqlx::query(
            "UPDATE job_handoffs SET staging_cleaned=1 WHERE job_id=?1 AND source_lease_epoch=?2",
        )
        .bind(job)
        .bind(epoch)
        .execute(&s.pool)
        .await?;
    }
    Ok(())
}

pub async fn begin(
    State(s): State<AppState>,
    headers: HeaderMap,
    Path(job): Path<Uuid>,
    Json(begin): Json<HandoffBegin>,
) -> AppResult<StatusCode> {
    crate::auth::authorize_worker_identity(&s.pool, &headers, begin.worker_id).await?;
    let lease = LeaseRequest {
        worker_id: begin.worker_id,
        lease_token: begin.lease_token,
        lease_epoch: begin.lease_epoch,
        telemetry: None,
    };
    begin_or_fail(
        &s,
        job,
        &lease,
        &begin.request,
        &begin.artifact,
        begin.warning,
    )
    .await?;
    Ok(StatusCode::CREATED)
}

pub(crate) async fn begin_or_fail(
    s: &AppState,
    job: Uuid,
    lease: &LeaseRequest,
    request: &RelayRequest,
    artifact: &HandoffArtifact,
    warning: bool,
) -> AppResult<()> {
    if !repository::begin_handoff(
        &s.pool,
        job,
        lease,
        request,
        artifact,
        s.config.max_file_bytes,
        warning,
    )
    .await?
    {
        repository::fail(&s.pool, job, lease, JobErrorCode::NoCapableWorker).await?;
        s.notify_job_event(
            JobEventType::JobFailed,
            job,
            JobStatus::Failed,
            Some(JobErrorCode::NoCapableWorker),
        );
        return Err(AppError::Conflict);
    }
    Ok(())
}

pub async fn upload(
    State(s): State<AppState>,
    headers: HeaderMap,
    Path(job): Path<Uuid>,
    body: Body,
) -> AppResult<StatusCode> {
    let lease = super::transfers::lease(&headers)?;
    crate::auth::authorize_worker_identity(&s.pool, &headers, lease.worker_id).await?;
    let artifact = repository::handoff_upload_artifact(&s.pool, job, &lease).await?;
    let temporary = s
        .config
        .data_dir
        .join("staging")
        .join(format!("handoff-{}.tmp", Uuid::new_v4()));
    let destination = artifact_path(&s, job, lease.lease_epoch);
    let result = async {
        let mut file = tokio::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temporary)
            .await?;
        let mut stream = body.into_data_stream();
        let mut total = 0u64;
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|_| AppError::Invalid)?;
            total = total
                .checked_add(chunk.len() as u64)
                .ok_or(AppError::TooLarge)?;
            if total > s.config.max_file_bytes || total > artifact.bytes as u64 {
                return Err(AppError::TooLarge);
            }
            file.write_all(&chunk).await?;
        }
        file.sync_all().await?;
        drop(file);
        let (hash, size) = storage::fingerprint(&temporary, s.config.max_file_bytes).await?;
        if hash != artifact.sha256 || i64::try_from(size).ok() != Some(artifact.bytes) {
            return Err(AppError::Invalid);
        }
        tokio::fs::hard_link(&temporary, &destination).await?;
        storage::sync_directory(&s.config.data_dir.join("staging")).await?;
        if let Err(error) = repository::finish_handoff_upload(&s.pool, job, &lease).await {
            let _ = tokio::fs::remove_file(&destination).await;
            return Err(error);
        }
        Ok(StatusCode::NO_CONTENT)
    }
    .await;
    let _ = tokio::fs::remove_file(&temporary).await;
    result
}

pub async fn claim(
    State(s): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<WorkerQuery>,
) -> AppResult<Json<Option<HandoffOffer>>> {
    crate::auth::authorize_worker_identity(&s.pool, &headers, query.worker_id).await?;
    Ok(Json(
        repository::claim_handoff(&s.pool, query.worker_id).await?,
    ))
}

pub async fn download(
    State(s): State<AppState>,
    headers: HeaderMap,
    Path(job): Path<Uuid>,
    Query(query): Query<WorkerQuery>,
) -> AppResult<Response> {
    crate::auth::authorize_worker_identity(&s.pool, &headers, query.worker_id).await?;
    let token = headers
        .get("x-handoff-token")
        .and_then(|v| v.to_str().ok())
        .ok_or(AppError::Invalid)?;
    let (epoch, _) =
        repository::handoff_download_artifact(&s.pool, job, query.worker_id, token).await?;
    super::transfers::stream_file(
        &artifact_path(&s, job, epoch),
        None,
        "application/octet-stream",
    )
    .await
}

pub async fn status(
    State(s): State<AppState>,
    headers: HeaderMap,
    Path(job): Path<Uuid>,
    Query(query): Query<StateQuery>,
) -> AppResult<Json<HandoffState>> {
    crate::auth::authorize_worker_identity(&s.pool, &headers, query.worker_id).await?;
    repository::expire_handoffs(&s.pool).await?;
    Ok(Json(
        repository::handoff_state(&s.pool, job, query.worker_id, query.lease_epoch).await?,
    ))
}

/// Permet au seul repreneur de savoir si sa copie locale est encore nécessaire.
pub async fn retained(
    State(s): State<AppState>,
    headers: HeaderMap,
    Path((job, epoch)): Path<(Uuid, i64)>,
    Query(query): Query<WorkerQuery>,
) -> AppResult<Json<bool>> {
    crate::auth::authorize_worker_identity(&s.pool, &headers, query.worker_id).await?;
    Ok(Json(
        repository::retained_handoff_needed(&s.pool, job, query.worker_id, epoch).await?,
    ))
}

pub async fn acknowledge(
    State(s): State<AppState>,
    headers: HeaderMap,
    Path(job): Path<Uuid>,
    Json(ack): Json<HandoffAcknowledgement>,
) -> AppResult<StatusCode> {
    crate::auth::authorize_worker_identity(&s.pool, &headers, ack.worker_id).await?;
    let epoch =
        repository::handoff_download_artifact(&s.pool, job, ack.worker_id, &ack.receipt_token)
            .await
            .ok()
            .map(|(epoch, _)| epoch);
    if repository::acknowledge_handoff(&s.pool, job, &ack).await? {
        s.notify_job_event(JobEventType::JobRelayed, job, JobStatus::Queued, None);
        if let Some(epoch) = epoch
            && let Err(error) = tokio::fs::remove_file(artifact_path(&s, job, epoch)).await
            && error.kind() != std::io::ErrorKind::NotFound
        {
            tracing::warn!(job_id=%job, error=%error, "handoff staging cleanup failed");
        }
    }
    Ok(StatusCode::NO_CONTENT)
}
