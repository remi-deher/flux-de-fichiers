use super::AppState;
use crate::{
    domain::{Job, JobEventType, JobStatus, LeaseRequest},
    error::{AppError, AppResult},
    library_scan::central_writable_job_source,
    media, repository, storage,
};
use axum::{
    body::Body,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use tokio_util::io::ReaderStream;
use uuid::Uuid;

pub(super) fn lease(h: &HeaderMap) -> AppResult<LeaseRequest> {
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

/// Le worker traitant redemande un billet frais (expiré, ou lecture à reprendre par `Range`).
/// Réponse `null` quand la source se lit autrement (montage du worker, ou aucun détenteur joignable).
pub async fn reissue_ticket(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(id): Path<Uuid>,
) -> AppResult<axum::Json<Option<forgearr_shared::transfer::SourceTicket>>> {
    let credentials = lease(&h)?;
    crate::auth::authorize_worker_identity(&s.pool, &h, credentials.worker_id).await?;
    Ok(axum::Json(
        repository::reissue_source_ticket(&s.pool, id, &credentials).await?,
    ))
}

/// Le worker source valide un billet présenté par un worker traitant (ADR 0019).
pub async fn redeem(
    State(s): State<AppState>,
    h: HeaderMap,
    axum::Json(request): axum::Json<forgearr_shared::transfer::RedeemRequest>,
) -> AppResult<axum::Json<forgearr_shared::transfer::TransferGrant>> {
    crate::auth::authorize_worker_identity(&s.pool, &h, request.worker_id).await?;
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
    let name = repository::downloadable_output_name(&s.pool, id)
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
    if !storage::valid_output_extension(&spec.extension) {
        return Err(AppError::Invalid);
    }
    let (hash, size) = storage::fingerprint(staging, s.config.max_file_bytes).await?;
    if let Some(source) = source {
        crate::source_publication::verify_source(
            source,
            job.source_sha256.as_deref().ok_or(AppError::Invalid)?,
            job.source_bytes.ok_or(AppError::Invalid)?,
            s.config.max_file_bytes,
        )
        .await?;
        validate_staged_output(s, spec, source, staging)
            .await
            .map_err(|_| AppError::Invalid)?;
    }
    let name = storage::output_file_name(id, lease.lease_token, &spec.extension);
    let destination = s.config.data_dir.join("output").join(&name);
    let mut output_prepared = false;
    if let Some(library_id) = job.library_id {
        let library = repository::get_library(&s.pool, library_id).await?;
        if library.output_policy != crate::domain::LibraryOutputPolicy::OutputDir {
            // Seule une source que le central voit s'applique ici ; sinon un autre worker la détient
            // et tire le résultat chez le worker qui le retient (ADR 0022).
            let central_source = central_writable_job_source(&s.pool, &s.config.input_dir, job)
                .await
                .map_err(crate::source_publication::missing_source_is_changed)?
                .ok_or(AppError::ResultUndeliverable)?;
            // Écarter une erreur de stockage centrale avant de modifier la source.
            // La copie reste disponible si le résultat du remplacement devient incertain.
            repository::active(&s.pool, id, lease).await?;
            tokio::fs::hard_link(staging, &destination).await?;
            let prepared = async {
                tokio::fs::File::open(s.config.data_dir.join("output"))
                    .await?
                    .sync_all()
                    .await?;
                repository::begin_source_publication(
                    &s.pool,
                    id,
                    lease,
                    repository::PublishedOutput {
                        name: &name,
                        sha256: &hash,
                        bytes: i64::try_from(size).map_err(|_| AppError::TooLarge)?,
                        warning,
                    },
                    library.output_policy,
                )
                .await
            }
            .await;
            if let Err(error) = prepared {
                let _ = tokio::fs::remove_file(&destination).await;
                return Err(error);
            }
            output_prepared = true;
            let replacement = crate::source_publication::apply_library_output_policy(
                library.output_policy,
                &central_source,
                staging,
                &spec.extension,
                job.source_sha256.as_deref().ok_or(AppError::Invalid)?,
                job.source_bytes.ok_or(AppError::Invalid)?,
                s.config.max_file_bytes,
                repository::require_publication_grant(&s.pool, id, lease),
            )
            .await;
            if let Err(error) = replacement {
                return Err(abandon_publication(s, job, lease, &name, error).await);
            }
        }
    }
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
            if !output_prepared {
                tokio::fs::hard_link(staging, &destination).await?;
            }
            Ok(())
        },
    )
    .await
    {
        if output_prepared {
            mark_publication_uncertain(s, id, lease.lease_epoch).await;
        }
        if matches!(error, AppError::Database(_)) {
            tracing::error!(job_id=%id, "result publication database failure");
        }
        return Err(error);
    }
    tracing::info!(job_id=%id, worker_id=%lease.worker_id, bytes=size, warning, "remux completed");
    if let Err(error) =
        crate::flows::publication::notify_after_publication(s, job, lease.worker_id, &destination)
            .await
    {
        tracing::warn!(job_id=%id, error=%error, "notification après publication indisponible");
    }
    s.notify_job_event(JobEventType::JobSucceeded, id, JobStatus::Succeeded, None);
    Ok(())
}

/// Referme le journal d'un remplacement qui n'a pas abouti : une source modifiée a été refusée
/// avant tout remplacement, et sans accord rien n'a pu être remplacé (le journal et la copie centrale
/// `output_name` éventuelle sont retirés) ; toute autre issue reste incertaine et garde la copie.
pub(super) async fn abandon_publication(
    s: &AppState,
    job: &Job,
    lease: &LeaseRequest,
    output_name: &str,
    error: AppError,
) -> AppError {
    let key = (job.id, lease.lease_epoch);
    if matches!(error, AppError::SourceChanged) {
        if let Err(uncleared) = repository::clear_source_publication(&s.pool, key.0, key.1).await {
            return uncleared;
        }
        discard_intent(s, key, output_name).await;
        return error;
    }
    match drop_unauthorized(s, key, output_name).await {
        Ok(true) => return error,
        Ok(false) => {}
        Err(unread) => {
            tracing::warn!(job_id=%job.id, error=%unread, "accord de publication illisible");
        }
    }
    mark_publication_uncertain(s, job.id, lease.lease_epoch).await;
    error
}

/// Écarte une intention jamais autorisée (rien n'a été remplacé). Le titulaire annoncera de nouveau
/// son résultat ou déclarera un échec ordinaire ; sans titulaire, la tâche est conclue aussitôt.
pub(super) async fn drop_unauthorized(
    s: &AppState,
    key: repository::LeaseKey,
    output_name: &str,
) -> AppResult<bool> {
    if !repository::abandon_unauthorized_publication(&s.pool, key.0, key.1).await? {
        return Ok(false);
    }
    tracing::info!(job_id=%key.0, "publication jamais autorisée écartée");
    discard_intent(s, key, output_name).await;
    Ok(true)
}

/// Suites d'une intention retirée : copie centrale supprimée, bail orphelin conclu.
async fn discard_intent(s: &AppState, key: repository::LeaseKey, output_name: &str) {
    if storage::valid_name(output_name) {
        let copy = s.config.data_dir.join("output").join(output_name);
        let _ = tokio::fs::remove_file(copy).await;
    }
    match repository::end_orphaned_lease(&s.pool, key).await {
        Ok(change) => super::leases::notify_lease_changes(s, change.as_slice()),
        // L'échéance conclura.
        Err(error) => tracing::warn!(job_id=%key.0, %error, "bail orphelin non conclu"),
    }
}

pub(super) async fn mark_publication_uncertain(s: &AppState, job: Uuid, epoch: i64) {
    match repository::mark_source_publication_uncertain(&s.pool, job, epoch).await {
        Ok(Some(code)) => {
            s.notify_job_event(JobEventType::JobFailed, job, JobStatus::Failed, Some(code))
        }
        Ok(None) => {}
        Err(error) => {
            tracing::error!(job_id=%job, error=%error, "failed to record uncertain source publication")
        }
    }
}

#[cfg(test)]
#[path = "transfers_tests.rs"]
pub(crate) mod tests;
