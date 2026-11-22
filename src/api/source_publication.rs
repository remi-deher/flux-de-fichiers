//! Routes de remise du résultat au worker détenteur de la source.

use super::AppState;
use crate::{
    domain::{Job, LeaseRequest, LibraryOutputPolicy},
    error::{AppError, AppResult},
    repository,
};
use axum::{
    Json,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
};
use forgearr_shared::transfer::{
    ArtifactKind, SourcePublicationOutcome, SourcePublicationReport, SourcePublicationTask,
    SourceTicket,
};
use serde::Deserialize;
use std::time::Duration;
use uuid::Uuid;

const MAX_PENDING_WAIT_MS: u64 = 15_000;
const PUBLICATION_TIMEOUT: Duration = Duration::from_secs(60 * 60);

/// Transmet le résultat à un détenteur inscriptible, sans le déclarer réussi avant sa réponse : il le
/// tire en direct chez le worker qui le retient avec ce billet (ADR 0022).
#[allow(clippy::too_many_arguments)]
pub(crate) async fn publish_on_holder(
    s: &AppState,
    job: &Job,
    lease: &LeaseRequest,
    holder: repository::SourceHolder,
    result_ticket: SourceTicket,
    result_hash: &str,
    result_bytes: i64,
    extension: &str,
    policy: LibraryOutputPolicy,
) -> AppResult<()> {
    if policy != LibraryOutputPolicy::OutputDir
        && (job.source_sha256.is_none() || job.source_bytes.is_none())
    {
        return Err(AppError::Invalid);
    }
    let task = SourcePublicationTask {
        request_id: Uuid::new_v4(),
        worker_id: holder.worker_id,
        root: holder.root,
        source_name: job.source_name.clone(),
        source_sha256: job.source_sha256.clone().unwrap_or_default(),
        source_bytes: job.source_bytes.unwrap_or_default(),
        result_sha256: result_hash.to_owned(),
        result_bytes,
        extension: extension.to_owned(),
        policy,
        result_ticket: Some(result_ticket),
    };
    let broker = &s.source_publication;
    let outcome = broker
        .request(task, job.id, lease.clone(), PUBLICATION_TIMEOUT)
        .await?;
    match outcome {
        SourcePublicationOutcome::Published => {
            repository::active(&s.pool, job.id, lease).await?;
            Ok(())
        }
        SourcePublicationOutcome::SourceChanged => Err(AppError::SourceChanged),
        SourcePublicationOutcome::Failed => Err(AppError::CentralUnavailable),
    }
}

#[derive(Deserialize)]
pub struct HolderQuery {
    worker_id: Uuid,
    #[serde(default)]
    wait_ms: Option<u64>,
}

pub async fn pending(
    State(s): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<HolderQuery>,
) -> AppResult<Json<Vec<SourcePublicationTask>>> {
    pending_for(&s, &headers, query, &s.source_publication).await
}

pub async fn inspection_pending(
    State(s): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<HolderQuery>,
) -> AppResult<Json<Vec<SourcePublicationTask>>> {
    pending_for(&s, &headers, query, &s.source_inspection).await
}

async fn pending_for(
    s: &AppState,
    headers: &HeaderMap,
    query: HolderQuery,
    broker: &crate::source_publication_broker::SourcePublicationBroker,
) -> AppResult<Json<Vec<SourcePublicationTask>>> {
    crate::auth::authorize_worker_identity(&s.pool, headers, query.worker_id).await?;
    let wait = Duration::from_millis(query.wait_ms.unwrap_or(0).min(MAX_PENDING_WAIT_MS));
    Ok(Json(broker.take_for_worker(query.worker_id, wait).await))
}

/// Le détenteur redemande un billet pour tirer le résultat chez le worker qui le retient (billet
/// expiré, lecture à reprendre par `Range`). Réservé au détenteur à qui la demande a été remise.
pub async fn result_ticket(
    State(s): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Query(query): Query<HolderQuery>,
) -> AppResult<Json<Option<SourceTicket>>> {
    crate::auth::authorize_worker_identity(&s.pool, &headers, query.worker_id).await?;
    let access = s
        .source_publication
        .access_for_worker(id, query.worker_id)
        .await
        .ok_or(AppError::NotFound)?;
    repository::active(&s.pool, access.job_id, &access.lease).await?;
    let ticket = repository::issue_artifact_ticket(
        &s.pool,
        access.job_id,
        ArtifactKind::Result,
        query.worker_id,
        access.lease.lease_epoch,
    )
    .await?;
    Ok(Json(ticket))
}

/// Vérifie le bail juste avant que le détenteur ne remplace la source.
pub async fn authorize_commit(
    State(s): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Query(query): Query<HolderQuery>,
) -> AppResult<StatusCode> {
    crate::auth::authorize_worker_identity(&s.pool, &headers, query.worker_id).await?;
    let access = s
        .source_publication
        .access_for_worker(id, query.worker_id)
        .await
        .ok_or(AppError::NotFound)?;
    repository::active(&s.pool, access.job_id, &access.lease).await?;
    let writable = repository::writable_source_holders(&s.pool, access.job_id, Uuid::nil())
        .await?
        .iter()
        .any(|holder| holder.worker_id == query.worker_id && holder.root == access.root);
    if !writable {
        return Err(AppError::Forbidden);
    }
    // Accord inscrit avec le contrôle du bail. Un dossier de sortie n'a qu'un détenteur, le worker
    // intégré, qui ne passe pas par ici : sans intention inscrite, rien ne se remplace.
    repository::require_publication_grant(&s.pool, access.job_id, &access.lease).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn report(
    State(s): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(report): Json<SourcePublicationReport>,
) -> AppResult<StatusCode> {
    report_for(&s, &headers, id, report, &s.source_publication).await
}

pub async fn inspection_report(
    State(s): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Json(report): Json<SourcePublicationReport>,
) -> AppResult<StatusCode> {
    report_for(&s, &headers, id, report, &s.source_inspection).await
}

async fn report_for(
    s: &AppState,
    headers: &HeaderMap,
    id: Uuid,
    report: SourcePublicationReport,
    broker: &crate::source_publication_broker::SourcePublicationBroker,
) -> AppResult<StatusCode> {
    crate::auth::authorize_worker_identity(&s.pool, headers, report.worker_id).await?;
    if id != report.request_id {
        return Err(AppError::Invalid);
    }
    let access = broker
        .access_for_worker(id, report.worker_id)
        .await
        .ok_or(AppError::NotFound)?;
    repository::active(&s.pool, access.job_id, &access.lease).await?;
    if !broker.report(report).await {
        return Err(AppError::NotFound);
    }
    Ok(StatusCode::NO_CONTENT)
}
