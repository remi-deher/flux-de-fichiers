//! Retrait d'un élément, refusé tant que quelque chose s'en sert (ADR 0029).

use super::{EngineCatalog, InstallationGuard, UsageSources, item_usage};
use crate::{
    config::Config,
    error::{AppError, AppResult},
    plugins::{
        CatalogCache, CatalogItemType, engine_roots,
        local::{delete_local_item, local_item_dir},
        local_catalog_dir,
    },
    repository,
};
use sqlx::{SqliteConnection, SqlitePool};
use std::{path::Path, sync::Arc};

/// Refuse, en nommant l'usage, de retirer un élément dont quelque chose se sert. Lu dans la transaction de
/// l'appelant, pour que le refus porte sur l'état qu'elle valide.
pub async fn ensure_unused(
    conn: &mut SqliteConnection,
    item_id: &str,
    catalog: &Arc<EngineCatalog>,
    local_dir: &Path,
) -> AppResult<()> {
    let sources = UsageSources::read(conn, local_dir).await?;
    let name = catalog
        .entry(item_id)
        .map_or_else(|| item_id.to_owned(), |entry| entry.name.clone());
    let (item_id, catalog) = (item_id.to_owned(), Arc::clone(catalog));
    let local_dir = local_dir.to_path_buf();
    let usage =
        tokio::task::spawn_blocking(move || item_usage(&item_id, &catalog, &sources, &local_dir))
            .await
            .map_err(|_| AppError::Internal)?;
    if usage.is_empty() {
        return Ok(());
    }
    Err(AppError::ItemInUse(format!(
        "« {name} » est utilisé ({})",
        usage.summary()
    )))
}

/// Supprime un élément de la collection locale dont rien ne se sert. Un script installé n'est plus inscrit : un
/// arrêt juste après ne laisse qu'un script de l'utilisateur, qu'il peut encore supprimer.
pub async fn delete_collection_item(
    _lock: &InstallationGuard,
    pool: &SqlitePool,
    config: &Config,
    cache: &CatalogCache,
    item_id: &str,
) -> AppResult<()> {
    let local_dir = local_catalog_dir(config);
    local_item_dir(&local_dir, item_id)?;
    let roots = engine_roots(pool, config).await?;
    let catalog = Arc::new(EngineCatalog::cached(cache, &roots).await);
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    ensure_unused(&mut tx, item_id, &catalog, &local_dir).await?;
    repository::delete_installation_of_copy(&mut tx, CatalogItemType::Script, item_id).await?;
    tx.commit().await?;
    let item_id = item_id.to_owned();
    let deleted = tokio::task::spawn_blocking(move || delete_local_item(&local_dir, &item_id))
        .await
        .map_err(|_| AppError::Internal)?;
    cache.invalidate().await;
    deleted
}

/// Désinstalle un élément dont rien ne se sert (ADR 0029 §8). Ses réglages sont conservés ; un script perd sa copie
/// modifiable, un flux créé reste à l'utilisateur. `NotFound` s'il n'est pas inscrit.
pub async fn uninstall_item(
    _lock: &InstallationGuard,
    pool: &SqlitePool,
    config: &Config,
    cache: &CatalogCache,
    item_id: &str,
) -> AppResult<()> {
    let local_dir = local_catalog_dir(config);
    let roots = engine_roots(pool, config).await?;
    let catalog = Arc::new(EngineCatalog::cached(cache, &roots).await);
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    let record = repository::get_installation(&mut tx, item_id)
        .await?
        .ok_or(AppError::NotFound)?
        .record;
    ensure_unused(&mut tx, item_id, &catalog, &local_dir).await?;
    repository::delete_installation(&mut tx, item_id).await?;
    tx.commit().await?;
    if record.kind == CatalogItemType::Script && local_item_dir(&local_dir, item_id).is_ok() {
        let item_id = item_id.to_owned();
        tokio::task::spawn_blocking(move || delete_local_item(&local_dir, &item_id))
            .await
            .map_err(|_| AppError::Internal)??;
    }
    // Le dossier figé devient un reste : le balayage l'effacera après son délai de garde.
    cache.invalidate().await;
    Ok(())
}
