//! Publication d'un résultat retenu chez le worker qui l'a produit (ADR 0022) : le détenteur de la
//! bibliothèque le tire en direct, le central n'en tient aucune copie.

use super::{
    AppState,
    source_publication::{ResultSource, publish_on_holder},
    transfers,
};
use crate::{
    domain::{Job, JobEventType, JobStatus, LeaseRequest, LibraryOutputPolicy},
    error::{AppError, AppResult},
    library_scan::central_writable_job_source,
    repository::{self, PublishedOutput},
    storage,
};
use axum::{
    Json,
    extract::{Path, State},
    http::HeaderMap,
};
use forgearr_shared::transfer::{ArtifactKind, ResultReady, ResultReadyResponse};
use uuid::Uuid;

/// Politique de sortie quand le résultat revient à un détenteur distant de la source : source
/// tenue par un autre worker que celui qui traite, inscriptible, politique de remplacement.
/// `None` pour les autres cas, qui gardent l'envoi au central.
async fn pulled_policy(
    s: &AppState,
    job: &Job,
    processor: Uuid,
) -> AppResult<Option<LibraryOutputPolicy>> {
    let Some(library_id) = job.library_id else {
        return Ok(None);
    };
    let library = repository::get_library(&s.pool, library_id).await?;
    if library.output_policy == LibraryOutputPolicy::OutputDir {
        return Ok(None);
    }
    let local = central_writable_job_source(&s.pool, &s.config.input_dir, job)
        .await
        .map_err(crate::source_publication::missing_source_is_changed)?;
    if local.is_some() {
        return Ok(None);
    }
    let holders = repository::writable_source_holders(&s.pool, job.id, Uuid::nil()).await?;
    let remote = holders
        .first()
        .is_some_and(|holder| holder.worker_id != processor);
    Ok(remote.then_some(library.output_policy))
}

/// Publie le résultat que le worker retient : le détenteur de la bibliothèque le tire en direct,
/// applique la politique de sortie, puis le central marque la tâche réussie.
///
/// `false` quand ce cas n'est pas pris en charge : le worker envoie alors le résultat au central.
pub(crate) async fn publish_pulled(
    s: &AppState,
    job: &Job,
    lease: &LeaseRequest,
    ready: &ResultReady,
) -> AppResult<bool> {
    if !storage::valid_output_extension(&ready.extension) {
        return Err(AppError::Invalid);
    }
    let artifact = repository::get_artifact(&s.pool, job.id, ArtifactKind::Result)
        .await?
        .filter(|artifact| {
            artifact.holder_worker_id == lease.worker_id
                && artifact.lease_epoch == lease.lease_epoch
        })
        .ok_or(AppError::Invalid)?;
    let Some(policy) = pulled_policy(s, job, lease.worker_id).await? else {
        return Ok(false);
    };
    let declared = &artifact.declaration;
    let output = PublishedOutput {
        name: &declared.name,
        sha256: &declared.sha256,
        bytes: declared.bytes,
        warning: ready.warning,
    };
    repository::begin_source_publication(&s.pool, job.id, lease, output, policy).await?;
    let published = publish_on_holder(
        s,
        job,
        lease,
        ResultSource::Pulled,
        &declared.sha256,
        declared.bytes,
        &ready.extension,
        policy,
    )
    .await;
    match published {
        Ok(()) => {}
        Err(AppError::SourceChanged) => {
            repository::clear_source_publication(&s.pool, job.id, lease.lease_epoch).await?;
            return Err(AppError::SourceChanged);
        }
        Err(error) => {
            transfers::mark_publication_uncertain(s, job.id, lease.lease_epoch).await;
            return Err(error);
        }
    }
    if let Err(error) =
        repository::publish_succeeded(&s.pool, job.id, lease, output, async { Ok(()) }).await
    {
        transfers::mark_publication_uncertain(s, job.id, lease.lease_epoch).await;
        return Err(error);
    }
    repository::release_artifact(&s.pool, job.id, ArtifactKind::Result).await?;
    tracing::info!(job_id=%job.id, worker_id=%lease.worker_id, bytes=declared.bytes, "remux completed");
    let notified = crate::flows::publication::notify_after_publication(
        s,
        job,
        lease.worker_id,
        std::path::Path::new(&declared.name),
    )
    .await;
    if let Err(error) = notified {
        tracing::warn!(job_id=%job.id, error=%error, "notification après publication indisponible");
    }
    s.notify_job_event(
        JobEventType::JobSucceeded,
        job.id,
        JobStatus::Succeeded,
        None,
    );
    Ok(true)
}

/// Le worker annonce que son résultat est prêt et retenu chez lui (bail et clé de son worker).
pub async fn ready(
    State(s): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(ready): Json<ResultReady>,
) -> AppResult<Json<ResultReadyResponse>> {
    let lease = transfers::lease(&headers)?;
    crate::auth::authorize_worker_identity(&s.pool, &headers, lease.worker_id).await?;
    let job = repository::active(&s.pool, id, &lease).await?;
    let pulled = publish_pulled(&s, &job, &lease, &ready).await?;
    Ok(Json(ResultReadyResponse { pulled }))
}

#[cfg(test)]
#[path = "result_ready_tests.rs"]
mod tests;
