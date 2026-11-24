//! Gestionnaires HTTP de la librairie locale (`/api/v1/local`) : scripts, ressources, modèles et packs maison.

use super::{AnyAuth, AppState, ManagePluginsAuth, ViewLibrariesAuth, to_completion, transfers};
use crate::{
    error::{AppError, AppResult},
    plugins, repository, script_trial,
};
use axum::{
    Json,
    extract::{Path, State},
    http::{HeaderValue, header},
    response::Response,
};
use forgearr_shared::{
    Permission,
    highlight::{HighlightRequest, HighlightedCode},
    local::{
        CreateBlockTemplateRequest, CreateFlowTemplateRequest, CreateLocalPackRequest,
        LocalBlockTemplate, LocalFlowTemplate, LocalLibrarySnapshot, LocalResourceSummary,
        LocalScriptSummary, UploadLocalResourceRequest, UpsertLocalScriptRequest,
    },
    plugins::CatalogEntry,
    script_trial::{ScriptTrialRequest, ScriptTrialResult},
};
use serde_json::{Value, json};

/// Écrit dans la librairie locale puis vide le cache du catalogue, qui signale le changement. Une écriture
/// à la fois : chacune réécrit l'index, comme l'installation d'un script.
pub(super) async fn write_local<T, W>(s: &AppState, write: W) -> AppResult<T>
where
    T: Send + 'static,
    W: FnOnce(&std::path::Path) -> AppResult<T> + Send + 'static,
{
    let local_dir = plugins::local_catalog_dir(&s.config);
    let cache = s.catalog_cache.clone();
    to_completion(async move {
        let _installations = plugins::lock_installations().await;
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
    let roots = plugins::engine_roots(&s.pool, &s.config).await?;
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
    let roots = plugins::engine_roots(&s.pool, &s.config).await?;
    let (path, filename) = tokio::task::spawn_blocking(move || plugins::resource_file(&roots, &id))
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

/// Colore le code d'un script en cours d'édition : analyse syntaxique côté serveur, hors du fil asynchrone.
pub async fn highlight(
    _auth: ManagePluginsAuth,
    Json(req): Json<HighlightRequest>,
) -> AppResult<Json<HighlightedCode>> {
    let lines =
        tokio::task::spawn_blocking(move || plugins::highlight_code(&req.code, &req.extension))
            .await
            .map_err(|_| AppError::Internal)?;
    Ok(Json(HighlightedCode { lines }))
}

/// Dossier, sous les données du central, où s'exécutent les essais de script du worker intégré.
const TRIAL_DIRECTORY: &str = "script-trials";

/// Exécute le code en cours d'édition sur le worker choisi (ADR 0031). Seul le worker intégré est joignable
/// pour l'instant ; un worker distant arrive avec le sous-lot 3b.
pub async fn trial_script(
    State(s): State<AppState>,
    _auth: ManagePluginsAuth,
    Json(req): Json<ScriptTrialRequest>,
) -> AppResult<Json<ScriptTrialResult>> {
    let worker = repository::get_worker(&s.pool, req.worker_id).await?;
    if !worker.is_integrated() {
        return Err(AppError::Invalid);
    }
    let scratch = s.config.data_dir.join(TRIAL_DIRECTORY);
    tokio::fs::create_dir_all(&scratch).await?;
    // Chemin absolu : le script s'exécute dans son propre dossier, un chemin relatif n'y serait plus valable.
    let scratch = tokio::fs::canonicalize(&scratch).await?;
    Ok(Json(script_trial::run_script_trial(&req, &scratch).await))
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
    to_completion(async move {
        let lock = plugins::lock_installations().await;
        plugins::delete_collection_item(&lock, &s.pool, &s.config, &s.catalog_cache, &id).await
    })
    .await?;
    Ok(Json(json!({ "id": deleted_id, "deleted": true })))
}
