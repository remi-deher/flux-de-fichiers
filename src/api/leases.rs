//! Baux (ADR 0027) : battement, restitution, accord avant une action irréversible, état injoignable.

use super::{AppState, ViewJobsAuth, jobs::RelayInfoQuery, source_recovery_remote};
use crate::{
    domain::{AuthorizeAction, IrreversibleAction, JobEventType, JobStatus, LeaseRequest},
    error::{AppError, AppResult},
    repository::{self, LeaseChange},
};
use axum::{
    Json,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
};
use forgearr_shared::jobs::{JobLeaseInfo, UNREACHABLE_TIMEOUT_HEADER};
use uuid::Uuid;

/// Notifie les changements de bail ; une publication laissée sans titulaire est vérifiée aussitôt.
pub(crate) fn notify_lease_changes(s: &AppState, changes: &[LeaseChange]) {
    for change in changes {
        tracing::info!(job_id=%change.job_id, event=%change.event, "changement de bail");
        s.notify_job_event(
            change.event,
            change.job_id,
            change.status,
            change.error_code,
        );
        if change.deferred {
            tokio::spawn(source_recovery_remote::recover_deferred(
                s.clone(),
                change.job_id,
            ));
        }
    }
}

/// Renouvelle le bail et rend le délai d'injoignabilité courant, sur lequel le worker règle sa patience.
/// `409 lease_lost` : le central l'a repris, refus définitif.
pub async fn heartbeat(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(id): Path<Uuid>,
    Json(r): Json<LeaseRequest>,
) -> AppResult<impl IntoResponse> {
    crate::auth::authorize_worker(&s.pool, &h).await?;
    let restored = repository::heartbeat(&s.pool, id, &r).await?;
    let event = if restored {
        JobEventType::JobRestored
    } else {
        JobEventType::JobHeartbeat
    };
    s.notify_job_event(event, id, JobStatus::Running, None);
    let timeout = crate::runtime_settings::load_unreachable_timeout(&s.pool).await;
    let header = [(UNREACHABLE_TIMEOUT_HEADER, timeout.to_string())];
    Ok((StatusCode::NO_CONTENT, header))
}

/// Le worker rend sa tâche, processus déjà arrêté. `409` pendant une publication.
pub async fn release(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(id): Path<Uuid>,
    Json(r): Json<LeaseRequest>,
) -> AppResult<StatusCode> {
    crate::auth::authorize_worker_identity(&s.pool, &h, r.worker_id).await?;
    if s.publications.contains((id, r.lease_epoch)) {
        return Err(AppError::Conflict);
    }
    let change = repository::release_lease(&s.pool, id, &r).await?;
    notify_lease_changes(&s, &[change]);
    Ok(StatusCode::NO_CONTENT)
}

/// Accord avant une action irréversible, inscrit durablement avant la réponse.
pub async fn authorize_action(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(id): Path<Uuid>,
    Json(r): Json<AuthorizeAction>,
) -> AppResult<StatusCode> {
    crate::auth::authorize_worker_identity(&s.pool, &h, r.worker_id).await?;
    authorize(&s, id, &r).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Accord d'une action ; partagé avec le worker intégré.
pub(crate) async fn authorize(s: &AppState, id: Uuid, request: &AuthorizeAction) -> AppResult<()> {
    // Une publication vers le dossier de sortie n'a pas de journal : seule la carte la connaît.
    if request.action == IrreversibleAction::FailureFlow
        && s.publications.contains((id, request.lease_epoch))
    {
        return Err(AppError::Conflict);
    }
    repository::authorize_action(&s.pool, id, request).await
}

/// État des baux des tâches listées, pour l'interface (200 identifiants au plus).
pub async fn lease_infos(
    State(s): State<AppState>,
    Query(query): Query<RelayInfoQuery>,
    _auth: ViewJobsAuth,
) -> AppResult<Json<Vec<JobLeaseInfo>>> {
    Ok(Json(
        repository::lease_infos(&s.pool, &query.parse_ids()?).await?,
    ))
}
