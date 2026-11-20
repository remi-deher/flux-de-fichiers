//! Publication d'un résultat retenu chez le worker qui l'a produit (ADR 0022) : le détenteur de la
//! bibliothèque le tire en direct, le central n'en tient aucune copie.

use super::{AppState, source_publication::publish_on_holder, transfers};
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
    http::{HeaderMap, StatusCode},
};
use forgearr_shared::transfer::{
    ArtifactDeclaration, ArtifactKind, LocalApplication, ResultReady, ResultReadyResponse,
    SourcePublicationOutcome,
};
use uuid::Uuid;

/// Politique et détenteur quand le résultat revient à un détenteur, toujours un autre worker que
/// celui qui traite : pour un dossier de sortie du central (ou une tâche sans bibliothèque), le
/// worker intégré s'il est actif ; pour une politique de remplacement, le worker intégré si le
/// central voit la source, sinon le détenteur distant inscriptible.
/// `None` pour les autres cas, qui gardent l'envoi au central.
pub(crate) async fn pulled_destination(
    s: &AppState,
    job: &Job,
    processor: Uuid,
) -> AppResult<Option<(LibraryOutputPolicy, SourceHolder)>> {
    let policy = match job.library_id {
        Some(library_id) => {
            repository::get_library(&s.pool, library_id)
                .await?
                .output_policy
        }
        None => LibraryOutputPolicy::OutputDir,
    };
    if policy == LibraryOutputPolicy::OutputDir {
        let output_root = s.config.data_dir.join("output");
        let holder = s.local_worker_id.get().map(|worker_id| SourceHolder {
            worker_id: *worker_id,
            root: output_root.to_string_lossy().into_owned(),
            transfer_url: None,
        });
        let remote = holder.filter(|holder| holder.worker_id != processor);
        return Ok(remote.map(|holder| (policy, holder)));
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

/// Le résultat que ce bail a déclaré, avec une extension de sortie valide.
async fn declared_result(
    s: &AppState,
    job: &Job,
    lease: &LeaseRequest,
    ready: &ResultReady,
) -> AppResult<ArtifactDeclaration> {
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
    Ok(artifact.declaration)
}

/// Marque la tâche réussie une fois le résultat appliqué, libère le fichier retenu et notifie.
async fn complete_publication(
    s: &AppState,
    job: &Job,
    lease: &LeaseRequest,
    output: PublishedOutput<'_>,
    notified_path: &std::path::Path,
) -> AppResult<()> {
    if let Err(error) =
        repository::publish_succeeded(&s.pool, job.id, lease, output, async { Ok(()) }).await
    {
        transfers::mark_publication_uncertain(s, job.id, lease.lease_epoch).await;
        return Err(error);
    }
    repository::release_artifact(&s.pool, job.id, ArtifactKind::Result).await?;
    tracing::info!(job_id=%job.id, worker_id=%lease.worker_id, bytes=output.bytes, "remux completed");
    let notified =
        crate::flows::publication::notify_after_publication(s, job, lease.worker_id, notified_path)
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
    Ok(())
}

/// Remplacement que le worker applique lui-même : il détient, sur son montage, la source d'une
/// bibliothèque à politique de remplacement que le central ne voit pas.
async fn local_application(
    s: &AppState,
    job: &Job,
    processor: Uuid,
) -> AppResult<Option<LocalApplication>> {
    let Some(library_id) = job.library_id else {
        return Ok(None);
    };
    let policy = repository::get_library(&s.pool, library_id)
        .await?
        .output_policy;
    if policy == LibraryOutputPolicy::OutputDir
        || job.source_sha256.is_none()
        || job.source_bytes.is_none()
    {
        return Ok(None);
    }
    let central = central_writable_job_root(&s.pool, &s.config.input_dir, job)
        .await
        .map_err(crate::source_publication::missing_source_is_changed)?;
    if central.is_some() {
        return Ok(None);
    }
    let holders = repository::writable_source_holders(&s.pool, job.id, Uuid::nil()).await?;
    let own = holders.into_iter().find(|h| h.worker_id == processor);
    Ok(own.map(|holder| LocalApplication {
        policy,
        root: holder.root,
    }))
}

/// Inscrit au journal le remplacement que le worker va appliquer lui-même ; `None` quand ce cas ne
/// s'applique pas (le résultat est alors tiré par un autre détenteur ou envoyé au central).
pub(crate) async fn begin_local_application(
    s: &AppState,
    job: &Job,
    lease: &LeaseRequest,
    ready: &ResultReady,
) -> AppResult<Option<LocalApplication>> {
    let declared = declared_result(s, job, lease, ready).await?;
    let Some(local) = local_application(s, job, lease.worker_id).await? else {
        return Ok(None);
    };
    let output = PublishedOutput {
        name: "",
        sha256: &declared.sha256,
        bytes: declared.bytes,
        warning: ready.warning,
    };
    let extension = ready.extension.as_str();
    let policy = local.policy;
    repository::begin_local_publication(&s.pool, job.id, lease, output, extension, policy).await?;
    Ok(Some(local))
}

/// Remplacement inscrit au journal pour ce bail, en attente de l'application par le worker.
async fn pending_application(
    s: &AppState,
    job: &Job,
    lease: &LeaseRequest,
) -> AppResult<repository::PendingSourcePublication> {
    let pending = repository::pending_source_publications(&s.pool).await?;
    pending
        .into_iter()
        .find(|p| p.job_id == job.id && p.lease_epoch == lease.lease_epoch)
        .filter(|p| p.applied_locally)
        .ok_or(AppError::Conflict)
}

/// Le worker confirme l'issue du remplacement qu'il a appliqué (ADR 0022, point de commit).
pub(crate) async fn finish_local_application(
    s: &AppState,
    job: &Job,
    lease: &LeaseRequest,
    outcome: SourcePublicationOutcome,
) -> AppResult<()> {
    let pending = pending_application(s, job, lease).await?;
    match outcome {
        SourcePublicationOutcome::Published => {
            let output = PublishedOutput {
                name: &pending.output_name,
                sha256: &pending.output_sha256,
                bytes: pending.output_bytes,
                warning: pending.output_warning,
            };
            let path = std::path::Path::new(&job.source_name);
            complete_publication(s, job, lease, output, path).await
        }
        SourcePublicationOutcome::SourceChanged => {
            Err(transfers::abandon_publication(s, job, lease, AppError::SourceChanged).await)
        }
        SourcePublicationOutcome::Failed => {
            Err(transfers::abandon_publication(s, job, lease, AppError::CentralUnavailable).await)
        }
    }
}

/// Publie le résultat que le worker retient : le détenteur de la bibliothèque le tire en direct,
/// applique la politique de sortie, puis le central marque la tâche réussie.
///
/// `ResultUndeliverable` quand aucun détenteur actif ne peut le tirer : le résultat ne passe
/// jamais par le central.
pub(crate) async fn publish_pulled(
    s: &AppState,
    job: &Job,
    lease: &LeaseRequest,
    ready: &ResultReady,
) -> AppResult<()> {
    let declared = declared_result(s, job, lease, ready).await?;
    let Some((policy, holder)) = pulled_destination(s, job, lease.worker_id).await? else {
        tracing::warn!(job_id=%job.id, "aucun détenteur actif pour recevoir le résultat");
        return Err(AppError::ResultUndeliverable);
    };
    let kind = ArtifactKind::Result;
    let ticket = repository::issue_artifact_ticket(
        &s.pool,
        job.id,
        kind,
        holder.worker_id,
        lease.lease_epoch,
    )
    .await?
    .ok_or(AppError::ResultUndeliverable)?;
    let declared = &declared;
    let in_output_dir = policy == LibraryOutputPolicy::OutputDir;
    // Un résultat remplacé chez le détenteur n'a aucun fichier au central : nom vide.
    let output_name = if in_output_dir {
        storage::output_file_name(job.id, lease.lease_token, &ready.extension)
    } else {
        String::new()
    };
    let output = PublishedOutput {
        name: &output_name,
        sha256: &declared.sha256,
        bytes: declared.bytes,
        warning: ready.warning,
    };
    if !in_output_dir {
        let extension = ready.extension.as_str();
        repository::begin_pulled_publication(&s.pool, job.id, lease, output, extension, policy)
            .await?;
    }
    let published = publish_on_holder(
        s,
        job,
        lease,
        holder,
        ticket,
        &declared.sha256,
        declared.bytes,
        &ready.extension,
        policy,
    )
    .await;
    match published {
        Ok(()) => {}
        Err(error) if in_output_dir => {
            let output_file = s.config.data_dir.join("output").join(&output_name);
            let _ = tokio::fs::remove_file(output_file).await;
            return Err(error);
        }
        Err(error) => return Err(transfers::abandon_publication(s, job, lease, error).await),
    }
    let notified_path = if in_output_dir {
        s.config.data_dir.join("output").join(&output_name)
    } else {
        std::path::PathBuf::from(&job.source_name)
    };
    complete_publication(s, job, lease, output, &notified_path).await
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
    if ready.can_apply_locally
        && let Some(local) = begin_local_application(&s, &job, &lease, &ready).await?
    {
        return Ok(Json(ResultReadyResponse {
            apply_locally: Some(local),
        }));
    }
    publish_pulled(&s, &job, &lease, &ready).await?;
    Ok(Json(ResultReadyResponse {
        apply_locally: None,
    }))
}

/// Dernière vérification du bail, juste avant que le worker ne remplace la source.
pub async fn authorize_application(
    State(s): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> AppResult<StatusCode> {
    let lease = transfers::lease(&headers)?;
    crate::auth::authorize_worker_identity(&s.pool, &headers, lease.worker_id).await?;
    let job = repository::active(&s.pool, id, &lease).await?;
    pending_application(&s, &job, &lease).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Le worker annonce l'issue du remplacement qu'il vient d'appliquer.
pub async fn applied(
    State(s): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(outcome): Json<SourcePublicationOutcome>,
) -> AppResult<StatusCode> {
    let lease = transfers::lease(&headers)?;
    crate::auth::authorize_worker_identity(&s.pool, &headers, lease.worker_id).await?;
    let job = repository::active(&s.pool, id, &lease).await?;
    finish_local_application(&s, &job, &lease, outcome).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
#[path = "result_ready_tests.rs"]
mod tests;
