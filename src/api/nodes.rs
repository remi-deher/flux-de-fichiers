//! Enrôlement des nœuds : la centrale crée la clé d'un worker, un nœud non installé devient worker.

use super::{AppState, session};
use crate::{
    config, enrollment,
    error::{AppError, AppResult},
    node, node_settings, repository, web_session,
};
use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};
use flux_shared::{NodeKind, Role};
use serde::{Deserialize, Serialize};

/// État public du nœud, identique pour la centrale et le worker (sondes, supervision).
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeStatus {
    node_kind: NodeKind,
    version: &'static str,
    installed: bool,
}

pub async fn status(State(state): State<AppState>) -> AppResult<Json<NodeStatus>> {
    Ok(Json(NodeStatus {
        node_kind: NodeKind::Central,
        version: env!("CARGO_PKG_VERSION"),
        installed: repository::account_count(&state.pool).await? > 0,
    }))
}

/// Crée la clé d'API d'un nouveau nœud worker. Réservé aux identifiants d'un administrateur ; la clé
/// n'est renvoyée qu'ici. Pas de cookie : l'appel vient du worker, jamais d'un navigateur.
pub async fn enroll(
    State(state): State<AppState>,
    Json(request): Json<enrollment::EnrollRequest>,
) -> AppResult<Response> {
    if !node_settings::is_valid_node_name(&request.node_name) {
        return Err(AppError::Invalid);
    }
    let account =
        session::verify_credentials(&state.pool, &request.username, request.password).await?;
    if account.role != Role::Admin {
        return Err(AppError::Forbidden);
    }
    let issued = crate::api_keys::issue(&state.pool, &request.node_name, Role::Worker).await?;
    let body = Json(enrollment::EnrolledNode { key: issued.raw });
    Ok(([(header::CACHE_CONTROL, "no-store")], body).into_response())
}

#[derive(Deserialize)]
pub struct SetupWorkerRequest {
    coordinator_url: String,
    username: String,
    password: String,
    node_name: String,
}

/// Transforme un nœud non installé en worker : enrôlement auprès de la centrale, rôle conservé dans
/// `/config`, puis redémarrage interne en worker. Une fois la centrale installée ou le rôle choisi : 409.
pub async fn setup_worker(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<SetupWorkerRequest>,
) -> AppResult<StatusCode> {
    web_session::validate_origin(&headers, &state.config)?;
    let config_dir = &state.config.config_dir;
    let already_worker = node_settings::load_worker(config_dir)
        .map_err(|error| {
            tracing::error!(error = %error, "rôle du nœud illisible");
            AppError::Internal
        })?
        .is_some();
    if already_worker || repository::account_count(&state.pool).await? > 0 {
        return Err(AppError::AlreadyConfigured);
    }
    if !node_settings::is_valid_node_name(&request.node_name) {
        return Err(AppError::Invalid);
    }
    let coordinator =
        config::coordinator_url(request.coordinator_url.trim()).map_err(|_| AppError::Invalid)?;
    let key = enrollment::request_key(
        &coordinator,
        &enrollment::EnrollRequest {
            username: request.username.trim().to_owned(),
            password: request.password,
            node_name: request.node_name.clone(),
        },
    )
    .await?;
    node_settings::save_worker(
        config_dir,
        &node_settings::WorkerNodeSettings {
            name: request.node_name,
            coordinator_url: coordinator.as_str().trim_end_matches('/').to_owned(),
        },
        &key,
    )
    .map_err(|error| {
        tracing::error!(error = %error, "rôle worker impossible à conserver");
        AppError::Internal
    })?;
    node::request_role_switch();
    Ok(StatusCode::ACCEPTED)
}
