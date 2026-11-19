//! Gestionnaires HTTP pour les plugins, le catalogue et les outils distribués.

use super::{AppState, ManagePluginsAuth, ViewPluginsAuth};
use crate::{
    error::{AppError, AppResult},
    plugins::{self, CatalogEntry, ToolStatusSummary},
    secrets::SecretBox,
    tools_cache::ToolCacheManager,
};
use axum::{
    Json,
    extract::{Path, State},
};
use serde_json::Value;

/// Liste les pièces du catalogue à exposer ; les blocs, propriété d'un plugin, n'en font pas partie.
pub async fn catalog(
    State(s): State<AppState>,
    _auth: ViewPluginsAuth,
) -> AppResult<Json<Vec<CatalogEntry>>> {
    let roots = plugins::catalog_roots(&s.pool, &s.config).await;
    let cache_mgr = ToolCacheManager::new(&s.config.tools_cache_dir);
    let saved_settings = crate::repository::list_all_plugin_settings(&s.pool)
        .await
        .unwrap_or_default();
    Ok(Json(
        s.catalog_cache
            .exposed_catalog_enriched(&roots, &cache_mgr, &saved_settings)
            .await,
    ))
}

/// Fiche détaillée d'un plugin du catalogue (métadonnées, dépendances avec état réel, compatibilité et blocs exposés).
pub async fn plugin_detail(
    State(s): State<AppState>,
    Path(id): Path<String>,
    _auth: ViewPluginsAuth,
) -> AppResult<Json<plugins::PluginDetailView>> {
    if !plugins::is_valid_repository_id(&id) {
        return Err(AppError::NotFound);
    }
    let roots = plugins::catalog_roots(&s.pool, &s.config).await;
    let mut detail = s
        .catalog_cache
        .plugin_detail(&roots, &id)
        .await
        .ok_or(AppError::NotFound)?;
    let cache_mgr = ToolCacheManager::new(&s.config.tools_cache_dir);
    let tool_statuses: std::collections::HashMap<String, plugins::ToolStatus> = s
        .catalog_cache
        .tools_status(&roots, &cache_mgr)
        .await
        .into_iter()
        .map(|tool| (tool.id, tool.status))
        .collect();
    plugins::attach_dependency_statuses(&mut detail, &tool_statuses);
    Ok(Json(detail))
}

/// Liste l'état d'installation et de cache des outils distribués.
pub async fn tools(
    State(s): State<AppState>,
    _auth: ViewPluginsAuth,
) -> AppResult<Json<Vec<ToolStatusSummary>>> {
    let cache_mgr = ToolCacheManager::new(&s.config.tools_cache_dir);
    let roots = plugins::catalog_roots(&s.pool, &s.config).await;
    Ok(Json(s.catalog_cache.tools_status(&roots, &cache_mgr).await))
}

/// Renvoie le contrat déclaratif de distribution d'un outil, lu dans les dépôts synchronisés.
pub async fn tool_distribution(
    State(s): State<AppState>,
    Path(id): Path<String>,
    _auth: ManagePluginsAuth,
) -> AppResult<Json<Value>> {
    let roots = plugins::catalog_roots(&s.pool, &s.config).await;
    Ok(Json(plugins::read_distribution(&roots, &id)?))
}

/// Même contrat, pour un worker distant : il l'installe lui-même dans son cache.
pub async fn worker_tool_distribution(
    State(s): State<AppState>,
    headers: axum::http::HeaderMap,
    Path(id): Path<String>,
) -> AppResult<Json<Value>> {
    crate::auth::authorize_worker(&s.pool, &headers).await?;
    let roots = plugins::catalog_roots(&s.pool, &s.config).await;
    Ok(Json(plugins::read_distribution(&roots, &id)?))
}

/// Installe un outil du catalogue dans le cache du central (worker intégré).
pub async fn install_tool(
    State(s): State<AppState>,
    Path(id): Path<String>,
    _auth: ManagePluginsAuth,
) -> AppResult<Json<plugins::DependencyInstallResult>> {
    install_catalog_item(&s, &id, plugins::CatalogItemType::App).await
}

/// Installe les outils requis par un plugin (dépendances transitives comprises).
pub async fn install_plugin(
    State(s): State<AppState>,
    Path(id): Path<String>,
    _auth: ManagePluginsAuth,
) -> AppResult<Json<plugins::DependencyInstallResult>> {
    install_catalog_item(&s, &id, plugins::CatalogItemType::Plugin).await
}

/// Installe une pièce du catalogue du type attendu et ses outils requis.
async fn install_catalog_item(
    s: &AppState,
    id: &str,
    expected: plugins::CatalogItemType,
) -> AppResult<Json<plugins::DependencyInstallResult>> {
    let roots = plugins::catalog_roots(&s.pool, &s.config).await;
    if !plugins::list_catalog(&roots)
        .iter()
        .any(|entry| entry.id == id && entry.entry_type == expected)
    {
        return Err(AppError::NotFound);
    }
    let cache_mgr = ToolCacheManager::new(&s.config.tools_cache_dir);
    let sync_policy = crate::runtime_settings::load_effective(&s.pool, &s.config)
        .await
        .sync_policy();
    let result =
        plugins::install_with_dependencies(Some(&s.pool), &roots, &cache_mgr, &sync_policy, id)
            .await?;
    Ok(Json(result))
}

fn parse_saved(raw: Option<&str>) -> Value {
    raw.and_then(|text| serde_json::from_str(text).ok())
        .unwrap_or_else(|| serde_json::json!({}))
}

fn keys_of(value: &Value) -> Vec<String> {
    value
        .as_object()
        .map(|object| object.keys().cloned().collect())
        .unwrap_or_default()
}

/// Récupère la configuration d'un plugin ; les secrets ne sont jamais renvoyés (seulement `isSet`).
pub async fn get_settings(
    State(s): State<AppState>,
    Path(id): Path<String>,
    _auth: ManagePluginsAuth,
) -> AppResult<Json<plugins::PluginSettingsView>> {
    let secrets = SecretBox::open(&s.config.config_dir)?;
    let roots = plugins::catalog_roots(&s.pool, &s.config).await;
    let schema_fields = s.catalog_cache.settings_fields(&roots, &id).await;
    let mut saved = parse_saved(
        crate::repository::get_plugin_settings(&s.pool, &id)
            .await?
            .as_deref(),
    );
    let names = plugins::secret_names(&schema_fields, keys_of(&saved).iter());
    // Anciens enregistrements en clair : chiffrés à la première lecture.
    if let Some(migrated) = plugins::migrate_plaintext(&secrets, &id, &saved, &names)? {
        let migrated_str = serde_json::to_string(&migrated).map_err(|_| AppError::Invalid)?;
        crate::repository::set_plugin_settings(&s.pool, &id, &migrated_str).await?;
        saved = migrated;
    }
    let (public, set) = plugins::mask(&saved, &names);
    // `fields[].value` propose le défaut du manifeste si la clé n'est pas sauvée.
    // `settings` reste l'objet enregistré, sans les secrets ni les défauts réécrits.
    let mut fields = plugins::resolve_settings_fields(&public, &schema_fields);
    for field in &mut fields {
        if field.field_type == plugins::SettingsFieldType::Secret {
            field.is_set = set.contains(&field.name);
            field.value.clear();
        }
    }
    Ok(Json(plugins::PluginSettingsView {
        plugin_id: id,
        settings: public.as_object().cloned().unwrap_or_default(),
        fields,
    }))
}

/// Enregistre la configuration d'un plugin du catalogue ; les secrets saisis sont chiffrés, les secrets laissés vides sont conservés.
pub async fn save_settings(
    State(s): State<AppState>,
    Path(id): Path<String>,
    _auth: ManagePluginsAuth,
    Json(payload): Json<Value>,
) -> AppResult<Json<Value>> {
    let roots = plugins::catalog_roots(&s.pool, &s.config).await;
    if !plugins::is_valid_repository_id(&id) || !s.catalog_cache.plugin_exists(&roots, &id).await {
        return Err(AppError::NotFound);
    }
    let secrets = SecretBox::open(&s.config.config_dir)?;
    let schema_fields = s.catalog_cache.settings_fields(&roots, &id).await;
    let existing = parse_saved(
        crate::repository::get_plugin_settings(&s.pool, &id)
            .await?
            .as_deref(),
    );
    let keys: Vec<String> = keys_of(&existing)
        .into_iter()
        .chain(keys_of(&payload))
        .collect();
    let names = plugins::secret_names(&schema_fields, keys.iter());
    let sealed = plugins::seal(&secrets, &id, &payload, &existing, &names)?;
    let sealed_str = serde_json::to_string(&sealed).map_err(|_| AppError::Invalid)?;
    crate::repository::set_plugin_settings(&s.pool, &id, &sealed_str).await?;
    let (public, _) = plugins::mask(&sealed, &names);
    Ok(Json(serde_json::json!({
        "pluginId": id,
        "status": "saved",
        "settings": public,
    })))
}

fn find_setting_str(map: &serde_json::Map<String, Value>, keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|k| map.get(*k).and_then(|v| v.as_str()))
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
}

/// Teste la connectivité réseau et l'authentification d'un plugin API (Sonarr, Radarr, Jellyfin, etc.).
pub async fn test_connection(
    State(s): State<AppState>,
    Path(id): Path<String>,
    _auth: ManagePluginsAuth,
    body: axum::body::Bytes,
) -> AppResult<Json<plugins::PluginConnectionTestResult>> {
    let roots = plugins::catalog_roots(&s.pool, &s.config).await;
    if !plugins::is_valid_repository_id(&id) || !s.catalog_cache.plugin_exists(&roots, &id).await {
        return Err(AppError::NotFound);
    }
    let schema_fields = s.catalog_cache.settings_fields(&roots, &id).await;
    let secrets = SecretBox::open(&s.config.config_dir)?;
    let saved_raw = parse_saved(
        crate::repository::get_plugin_settings(&s.pool, &id)
            .await?
            .as_deref(),
    );
    let mut merged = plugins::decrypt_all(&secrets, &id, &saved_raw)?;
    if !body.is_empty() {
        let overrides: Value = serde_json::from_slice(&body).map_err(|_| AppError::Invalid)?;
        let Some(obj) = overrides.as_object() else {
            return Err(AppError::Invalid);
        };
        for (k, v) in obj {
            let is_empty_str = v.as_str().is_some_and(|s| s.trim().is_empty());
            if !is_empty_str {
                merged.insert(k.clone(), v.clone());
            }
        }
    }

    let url_field = schema_fields
        .iter()
        .find(|f| f.field_type == plugins::SettingsFieldType::Url)
        .map(|f| f.name.as_str());
    let base_url = url_field
        .and_then(|name| find_setting_str(&merged, &[name]))
        .or_else(|| find_setting_str(&merged, &["url", "baseUrl", "endpoint", "webhookUrl"]));
    let Some(base_url) = base_url else {
        return Ok(Json(plugins::PluginConnectionTestResult {
            ok: false,
            status_code: None,
            latency_ms: 0,
            message: "Aucune URL configurée pour ce plugin.".to_owned(),
        }));
    };

    let (probe_path, auth_header) = match id.as_str() {
        "sonarr" | "radarr" => ("/api/v3/system/status", Some("X-Api-Key")),
        "jellyfin" | "emby" => ("/System/Info", Some("X-Emby-Token")),
        "plex" => ("/identity", Some("X-Plex-Token")),
        _ => ("", None),
    };
    let target_url = if probe_path.is_empty() {
        base_url
    } else {
        format!("{}{probe_path}", base_url.trim_end_matches('/'))
    };

    let effective = crate::runtime_settings::load_effective(&s.pool, &s.config).await;
    let policy = effective.integration_policy();
    let Ok(parsed_url) = reqwest::Url::parse(target_url.trim()) else {
        return Ok(Json(plugins::PluginConnectionTestResult {
            ok: false,
            status_code: None,
            latency_ms: 0,
            message: "URL invalide.".to_owned(),
        }));
    };
    let (_, addresses) = match crate::net_policy::resolve_checked(&parsed_url, policy).await {
        Ok(resolved) => resolved,
        Err(err) => {
            return Ok(Json(plugins::PluginConnectionTestResult {
                ok: false,
                status_code: None,
                latency_ms: 0,
                message: format!("Adresse refusée par la politique réseau ({err})."),
            }));
        }
    };
    let client = match crate::net_policy::build_pinned_client(
        &parsed_url,
        &addresses,
        std::time::Duration::from_secs(5),
    ) {
        Ok(client) => client,
        Err(_) => {
            return Ok(Json(plugins::PluginConnectionTestResult {
                ok: false,
                status_code: None,
                latency_ms: 0,
                message: "Impossible d'initialiser la connexion sortante.".to_owned(),
            }));
        }
    };
    let token = find_setting_str(&merged, &["apiKey", "token", "secret", "authToken"]);
    let mut req = client.get(parsed_url);
    if let Some(tok) = token {
        if let Some(header_name) = auth_header {
            req = req.header(header_name, tok);
        } else {
            req = req.bearer_auth(tok);
        }
    }
    let started = std::time::Instant::now();
    match req.send().await {
        Ok(resp) => {
            let latency_ms = started.elapsed().as_millis() as u64;
            let code = resp.status().as_u16();
            let ok = resp.status().is_success();
            let message = if ok {
                format!("Connexion réussie (HTTP {code}) en {latency_ms} ms.")
            } else {
                format!("Le service a répondu avec le code HTTP {code}.")
            };
            Ok(Json(plugins::PluginConnectionTestResult {
                ok,
                status_code: Some(code),
                latency_ms,
                message,
            }))
        }
        Err(_) => Ok(Json(plugins::PluginConnectionTestResult {
            ok: false,
            status_code: None,
            latency_ms: started.elapsed().as_millis() as u64,
            message: "Service injoignable ou délai dépassé.".to_owned(),
        })),
    }
}

/// Retourne les couples `(tool_id, version)` actifs du catalogue pour la purge côté worker.
pub async fn worker_active_tools(
    State(s): State<AppState>,
    headers: axum::http::HeaderMap,
) -> AppResult<Json<Vec<crate::domain::ToolRequirement>>> {
    crate::auth::authorize_worker(&s.pool, &headers).await?;
    let roots = plugins::catalog_roots(&s.pool, &s.config).await;
    let cache_mgr = ToolCacheManager::new(&s.config.tools_cache_dir);
    let list = s
        .catalog_cache
        .tools_status(&roots, &cache_mgr)
        .await
        .into_iter()
        .map(|tool| crate::domain::ToolRequirement {
            tool_id: tool.id,
            version: tool.version,
        })
        .collect();
    Ok(Json(list))
}

/// Liste l'ensemble des dépôts configurés.
pub async fn list_repositories(
    State(s): State<AppState>,
    _auth: ViewPluginsAuth,
) -> AppResult<Json<Vec<plugins::PluginRepository>>> {
    let repos = crate::repository::list_plugin_repositories(&s.pool).await?;
    Ok(Json(repos))
}

/// Fiche détaillée d'un dépôt de plugins avec son contenu indexé localement.
pub async fn repository_detail(
    State(s): State<AppState>,
    Path(id): Path<String>,
    _auth: ViewPluginsAuth,
) -> AppResult<Json<plugins::RepositoryDetailView>> {
    if !plugins::is_valid_repository_id(&id) {
        return Err(AppError::NotFound);
    }
    let repo = crate::repository::get_plugin_repository(&s.pool, &id).await?;
    let repos_dir = s.config.plugin_repos_dir();
    let detail =
        tokio::task::spawn_blocking(move || plugins::build_repository_detail(repo, &repos_dir))
            .await
            .map_err(|_| AppError::Internal)?;
    Ok(Json(detail))
}

/// Ajoute un nouveau dépôt tiers de plugins ou flux.
pub async fn add_repository(
    State(s): State<AppState>,
    _auth: ManagePluginsAuth,
    Json(payload): Json<plugins::CreateRepositoryRequest>,
) -> AppResult<Json<plugins::PluginRepository>> {
    let url = payload.index_url.trim();
    plugins::validate_index_url(url)
        .map_err(|error| AppError::InvalidManifest(error.to_string()))?;
    let name = payload.name.trim();
    if name.is_empty() {
        return Err(AppError::InvalidManifest(
            "Le nom du dépôt ne peut pas être vide".to_string(),
        ));
    }
    let id = payload
        .id
        .unwrap_or_else(|| format!("repo.{}", uuid::Uuid::new_v4().simple()));
    if !plugins::is_valid_repository_id(&id) {
        return Err(AppError::InvalidManifest(
            "L'identifiant du dépôt ne peut contenir que lettres, chiffres, « . », « - » et « _ »."
                .to_string(),
        ));
    }

    let repo = crate::repository::add_plugin_repository(&s.pool, &id, name, url).await?;
    Ok(Json(repo))
}

/// Active ou désactive une source de dépôt.
pub async fn toggle_repository(
    State(s): State<AppState>,
    Path(id): Path<String>,
    _auth: ManagePluginsAuth,
) -> AppResult<Json<Value>> {
    let new_state = crate::repository::toggle_plugin_repository(&s.pool, &id).await?;
    s.catalog_cache.invalidate().await;
    Ok(Json(serde_json::json!({
        "id": id,
        "enabled": new_state,
    })))
}

/// Supprime un dépôt tiers (interdit sur le catalogue officiel).
pub async fn delete_repository(
    State(s): State<AppState>,
    Path(id): Path<String>,
    _auth: ManagePluginsAuth,
) -> AppResult<Json<Value>> {
    crate::repository::delete_plugin_repository(&s.pool, &id).await?;
    s.catalog_cache.invalidate().await;
    Ok(Json(serde_json::json!({
        "id": id,
        "deleted": true,
    })))
}

/// Synchronise un dépôt : copie atomique de son index, de ses manifestes et de ses contrats de distribution.
pub async fn sync_repository(
    State(s): State<AppState>,
    Path(id): Path<String>,
    _auth: ManagePluginsAuth,
) -> AppResult<Json<Value>> {
    let repo = crate::repository::get_plugin_repository(&s.pool, &id).await?;
    let sync_policy = crate::runtime_settings::load_effective(&s.pool, &s.config)
        .await
        .sync_policy();
    let report = plugins::sync_repository_verified(
        &s.pool,
        &repo,
        &s.config.plugin_repos_dir(),
        &sync_policy,
    )
    .await
    .map_err(|error| AppError::RepositoryUnavailable(error.to_string()))?;
    crate::repository::touch_plugin_repository_sync(&s.pool, &id).await?;
    s.catalog_cache.invalidate().await;
    Ok(Json(serde_json::json!({
        "id": id,
        "status": "synced",
        "syncedAt": chrono::Utc::now().to_rfc3339(),
        "report": report,
    })))
}

/// Lit le plan d'installation déclaratif (`packs/*/installation.json`) d'un pack du catalogue.
pub async fn pack_installation(
    State(s): State<AppState>,
    Path(id): Path<String>,
    _auth: ViewPluginsAuth,
) -> AppResult<Json<plugins::PackInstallation>> {
    let roots = plugins::catalog_roots(&s.pool, &s.config).await;
    Ok(Json(plugins::read_pack_installation(&roots, &id)?))
}

/// Fiche détaillée d'un pack (métadonnées + plan d'installation).
pub async fn pack_detail(
    State(s): State<AppState>,
    Path(id): Path<String>,
    _auth: ViewPluginsAuth,
) -> AppResult<Json<plugins::PackDetailView>> {
    if !plugins::is_valid_repository_id(&id) {
        return Err(AppError::NotFound);
    }
    let roots = plugins::catalog_roots(&s.pool, &s.config).await;
    let detail = tokio::task::spawn_blocking(move || plugins::build_pack_detail(&roots, &id))
        .await
        .map_err(|_| AppError::Internal)??;
    Ok(Json(detail))
}

/// Fiche détaillée d'un outil distribué (contrat de distribution, compatibilité et plugins utilisateurs).
pub async fn tool_detail(
    State(s): State<AppState>,
    Path(id): Path<String>,
    _auth: ViewPluginsAuth,
) -> AppResult<Json<plugins::ToolDetailView>> {
    if !plugins::is_valid_repository_id(&id) {
        return Err(AppError::NotFound);
    }
    let roots = plugins::catalog_roots(&s.pool, &s.config).await;
    let cache_mgr = ToolCacheManager::new(&s.config.tools_cache_dir);
    let detail =
        tokio::task::spawn_blocking(move || plugins::build_tool_detail(&roots, &cache_mgr, &id))
            .await
            .map_err(|_| AppError::Internal)?
            .ok_or(AppError::NotFound)?;
    Ok(Json(detail))
}

/// Exécute l'installation déclarative d'un pack (outils requis et vérification des composants).
pub async fn install_pack(
    State(s): State<AppState>,
    Path(id): Path<String>,
    _auth: ManagePluginsAuth,
) -> AppResult<Json<plugins::PackInstallResult>> {
    let roots = plugins::catalog_roots(&s.pool, &s.config).await;
    let cache_mgr = ToolCacheManager::new(&s.config.tools_cache_dir);
    let sync_policy = crate::runtime_settings::load_effective(&s.pool, &s.config)
        .await
        .sync_policy();
    let result =
        plugins::install_pack_verified(&s.pool, &roots, &cache_mgr, &sync_policy, &id).await?;
    Ok(Json(result))
}
