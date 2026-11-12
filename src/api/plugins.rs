//! Gestionnaires HTTP pour les plugins, le catalogue et les outils distribués.

use super::{AdminAuth, AppState};
use crate::{
    error::{AppError, AppResult},
    plugins::{self, CatalogEntry, ToolStatusSummary},
    tools_cache::ToolCacheManager,
};
use axum::{
    Json,
    extract::{Path, State},
};
use serde_json::Value;

/// Liste l'ensemble des composants déclarés dans le catalogue.
pub async fn catalog(
    State(s): State<AppState>,
    _auth: AdminAuth,
) -> AppResult<Json<Vec<CatalogEntry>>> {
    let entries = plugins::list_catalog(s.config.plugins_dir.as_deref());
    Ok(Json(entries))
}

/// Liste l'état d'installation et de cache des outils distribués.
pub async fn tools(
    State(s): State<AppState>,
    _auth: AdminAuth,
) -> AppResult<Json<Vec<ToolStatusSummary>>> {
    let cache_mgr = ToolCacheManager::new(&s.config.tools_cache_dir);
    let tools = plugins::list_tools_status(s.config.plugins_dir.as_deref(), &cache_mgr);
    Ok(Json(tools))
}

/// Renvoie le contrat déclaratif de distribution d'un outil spécifique.
pub async fn tool_distribution(
    State(s): State<AppState>,
    Path(id): Path<String>,
    _auth: AdminAuth,
) -> AppResult<Json<Value>> {
    let tool_name = id.trim_start_matches("tool.");
    if let Some(ref root) = s.config.plugins_dir {
        let dist_path = root
            .join("tools")
            .join(tool_name)
            .join("distribution.v1.json");
        if dist_path.is_file() {
            let content = std::fs::read_to_string(&dist_path)?;
            let val: Value = serde_json::from_str(&content)
                .map_err(|e| AppError::InvalidManifest(e.to_string()))?;
            return Ok(Json(val));
        }
    }
    // Repli de secours pour les outils fondamentaux
    match id.as_str() {
        "tool.mkvtoolnix" => Ok(Json(serde_json::json!({
            "schemaVersion": "1.0.0",
            "tool": "mkvtoolnix",
            "name": "MKVToolNix CLI",
            "description": "Ensemble d'outils officiel pour la manipulation de conteneurs Matroska (mkvmerge, mkvpropedit, mkvextract, mkvinfo).",
            "platforms": {
                "linux-x86_64": {
                    "distributionType": "system",
                    "package": "mkvtoolnix",
                    "binary": "mkvmerge"
                },
                "linux-aarch64": {
                    "distributionType": "system",
                    "package": "mkvtoolnix",
                    "binary": "mkvmerge"
                }
            }
        }))),
        _ => Err(AppError::NotFound),
    }
}

/// Récupère la configuration (valeurs personnalisées) d'un plugin.
pub async fn get_settings(
    State(s): State<AppState>,
    Path(id): Path<String>,
    _auth: AdminAuth,
) -> AppResult<Json<Value>> {
    let saved = crate::repository::get_plugin_settings(&s.pool, &id).await?;
    let settings_val = match saved {
        Some(s) => serde_json::from_str(&s).unwrap_or(serde_json::json!({})),
        None => serde_json::json!({}),
    };
    Ok(Json(serde_json::json!({
        "pluginId": id,
        "settings": settings_val,
    })))
}

/// Enregistre la configuration (valeurs par défaut des blocs) d'un plugin.
pub async fn save_settings(
    State(s): State<AppState>,
    Path(id): Path<String>,
    _auth: AdminAuth,
    Json(payload): Json<Value>,
) -> AppResult<Json<Value>> {
    let settings_str = serde_json::to_string(&payload).map_err(|_| AppError::Invalid)?;
    crate::repository::set_plugin_settings(&s.pool, &id, &settings_str).await?;
    Ok(Json(serde_json::json!({
        "pluginId": id,
        "status": "saved",
        "settings": payload,
    })))
}

/// Liste l'ensemble des dépôts configurés.
pub async fn list_repositories(
    State(s): State<AppState>,
    _auth: AdminAuth,
) -> AppResult<Json<Vec<plugins::PluginRepository>>> {
    let repos = crate::repository::list_plugin_repositories(&s.pool).await?;
    Ok(Json(repos))
}

/// Ajoute un nouveau dépôt tiers de plugins ou flux.
pub async fn add_repository(
    State(s): State<AppState>,
    _auth: AdminAuth,
    Json(payload): Json<plugins::CreateRepositoryRequest>,
) -> AppResult<Json<plugins::PluginRepository>> {
    let url = payload.index_url.trim();
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return Err(AppError::InvalidManifest(
            "L'URL du dépôt doit commencer par http:// ou https://".to_string(),
        ));
    }
    let name = payload.name.trim();
    if name.is_empty() {
        return Err(AppError::InvalidManifest(
            "Le nom du dépôt ne peut pas être vide".to_string(),
        ));
    }
    let id = payload
        .id
        .unwrap_or_else(|| format!("repo.{}", uuid::Uuid::new_v4().simple()));

    let repo = crate::repository::add_plugin_repository(&s.pool, &id, name, url).await?;
    Ok(Json(repo))
}

/// Active ou désactive une source de dépôt.
pub async fn toggle_repository(
    State(s): State<AppState>,
    Path(id): Path<String>,
    _auth: AdminAuth,
) -> AppResult<Json<Value>> {
    let new_state = crate::repository::toggle_plugin_repository(&s.pool, &id).await?;
    Ok(Json(serde_json::json!({
        "id": id,
        "enabled": new_state,
    })))
}

/// Supprime un dépôt tiers (interdit sur le catalogue officiel).
pub async fn delete_repository(
    State(s): State<AppState>,
    Path(id): Path<String>,
    _auth: AdminAuth,
) -> AppResult<Json<Value>> {
    crate::repository::delete_plugin_repository(&s.pool, &id).await?;
    Ok(Json(serde_json::json!({
        "id": id,
        "deleted": true,
    })))
}

/// Synchronise l'index d'un dépôt donné.
pub async fn sync_repository(
    State(s): State<AppState>,
    Path(id): Path<String>,
    _auth: AdminAuth,
) -> AppResult<Json<Value>> {
    crate::repository::touch_plugin_repository_sync(&s.pool, &id).await?;
    Ok(Json(serde_json::json!({
        "id": id,
        "status": "synced",
        "syncedAt": chrono::Utc::now().to_rfc3339(),
    })))
}
