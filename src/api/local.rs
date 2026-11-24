//! Gestionnaires HTTP de la librairie locale (`/api/v1/local`) : scripts, ressources, modèles et packs maison.

use super::{
    AnyAuth, AppState, ManagePluginsAuth, ViewLibrariesAuth,
    libraries::{WorkerPendingBrowseQuery, complete_report, take_pending},
    to_completion, transfers,
};
use crate::{
    error::{AppError, AppResult},
    plugins, repository, script_trial,
};
use axum::{
    Json,
    extract::{Path, Query, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
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
    script_trial::{
        ScriptTrialRequest, ScriptTrialResult, TRIAL_TIMEOUT_SECONDS, WorkerScriptTrialReport,
        WorkerScriptTrialTask,
    },
};
use serde_json::{Value, json};
use std::time::Duration;
use uuid::Uuid;

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

/// Marge, en secondes, laissée au va-et-vient avec un worker distant en plus du délai du script.
const RELAY_MARGIN_SECONDS: u64 = 10;

/// Exécute le code en cours d'édition sur le worker choisi (ADR 0031) : dans ce processus pour le worker
/// intégré, relayé par le courtier pour un worker distant en ligne.
pub async fn trial_script(
    State(s): State<AppState>,
    _auth: ManagePluginsAuth,
    Json(req): Json<ScriptTrialRequest>,
) -> AppResult<Json<ScriptTrialResult>> {
    let worker = repository::get_worker(&s.pool, req.worker_id).await?;
    if !worker.is_integrated() {
        if !worker.is_online(chrono::Utc::now()) {
            return Err(AppError::CentralUnavailable);
        }
        let task = WorkerScriptTrialTask {
            request_id: Uuid::new_v4(),
            worker_id: worker.id,
            request: req,
        };
        let wait = Duration::from_secs(TRIAL_TIMEOUT_SECONDS + RELAY_MARGIN_SECONDS);
        let report = s.trial_broker.call(task, wait).await?;
        return Ok(Json(report.result));
    }
    let scratch = script_trial::scratch_root(&s.config.data_dir).await?;
    Ok(Json(script_trial::run_script_trial(&req, &scratch).await))
}

/// Essais en attente pour un worker distant authentifié.
pub async fn worker_pending_trials(
    State(s): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<WorkerPendingBrowseQuery>,
) -> AppResult<Json<Vec<WorkerScriptTrialTask>>> {
    take_pending(&s, &headers, &s.trial_broker, q).await
}

/// Résultat d'un essai renvoyé par un worker distant.
pub async fn worker_report_trial(
    State(s): State<AppState>,
    headers: HeaderMap,
    Json(report): Json<WorkerScriptTrialReport>,
) -> AppResult<StatusCode> {
    complete_report(&s, &headers, &s.trial_broker, report).await
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
