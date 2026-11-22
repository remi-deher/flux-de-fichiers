//! Gestionnaires HTTP de la librairie locale (`/api/v1/local`) : scripts, ressources, modèles et packs maison.

use super::{AnyAuth, AppState, ManagePluginsAuth, ViewLibrariesAuth, to_completion, transfers};
use crate::{
    error::{AppError, AppResult},
    plugins,
};
use axum::{
    Json,
    extract::{Path, State},
    http::{HeaderValue, header},
    response::Response,
};
use forgearr_shared::{
    Permission,
    local::{
        CreateBlockTemplateRequest, CreateFlowTemplateRequest, CreateLocalPackRequest,
        LocalBlockTemplate, LocalFlowTemplate, LocalLibrarySnapshot, LocalResourceSummary,
        LocalScriptSummary, UploadLocalResourceRequest, UpsertLocalScriptRequest,
    },
    plugins::CatalogEntry,
};
use serde_json::{Value, json};

/// Écrit dans la librairie locale puis vide le cache du catalogue, qui signale le changement.
pub(super) async fn write_local<T, W>(s: &AppState, write: W) -> AppResult<T>
where
    T: Send + 'static,
    W: FnOnce(&std::path::Path) -> AppResult<T> + Send + 'static,
{
    let local_dir = plugins::local_catalog_dir(&s.config);
    let cache = s.catalog_cache.clone();
    to_completion(async move {
        let written = tokio::task::spawn_blocking(move || write(&local_dir))
            .await
            .map_err(|_| AppError::Internal)??;
        cache.invalidate().await;
        Ok(written)
    })
    .await
}

pub async fn snapshot(
    State(s): State<AppState>,
    AnyAuth(principal): AnyAuth,
) -> AppResult<Json<LocalLibrarySnapshot>> {
    if !principal.allows(Permission::ViewLibraries) && !principal.allows(Permission::ViewFlows) {
        return Err(AppError::Forbidden);
    }
    let roots = plugins::catalog_roots(&s.pool, &s.config).await;
    let local_dir = plugins::local_catalog_dir(&s.config);
    let data =
        tokio::task::spawn_blocking(move || plugins::build_local_snapshot(&roots, &local_dir))
            .await
            .map_err(|_| AppError::Internal)?;
    Ok(Json(data))
}

pub async fn download_resource(
    State(s): State<AppState>,
    _auth: ViewLibrariesAuth,
    Path(id): Path<String>,
) -> AppResult<Response> {
    let local_dir = plugins::local_catalog_dir(&s.config);
    let (path, filename) =
        tokio::task::spawn_blocking(move || plugins::local_resource_file(&local_dir, &id))
            .await
            .map_err(|_| AppError::Internal)??;
    let mut response =
        transfers::stream_file(&path, Some(&filename), "application/octet-stream").await?;
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    Ok(response)
}

pub async fn upsert_script(
    State(s): State<AppState>,
    _auth: ManagePluginsAuth,
    Json(req): Json<UpsertLocalScriptRequest>,
) -> AppResult<Json<LocalScriptSummary>> {
    let saved = write_local(&s, move |dir| plugins::save_local_script(dir, req)).await?;
    Ok(Json(saved))
}

pub async fn upload_resource(
    State(s): State<AppState>,
    _auth: ManagePluginsAuth,
    Json(req): Json<UploadLocalResourceRequest>,
) -> AppResult<Json<LocalResourceSummary>> {
    let saved = write_local(&s, move |dir| plugins::upload_local_resource(dir, req)).await?;
    Ok(Json(saved))
}

pub async fn create_block_template(
    State(s): State<AppState>,
    _auth: ManagePluginsAuth,
    Json(req): Json<CreateBlockTemplateRequest>,
) -> AppResult<Json<LocalBlockTemplate>> {
    let saved = write_local(&s, move |dir| plugins::save_local_block_template(dir, req)).await?;
    Ok(Json(saved))
}

pub async fn create_flow_template(
    State(s): State<AppState>,
    _auth: ManagePluginsAuth,
    Json(req): Json<CreateFlowTemplateRequest>,
) -> AppResult<Json<LocalFlowTemplate>> {
    let saved = write_local(&s, move |dir| plugins::save_local_flow_template(dir, req)).await?;
    Ok(Json(saved))
}

pub async fn create_pack(
    State(s): State<AppState>,
    _auth: ManagePluginsAuth,
    Json(req): Json<CreateLocalPackRequest>,
) -> AppResult<Json<CatalogEntry>> {
    let saved = write_local(&s, move |dir| plugins::save_local_pack(dir, req)).await?;
    Ok(Json(saved))
}

pub async fn delete_item(
    State(s): State<AppState>,
    _auth: ManagePluginsAuth,
    Path(id): Path<String>,
) -> AppResult<Json<Value>> {
    let deleted_id = id.clone();
    write_local(&s, move |dir| plugins::delete_local_item(dir, &id)).await?;
    Ok(Json(json!({ "id": deleted_id, "deleted": true })))
}
