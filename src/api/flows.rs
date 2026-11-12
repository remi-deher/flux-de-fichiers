//! Gestionnaires HTTP pour l'API REST des flux de traitement (Flows).

use super::{AdminAuth, AppState};
use crate::{
    domain::UpdateFlowRequest,
    error::{AppError, AppResult},
    repository,
};
use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};

/// Liste l'ensemble des flux configurés en base de données.
pub async fn list_flows(
    State(s): State<AppState>,
    _auth: AdminAuth,
) -> AppResult<Json<Vec<crate::domain::FlowRecord>>> {
    Ok(Json(repository::list_flows(&s.pool).await?))
}

/// Récupère la définition détaillée d'un flux spécifique.
pub async fn get_flow(
    State(s): State<AppState>,
    Path(id): Path<String>,
    _auth: AdminAuth,
) -> AppResult<Json<crate::domain::FlowRecord>> {
    Ok(Json(repository::get_flow(&s.pool, &id).await?))
}

/// Enregistre ou met à jour la définition d'un flux.
pub async fn save_flow(
    State(s): State<AppState>,
    Path(id): Path<String>,
    _auth: AdminAuth,
    Json(payload): Json<UpdateFlowRequest>,
) -> AppResult<(StatusCode, Json<crate::domain::FlowRecord>)> {
    if payload.name.trim().is_empty() {
        return Err(AppError::Invalid);
    }
    let flow = repository::save_flow(&s.pool, &id, &payload).await?;
    Ok((StatusCode::OK, Json(flow)))
}

/// Supprime un flux personnalisé (interdit sur le flux actif par défaut).
pub async fn delete_flow(
    State(s): State<AppState>,
    Path(id): Path<String>,
    _auth: AdminAuth,
) -> AppResult<StatusCode> {
    repository::delete_flow(&s.pool, &id).await?;
    Ok(StatusCode::NO_CONTENT)
}
