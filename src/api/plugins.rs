//! Gestionnaires HTTP pour les plugins, le catalogue et les outils distribués.

use super::AppState;
use crate::{
    error::{AppError, AppResult},
    plugins::{self, CatalogEntry, ToolStatusSummary},
    tools_cache::ToolCacheManager,
};
use axum::{
    Json,
    extract::{Path, State},
    http::HeaderMap,
};
use serde_json::Value;

/// Liste l'ensemble des composants déclarés dans le catalogue.
pub async fn catalog(
    State(s): State<AppState>,
    h: HeaderMap,
) -> AppResult<Json<Vec<CatalogEntry>>> {
    crate::web_session::authorize_admin(&s.pool, &s.config, &h, &axum::http::Method::GET).await?;
    let entries = plugins::list_catalog(s.config.plugins_dir.as_deref());
    Ok(Json(entries))
}

/// Liste l'état d'installation et de cache des outils distribués.
pub async fn tools(
    State(s): State<AppState>,
    h: HeaderMap,
) -> AppResult<Json<Vec<ToolStatusSummary>>> {
    crate::web_session::authorize_admin(&s.pool, &s.config, &h, &axum::http::Method::GET).await?;
    let cache_mgr = ToolCacheManager::new(&s.config.tools_cache_dir);
    let tools = plugins::list_tools_status(s.config.plugins_dir.as_deref(), &cache_mgr);
    Ok(Json(tools))
}

/// Renvoie le contrat déclaratif de distribution d'un outil spécifique.
pub async fn tool_distribution(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(id): Path<String>,
) -> AppResult<Json<Value>> {
    crate::web_session::authorize_admin(&s.pool, &s.config, &h, &axum::http::Method::GET).await?;
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
            "toolId": "tool.mkvtoolnix",
            "toolVersion": "1.0.0",
            "distributionMode": "autonomous-binary",
            "capability": "remux",
            "currentChoice": true,
            "notes": ["Option B : binaire autonome hors conteneur applicatif"]
        }))),
        "tool.ffmpeg" => Ok(Json(serde_json::json!({
            "schemaVersion": "1.0.0",
            "toolId": "tool.ffmpeg",
            "toolVersion": "1.0.0",
            "distributionMode": "autonomous-binary",
            "capability": "analyze",
            "currentChoice": false
        }))),
        "tool.internal.telemetry" => Ok(Json(serde_json::json!({
            "schemaVersion": "1.0.0",
            "toolId": "tool.internal.telemetry",
            "toolVersion": "0.1.0",
            "distributionMode": "autonomous-binary",
            "capability": "telemetry",
            "currentChoice": true
        }))),
        _ => Err(AppError::NotFound),
    }
}

/// Renvoie la configuration enregistrée pour un plugin donné.
pub async fn get_settings(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(id): Path<String>,
) -> AppResult<Json<Value>> {
    crate::web_session::authorize_admin(&s.pool, &s.config, &h, &axum::http::Method::GET).await?;
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
    h: HeaderMap,
    Path(id): Path<String>,
    Json(payload): Json<Value>,
) -> AppResult<Json<Value>> {
    crate::web_session::authorize_admin(&s.pool, &s.config, &h, &axum::http::Method::PUT).await?;
    let settings_str = serde_json::to_string(&payload).map_err(|_| AppError::Invalid)?;
    crate::repository::set_plugin_settings(&s.pool, &id, &settings_str).await?;
    Ok(Json(serde_json::json!({
        "pluginId": id,
        "status": "saved",
        "settings": payload,
    })))
}
