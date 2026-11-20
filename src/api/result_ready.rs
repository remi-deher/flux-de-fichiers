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
    library_scan::central_writable_job_root,
    repository::{self, PublishedOutput, SourceHolder},
    storage,
};
use axum::{
    Json,
    extract::{Path, State},
    http::HeaderMap,
};
use forgearr_shared::transfer::{ArtifactKind, ResultReady, ResultReadyResponse};
use uuid::Uuid;

/// Politique et détenteur quand le résultat revient à un détenteur de la source : le worker intégré
/// pour une source que le central voit (s'il est actif), sinon le détenteur distant inscriptible ;
/// dans tous les cas un autre worker que celui qui traite, avec une politique de remplacement.
/// `None` pour les autres cas, qui gardent l'envoi au central.
async fn pulled_destination(
    s: &AppState,
    job: &Job,
    processor: Uuid,
) -> AppResult<Option<(LibraryOutputPolicy, SourceHolder)>> {
    let Some(library_id) = job.library_id else {
        return Ok(None);
    };
    let library = repository::get_library(&s.pool, library_id).await?;
    let policy = library.output_policy;
    if policy == LibraryOutputPolicy::OutputDir {
        return Ok(None);
    }
    let central = central_writable_job_root(&s.pool, &s.config.input_dir, job)
        .await
        .map_err(crate::source_publication::missing_source_is_changed)?;
    let local = s.local_worker_id.get().copied();
    let holder = match central {
        Some(root) => local.map(|worker_id| SourceHolder {
            worker_id,
            root: root.to_string_lossy().into_owned(),
            transfer_url: None,
        }),
        None => repository::writable_source_holders(&s.pool, job.id, Uuid::nil())
            .await?
            .into_iter()
            .next(),
    };
    let remote = holder.filter(|holder| holder.worker_id != processor);
    Ok(remote.map(|holder| (policy, holder)))
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
    let Some((policy, holder)) = pulled_destination(s, job, lease.worker_id).await? else {
        // Ce cas garde l'envoi au central : le fichier retenu n'a plus d'objet.
        repository::release_artifact(&s.pool, job.id, ArtifactKind::Result).await?;
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
        ResultSource::Pulled(holder),
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
