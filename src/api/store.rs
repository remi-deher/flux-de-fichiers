//! Store et éléments installés (ADR 0029) : vitrine des dépôts, plan, installation et désinstallation.

use super::{AppState, ManagePluginsAuth, ViewPluginsAuth, to_completion};
use crate::{
    error::{AppError, AppResult},
    plugins::{self, ApplyContext, PlanContext},
    repository,
};
use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use forgearr_shared::{
    Permission,
    plugins::store::{
        InstalledItem, StoreEntry,
        plan::{InstallOutcome, InstallPlan, InstallRequest, PlanRequest},
        usage::ItemUsage,
    },
};
use std::sync::Arc;

/// Contexte du Store, lu sous le verrou partagé des installations.
async fn read_context(s: &AppState, may_manage_flows: bool) -> AppResult<PlanContext> {
    let _installations = plugins::share_installations().await;
    PlanContext::read(&s.pool, &s.config, &s.catalog_cache, may_manage_flows).await
}

/// Éléments de tous les dépôts activés, homonymes compris.
pub async fn list_store(
    State(s): State<AppState>,
    _auth: ViewPluginsAuth,
) -> AppResult<Json<Vec<StoreEntry>>> {
    let context = read_context(&s, false).await?;
    let entries = tokio::task::spawn_blocking(move || context.store_entries())
        .await
        .map_err(|_| AppError::Internal)?;
    Ok(Json(entries))
}

/// Plan d'installation ou de mise à jour, sans effet.
pub async fn plan(
    State(s): State<AppState>,
    ManagePluginsAuth(principal): ManagePluginsAuth,
    Json(request): Json<PlanRequest>,
) -> AppResult<Json<InstallPlan>> {
    let may_manage_flows = principal.allows(Permission::ManageFlows);
    let _installations = plugins::share_installations().await;
    let prepared = plugins::plan_installation(
        &s.pool,
        &s.config,
        &s.catalog_cache,
        &request.targets,
        may_manage_flows,
    )
    .await?;
    Ok(Json(prepared.plan))
}

/// Applique le plan accepté, même si le client coupe la connexion pendant le téléchargement des outils.
pub async fn install(
    State(s): State<AppState>,
    ManagePluginsAuth(principal): ManagePluginsAuth,
    Json(request): Json<InstallRequest>,
) -> AppResult<Json<InstallOutcome>> {
    let may_manage_flows = principal.allows(Permission::ManageFlows);
    to_completion(async move {
        let tools_cache = s.tool_cache();
        let sync_policy = crate::runtime_settings::load_effective(&s.pool, &s.config)
            .await
            .sync_policy();
        let context = ApplyContext {
            pool: &s.pool,
            config: &s.config,
            cache: &s.catalog_cache,
            tools_cache: &tools_cache,
            sync_policy: &sync_policy,
            may_manage_flows,
        };
        plugins::apply_installation(&context, &request)
            .await
            .map(Json)
    })
    .await
}

/// Éléments installés, avec leurs mises à jour.
pub async fn list_installed(
    State(s): State<AppState>,
    _auth: ViewPluginsAuth,
) -> AppResult<Json<Vec<InstalledItem>>> {
    let context = read_context(&s, false).await?;
    let installations = repository::list_installations(&s.pool).await?;
    let items = tokio::task::spawn_blocking(move || context.installed_items(&installations))
        .await
        .map_err(|_| AppError::Internal)?;
    Ok(Json(items))
}

/// Ce qui se sert d'un élément installé.
pub async fn usage(
    State(s): State<AppState>,
    Path(id): Path<String>,
    _auth: ViewPluginsAuth,
) -> AppResult<Json<ItemUsage>> {
    let roots = plugins::engine_roots(&s.pool, &s.config).await?;
    let catalog = Arc::new(plugins::EngineCatalog::cached(&s.catalog_cache, &roots).await);
    if catalog.entry(&id).is_none() {
        return Err(AppError::NotFound);
    }
    let local_dir = plugins::local_catalog_dir(&s.config);
    let sources = {
        let mut conn = s.pool.acquire().await?;
        plugins::UsageSources::read(&mut conn, &local_dir).await?
    };
    let usage = tokio::task::spawn_blocking(move || {
        plugins::item_usage(&id, &catalog, &sources, &local_dir)
    })
    .await
    .map_err(|_| AppError::Internal)?;
    Ok(Json(usage))
}

/// Désinstalle un élément ; refusé, en nommant l'usage, si quelque chose s'en sert.
pub async fn uninstall(
    State(s): State<AppState>,
    Path(id): Path<String>,
    _auth: ManagePluginsAuth,
) -> AppResult<StatusCode> {
    to_completion(async move {
        let lock = plugins::lock_installations().await;
        plugins::uninstall_item(&lock, &s.pool, &s.config, &s.catalog_cache, &id).await
    })
    .await?;
    Ok(StatusCode::NO_CONTENT)
}
