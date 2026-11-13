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
    let mut flows = repository::list_flows(&s.pool).await?;
    for flow in &mut flows {
        flow.definition = crate::flows::mask_definition(&flow.definition);
    }
    Ok(Json(flows))
}

/// Récupère la définition détaillée d'un flux spécifique.
pub async fn get_flow(
    State(s): State<AppState>,
    Path(id): Path<String>,
    _auth: AdminAuth,
) -> AppResult<Json<crate::domain::FlowRecord>> {
    let mut flow = repository::get_flow(&s.pool, &id).await?;
    flow.definition = crate::flows::mask_definition(&flow.definition);
    Ok(Json(flow))
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
    let mut payload = payload;
    if crate::flows::has_webhook_headers(&payload.definition) {
        let existing = repository::get_flow(&s.pool, &id)
            .await
            .ok()
            .map(|flow| flow.definition);
        let secrets = crate::secrets::SecretBox::open(&s.config.config_dir)?;
        payload.definition =
            crate::flows::seal_definition(&secrets, &id, &payload.definition, existing.as_deref())?;
    }
    let mut flow = repository::save_flow(&s.pool, &id, &payload).await?;
    flow.definition = crate::flows::mask_definition(&flow.definition);
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
