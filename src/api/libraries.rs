//! Gestionnaires HTTP des bibliothèques déclarées.

use super::{AdminAuth, AppState};
use crate::{
    domain::{Library, LibraryWrite},
    error::AppResult,
    repository,
};
use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use uuid::Uuid;

pub async fn list(State(s): State<AppState>, _auth: AdminAuth) -> AppResult<Json<Vec<Library>>> {
    Ok(Json(repository::list_libraries(&s.pool).await?))
}

pub async fn create(
    State(s): State<AppState>,
    _auth: AdminAuth,
    Json(payload): Json<LibraryWrite>,
) -> AppResult<(StatusCode, Json<Library>)> {
    let library = repository::insert_library(&s.pool, payload).await?;
    Ok((StatusCode::CREATED, Json(library)))
}

pub async fn get(
    State(s): State<AppState>,
    Path(id): Path<Uuid>,
    _auth: AdminAuth,
) -> AppResult<Json<Library>> {
    Ok(Json(repository::get_library(&s.pool, id).await?))
}

pub async fn update(
    State(s): State<AppState>,
    Path(id): Path<Uuid>,
    _auth: AdminAuth,
    Json(payload): Json<LibraryWrite>,
) -> AppResult<Json<Library>> {
    Ok(Json(
        repository::update_library(&s.pool, id, payload).await?,
    ))
}

pub async fn delete(
    State(s): State<AppState>,
    Path(id): Path<Uuid>,
    _auth: AdminAuth,
) -> AppResult<StatusCode> {
    repository::delete_library(&s.pool, id).await?;
    Ok(StatusCode::NO_CONTENT)
}
