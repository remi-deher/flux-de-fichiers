use super::AppState;
use crate::{
    auth::authorize,
    domain::{Job, LeaseRequest},
    error::{AppError, AppResult},
    media, repository, storage,
};
use axum::{
    body::Body,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use futures_util::StreamExt;
use tokio::io::AsyncWriteExt;
use tokio_util::io::ReaderStream;
use uuid::Uuid;

fn lease(h: &HeaderMap) -> AppResult<LeaseRequest> {
    let uuid = |key: &str| {
        h.get(key)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse().ok())
            .ok_or(AppError::Invalid)
    };
    Ok(LeaseRequest {
        worker_id: uuid("x-worker-id")?,
        lease_token: uuid("x-lease-token")?,
        telemetry: None,
    })
}

async fn stream_file(path: &std::path::Path, attachment_name: Option<&str>) -> AppResult<Response> {
    let file = tokio::fs::File::open(path).await?;
    let size = file.metadata().await?.len();
    let mut response = (
        [
            ("content-type", "video/x-matroska".to_owned()),
            ("content-length", size.to_string()),
        ],
        Body::from_stream(ReaderStream::new(file)),
    )
        .into_response();
    response.headers_mut().insert(
        axum::http::header::X_CONTENT_TYPE_OPTIONS,
        axum::http::HeaderValue::from_static("nosniff"),
    );
    if let Some(name) = attachment_name {
        let value = format!("attachment; filename=\"{name}\"");
        let value = axum::http::HeaderValue::from_str(&value).map_err(|_| AppError::Internal)?;
        response
            .headers_mut()
            .insert(axum::http::header::CONTENT_DISPOSITION, value);
    }
    Ok(response)
}

pub async fn source(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(id): Path<Uuid>,
) -> AppResult<Response> {
    authorize(&h, &s.config.worker_key)?;
    let job = repository::active(&s.pool, id, &lease(&h)?).await?;
    stream_file(
        &storage::input_path(&s.config.input_dir, &job.source_name).await?,
        None,
    )
    .await
}
pub async fn output(
    State(s): State<AppState>,
    Path(id): Path<Uuid>,
    _auth: super::AdminAuth,
) -> AppResult<Response> {
    let name: Option<String> =
        sqlx::query_scalar("SELECT output_name FROM jobs WHERE id=?1 AND status='succeeded'")
            .bind(id)
            .fetch_optional(&s.pool)
            .await?
            .flatten();
    let mut response = stream_file(
        &s.config
            .data_dir
            .join("output")
            .join(name.ok_or(AppError::NotFound)?),
        Some(&format!("job-{id}.mkv")),
    )
    .await?;
    response.headers_mut().insert(
        axum::http::header::CACHE_CONTROL,
        axum::http::HeaderValue::from_static("no-store"),
    );
    Ok(response)
}

pub async fn result(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(id): Path<Uuid>,
    body: Body,
) -> AppResult<StatusCode> {
    authorize(&h, &s.config.worker_key)?;
    let credentials = lease(&h)?;
    let job = repository::active(&s.pool, id, &credentials).await?;
    let warning = match h.get("x-remux-warning").and_then(|v| v.to_str().ok()) {
        Some("true") => true,
        Some("false") | None => false,
        _ => return Err(AppError::Invalid),
    };
    let staging = s
        .config
        .data_dir
        .join("staging")
        .join(format!("{}.mkv", Uuid::new_v4()));
    let outcome = tokio::time::timeout(
        std::time::Duration::from_secs(3600),
        receive(&s, id, &credentials, &job, body, &staging, warning),
    )
    .await;
    if let Err(error) = tokio::fs::remove_file(&staging).await
        && error.kind() != std::io::ErrorKind::NotFound
    {
        tracing::error!("staging cleanup failed");
    }
    outcome.map_err(|_| AppError::Invalid)??;
    Ok(StatusCode::NO_CONTENT)
}

async fn receive(
    s: &AppState,
    id: Uuid,
    lease: &LeaseRequest,
    job: &Job,
    body: Body,
    staging: &std::path::Path,
    warning: bool,
) -> AppResult<()> {
    let mut file = tokio::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(staging)
        .await?;
    let mut stream = body.into_data_stream();
    let mut total = 0u64;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|_| AppError::Invalid)?;
        total += chunk.len() as u64;
        if total > s.config.max_file_bytes {
            return Err(AppError::TooLarge);
        }
        file.write_all(&chunk).await?;
    }
    file.sync_all().await?;
    drop(file);
    let source = storage::input_path(&s.config.input_dir, &job.source_name).await?;
    publish_staged(s, id, lease, job, &source, staging, warning).await?;
    Ok(())
}

pub(crate) async fn publish_staged(
    s: &AppState,
    id: Uuid,
    lease: &LeaseRequest,
    job: &Job,
    source: &std::path::Path,
    staging: &std::path::Path,
    warning: bool,
) -> AppResult<()> {
    let (hash, size) = storage::fingerprint(staging, s.config.max_file_bytes).await?;
    let (source_hash, source_size) = storage::fingerprint(source, s.config.max_file_bytes).await?;
    if Some(&source_hash) != job.source_sha256.as_ref()
        || job.source_bytes != Some(source_size as i64)
    {
        return Err(AppError::Conflict);
    }
    media::validate_remux(source, staging)
        .await
        .map_err(|_| AppError::Invalid)?;
    let mut tx = s.pool.begin_with("BEGIN IMMEDIATE").await?;
    // Fence a former lease even if it finishes after another agent has taken over.
    let active: Option<Uuid> = sqlx::query_scalar("SELECT id FROM jobs WHERE id=?1 AND worker_id=?2 AND lease_token=?3 AND status='running' AND leased_until > datetime('now')")
        .bind(id).bind(lease.worker_id).bind(lease.lease_token).fetch_optional(&mut *tx).await?;
    if active.is_none() {
        return Err(AppError::Conflict);
    }
    let name = format!("{id}-{}.mkv", lease.lease_token);
    let destination = s.config.data_dir.join("output").join(&name);
    // Hard-link publication is atomic, cannot overwrite, and stays on the data filesystem.
    tokio::fs::hard_link(staging, &destination).await?;
    let updated = sqlx::query("UPDATE jobs SET status='succeeded',output_name=?2,output_sha256=?3,output_bytes=?4,warning=?5,lease_token=NULL,leased_until=NULL,error_code=NULL,updated_at=datetime('now') WHERE id=?1")
        .bind(id).bind(name).bind(hash).bind(size as i64).bind(warning).execute(&mut *tx).await;
    let completed = match updated {
        Ok(_) => tx.commit().await,
        Err(e) => Err(e),
    };
    if let Err(error) = completed {
        // Commit outcome can be uncertain after a network failure. Keep the immutable file;
        // the database alone decides whether it is downloadable. Never remove a committed result.
        tracing::error!(job_id=%id, "result publication database failure");
        return Err(error.into());
    }
    tracing::info!(job_id=%id, worker_id=%lease.worker_id, bytes=size, warning, "remux completed");
    s.notify_job_event("job_succeeded", id, "succeeded", None);
    Ok(())
}
