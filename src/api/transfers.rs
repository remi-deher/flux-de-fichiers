use super::AppState;
use crate::{
    domain::{Job, JobEventType, JobStatus, LeaseRequest},
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
    let lease_epoch = h
        .get("x-lease-epoch")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<i64>().ok())
        .ok_or(AppError::Invalid)?;
    Ok(LeaseRequest {
        worker_id: uuid("x-worker-id")?,
        lease_token: uuid("x-lease-token")?,
        lease_epoch,
        telemetry: None,
    })
}

pub(super) async fn stream_file(
    path: &std::path::Path,
    attachment_name: Option<&str>,
    content_type: &str,
) -> AppResult<Response> {
    let file = tokio::fs::File::open(path).await?;
    let size = file.metadata().await?.len();
    let mut response = (
        [
            ("content-type", content_type.to_owned()),
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
    crate::auth::authorize_worker(&s.pool, &h).await?;
    let job = repository::active(&s.pool, id, &lease(&h)?).await?;
    match crate::library_scan::central_job_source(&s.pool, &s.config.input_dir, &job).await? {
        Some(source) => stream_file(&source, None, "application/octet-stream").await,
        // Source détenue par un autre worker : repli en flux par le central (ADR 0019).
        None => super::source_relay::relay_source_from_worker(&s, &job).await,
    }
}

/// Le worker source valide un billet présenté par un worker traitant (ADR 0019).
pub async fn redeem(
    State(s): State<AppState>,
    h: HeaderMap,
    axum::Json(request): axum::Json<forgearr_shared::transfer::RedeemRequest>,
) -> AppResult<axum::Json<forgearr_shared::transfer::TransferGrant>> {
    crate::auth::authorize_worker(&s.pool, &h).await?;
    Ok(axum::Json(
        repository::redeem_ticket(&s.pool, &request.token, request.worker_id).await?,
    ))
}

fn extension_for_name(name: &str) -> &str {
    std::path::Path::new(name)
        .extension()
        .and_then(|ext| ext.to_str())
        .filter(|ext| !ext.is_empty())
        .unwrap_or("bin")
}

pub async fn output(
    State(s): State<AppState>,
    Path(id): Path<Uuid>,
    _auth: super::ViewJobsAuth,
) -> AppResult<Response> {
    let name = repository::succeeded_output_name(&s.pool, id)
        .await?
        .ok_or(AppError::NotFound)?;
    let ext = extension_for_name(&name);
    let mut response = stream_file(
        &s.config.data_dir.join("output").join(&name),
        Some(&format!("job-{id}.{ext}")),
        "application/octet-stream",
    )
    .await?;
    response.headers_mut().insert(
        axum::http::header::CACHE_CONTROL,
        axum::http::HeaderValue::from_static("no-store"),
    );
    Ok(response)
}

/// Suppression explicite par un administrateur du fichier de sortie d'un job terminé.
pub async fn delete_output(
    State(s): State<AppState>,
    Path(id): Path<Uuid>,
    _auth: super::ManageJobsAuth,
) -> AppResult<StatusCode> {
    let previous = repository::clear_job_output_name(&s.pool, id).await?;
    if let Some(name) = previous
        && !name.starts_with('.')
        && !name.contains(['/', '\\', '\0'])
    {
        let path = s.config.data_dir.join("output").join(name);
        if let Err(error) = tokio::fs::remove_file(path).await
            && error.kind() != std::io::ErrorKind::NotFound
        {
            tracing::warn!(job_id = %id, error = %error, "suppression explicite du fichier de sortie impossible");
        }
    }
    Ok(StatusCode::NO_CONTENT)
}

async fn output_spec_for_job(
    s: &AppState,
    job: &Job,
    requested_ext: Option<&str>,
) -> AppResult<media::OutputSpec> {
    let frozen_flow = match (&job.flow_id, job.flow_version) {
        (Some(flow_id), Some(version)) => repository::get_flow_version(&s.pool, flow_id, version)
            .await
            .ok(),
        _ => None,
    };
    let roots = crate::plugins::catalog_roots(&s.pool, &s.config).await;
    let registry = s.catalog_cache.operations(&roots).await;
    registry
        .output_spec_for_flow(frozen_flow.as_ref(), requested_ext)
        .ok_or(AppError::Invalid)
}

pub async fn result(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(id): Path<Uuid>,
    body: Body,
) -> AppResult<StatusCode> {
    crate::auth::authorize_worker(&s.pool, &h).await?;
    let credentials = lease(&h)?;
    let job = repository::active(&s.pool, id, &credentials).await?;
    let warning = match h.get("x-remux-warning").and_then(|v| v.to_str().ok()) {
        Some("true") => true,
        Some("false") | None => false,
        _ => return Err(AppError::Invalid),
    };
    let requested_ext = match h
        .get("x-output-ext")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.trim().to_ascii_lowercase())
    {
        None => None,
        Some(ext)
            if (1..=16).contains(&ext.len()) && ext.chars().all(|c| c.is_ascii_alphanumeric()) =>
        {
            Some(ext)
        }
        Some(_) => return Err(AppError::Invalid),
    };
    let spec = output_spec_for_job(&s, &job, requested_ext.as_deref()).await?;
    let staging =
        s.config
            .data_dir
            .join("staging")
            .join(format!("{}.{}", Uuid::new_v4(), spec.extension));
    let outcome = tokio::time::timeout(
        std::time::Duration::from_secs(3600),
        receive(&s, id, &credentials, &job, body, &staging, warning, &spec),
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

#[allow(clippy::too_many_arguments)]
async fn receive(
    s: &AppState,
    id: Uuid,
    lease: &LeaseRequest,
    job: &Job,
    body: Body,
    staging: &std::path::Path,
    warning: bool,
    spec: &media::OutputSpec,
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
    let source = crate::library_scan::central_job_source(&s.pool, &s.config.input_dir, job).await?;
    publish_staged(s, id, lease, job, source.as_deref(), staging, warning, spec).await?;
    Ok(())
}

async fn validate_staged_output(
    s: &AppState,
    spec: &media::OutputSpec,
    source: &std::path::Path,
    staging: &std::path::Path,
) -> anyhow::Result<()> {
    if spec.post_validate.mode == media::PostValidateMode::None {
        return Ok(());
    }
    let roots = crate::plugins::catalog_roots(&s.pool, &s.config).await;
    let registry = s.catalog_cache.operations(&roots).await;
    let cache_mgr = crate::tools_cache::ToolCacheManager::new(&s.config.tools_cache_dir);
    let mut versions = std::collections::HashMap::new();
    for probe in registry.all_probes() {
        if let crate::flows::OperationRuntime::Binary(bin) = &probe.runtime
            && let Some(tool) = &bin.tool
        {
            versions.insert(
                tool.id.clone(),
                s.catalog_cache.resolve_tool_version(&roots, &tool.id).await,
            );
        }
    }
    crate::flows::binary_runner::post_validate_output(
        &registry,
        &cache_mgr,
        &versions,
        &spec.post_validate,
        None,
        source,
        staging,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn publish_staged(
    s: &AppState,
    id: Uuid,
    lease: &LeaseRequest,
    job: &Job,
    source: Option<&std::path::Path>,
    staging: &std::path::Path,
    warning: bool,
    spec: &media::OutputSpec,
) -> AppResult<()> {
    let (hash, size) = storage::fingerprint(staging, s.config.max_file_bytes).await?;
    if let Some(source) = source {
        let (source_hash, source_size) =
            storage::fingerprint(source, s.config.max_file_bytes).await?;
        if Some(&source_hash) != job.source_sha256.as_ref()
            || job.source_bytes != Some(source_size as i64)
        {
            return Err(AppError::Conflict);
        }
        validate_staged_output(s, spec, source, staging)
            .await
            .map_err(|_| AppError::Invalid)?;
    }
    let name = format!("{id}-{}.{}", lease.lease_token, spec.extension);
    let destination = s.config.data_dir.join("output").join(&name);
    if let Err(error) = repository::publish_succeeded(
        &s.pool,
        id,
        lease,
        repository::PublishedOutput {
            name: &name,
            sha256: &hash,
            bytes: size as i64,
            warning,
        },
        async {
            tokio::fs::hard_link(staging, &destination).await?;
            Ok(())
        },
    )
    .await
    {
        if matches!(error, AppError::Database(_)) {
            tracing::error!(job_id=%id, "result publication database failure");
        }
        return Err(error);
    }
    tracing::info!(job_id=%id, worker_id=%lease.worker_id, bytes=size, warning, "remux completed");
    if let Some(lib_id) = job.library_id
        && let Ok(lib) = repository::get_library(&s.pool, lib_id).await
    {
        let stored = repository::job_source(&s.pool, id).await;
        if lib.output_policy != crate::domain::LibraryOutputPolicy::OutputDir
            && stored.is_ok_and(|source| {
                source.is_none_or(|source| {
                    source.source_worker_id.is_none() && source.source_writable
                })
            })
            && let Ok(Some(source_path)) =
                crate::library_scan::central_job_source(&s.pool, &s.config.input_dir, job).await
        {
            let current = storage::fingerprint(&source_path, s.config.max_file_bytes).await;
            if current.is_ok_and(|(hash, bytes)| {
                job.source_sha256.as_deref() == Some(hash.as_str())
                    && job.source_bytes == Some(bytes as i64)
            }) && let Err(error) = crate::source_publication::apply_library_output_policy(
                lib.output_policy,
                &source_path,
                &destination,
                &spec.extension,
                job.source_sha256.as_deref().unwrap_or_default(),
                job.source_bytes.unwrap_or_default(),
                s.config.max_file_bytes,
            )
            .await
            {
                tracing::warn!(job_id=%id, %error, "remplacement de la source indisponible");
            }
        }
    }
    if let Err(error) =
        crate::flows::publication::notify_after_publication(s, job, lease.worker_id, &destination)
            .await
    {
        tracing::warn!(job_id=%id, error=%error, "notification après publication indisponible");
    }
    s.notify_job_event(JobEventType::JobSucceeded, id, JobStatus::Succeeded, None);
    Ok(())
}
