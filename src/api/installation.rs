//! Enrôlement des workers : le central crée la clé d'un worker, une installation vierge devient worker.

use super::{AppState, session};
use crate::{
    config, enrollment,
    error::{AppError, AppResult},
    instance, repository, web_session, worker_settings,
};
use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};
pub use flux_shared::system::SetupWorkerRequest;
use flux_shared::{InstanceKind, Role};
use serde::Serialize;

/// État public de l'installation, identique pour le central et le worker (sondes, supervision).
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstanceStatus {
    instance_kind: InstanceKind,
    version: &'static str,
    installed: bool,
}

pub async fn status(State(state): State<AppState>) -> AppResult<Json<InstanceStatus>> {
    Ok(Json(InstanceStatus {
        instance_kind: InstanceKind::Central,
        version: env!("CARGO_PKG_VERSION"),
        installed: repository::account_count(&state.pool).await? > 0,
    }))
}

/// Crée la clé d'API d'un nouveau worker. Réservé aux identifiants d'un administrateur ; la clé
/// n'est renvoyée qu'ici. Pas de cookie : l'appel vient du worker, jamais d'un navigateur.
pub async fn enroll(
    State(state): State<AppState>,
    Json(request): Json<enrollment::EnrollRequest>,
) -> AppResult<Response> {
    if !worker_settings::is_valid_worker_name(&request.worker_name) {
        return Err(AppError::Invalid);
    }
    let account =
        session::verify_credentials(&state.pool, &request.username, request.password).await?;
    if account.role != Role::Admin {
        return Err(AppError::Forbidden);
    }
    let issued = crate::api_keys::issue(&state.pool, &request.worker_name, Role::Worker).await?;
    let body = Json(enrollment::EnrolledWorker { key: issued.raw });
    Ok(([(header::CACHE_CONTROL, "no-store")], body).into_response())
}

/// Transforme une installation vierge en worker : enrôlement auprès du central, rôle conservé dans
/// `/config`, puis redémarrage interne en worker. Une fois le central installé ou le rôle choisi : 409.
pub async fn setup_worker(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<SetupWorkerRequest>,
) -> AppResult<StatusCode> {
    web_session::validate_origin(&headers, &state.config)?;
    let config_dir = &state.config.config_dir;
    let already_worker = worker_settings::load_worker(config_dir)
        .map_err(|error| {
            tracing::error!(error = %error, "rôle de l'installation illisible");
            AppError::Internal
        })?
        .is_some();
    if already_worker || repository::account_count(&state.pool).await? > 0 {
        return Err(AppError::AlreadyConfigured);
    }
    if !worker_settings::is_valid_worker_name(&request.worker_name) {
        return Err(AppError::Invalid);
    }
    let central = config::central_url(request.central_url.trim()).map_err(|_| AppError::Invalid)?;
    let key = enrollment::request_key(
        &central,
        &enrollment::EnrollRequest {
            username: request.username.trim().to_owned(),
            password: request.password,
            worker_name: request.worker_name.clone(),
        },
    )
    .await?;
    worker_settings::save_worker(
        config_dir,
        &worker_settings::WorkerSettings {
            name: request.worker_name,
            central_url: central.as_str().trim_end_matches('/').to_owned(),
        },
        &key,
    )
    .map_err(|error| {
        tracing::error!(error = %error, "rôle worker impossible à conserver");
        AppError::Internal
    })?;
    instance::request_role_switch();
    Ok(StatusCode::ACCEPTED)
}
