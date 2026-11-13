//! Gestion des clés d'API des machines : liste, création (valeur montrée une seule fois), révocation.

use super::{AdminAuth, AppState};
use crate::{
    api_keys,
    error::{AppError, AppResult},
    repository::{self, ApiKeyRecord},
};
use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use flux_shared::Role;
use flux_shared::accounts::{CreateKeyRequest, CreatedKey};

pub async fn list(
    State(state): State<AppState>,
    _auth: AdminAuth,
) -> AppResult<Json<Vec<ApiKeyRecord>>> {
    Ok(Json(repository::list_api_keys(&state.pool).await?))
}

pub async fn create(
    State(state): State<AppState>,
    _auth: AdminAuth,
    Json(request): Json<CreateKeyRequest>,
) -> AppResult<Json<CreatedKey>> {
    let role = Role::parse(&request.role).ok_or(AppError::Invalid)?;
    let issued = api_keys::issue(&state.pool, &request.name, role).await?;
    Ok(Json(CreatedKey {
        key: issued.raw,
        record: issued.record,
    }))
}

pub async fn revoke(
    State(state): State<AppState>,
    Path(id): Path<String>,
    _auth: AdminAuth,
) -> AppResult<StatusCode> {
    repository::revoke_api_key(&state.pool, &id).await?;
    Ok(StatusCode::NO_CONTENT)
}
