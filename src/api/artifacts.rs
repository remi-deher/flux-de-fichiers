//! Déclaration d'un artefact retenu par le worker qui détient le bail (ADR 0022).

use super::AppState;
use crate::{error::AppResult, repository};
use axum::{
    Json,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
};
use forgearr_shared::transfer::{ArtifactDeclaration, ArtifactKind};
use serde::Deserialize;
use uuid::Uuid;

#[derive(Deserialize)]
pub struct RetainedQuery {
    worker_id: Uuid,
    lease_epoch: i64,
}

/// Le worker annonce (ou remplace) un fichier qu'il retient pour ce job ; seul le détenteur du bail
/// peut le faire, sous la clé de son propre worker.
pub async fn declare(
    State(s): State<AppState>,
    headers: HeaderMap,
    Path((job, kind)): Path<(Uuid, ArtifactKind)>,
    Json(declaration): Json<ArtifactDeclaration>,
) -> AppResult<StatusCode> {
    let lease = super::transfers::lease(&headers)?;
    crate::auth::authorize_worker_identity(&s.pool, &headers, lease.worker_id).await?;
    repository::declare_artifact(
        &s.pool,
        job,
        &lease,
        kind,
        &declaration,
        s.config.max_file_bytes,
    )
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Le repreneur confirme avoir tiré et vérifié le fichier de travail : son détenteur peut le
/// purger. La confirmation est répétable.
pub async fn received(
    State(s): State<AppState>,
    headers: HeaderMap,
    Path(job): Path<Uuid>,
) -> AppResult<StatusCode> {
    let lease = super::transfers::lease(&headers)?;
    crate::auth::authorize_worker_identity(&s.pool, &headers, lease.worker_id).await?;
    repository::confirm_artifact_received(&s.pool, job, &lease).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Le détenteur doit-il encore retenir ce fichier ? Il purge sa copie quand la réponse est `false`
/// (réception confirmée, résultat publié, tâche terminée, échouée ou reprise depuis la source).
pub async fn retained(
    State(s): State<AppState>,
    headers: HeaderMap,
    Path((job, kind)): Path<(Uuid, ArtifactKind)>,
    Query(query): Query<RetainedQuery>,
) -> AppResult<Json<bool>> {
    let RetainedQuery {
        worker_id,
        lease_epoch,
    } = query;
    crate::auth::authorize_worker_identity(&s.pool, &headers, worker_id).await?;
    let keep = repository::artifact_retained(&s.pool, job, kind, worker_id, lease_epoch).await?;
    Ok(Json(keep))
}
