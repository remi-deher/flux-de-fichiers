//! Ce que la reprise des dépôts veut figer, et où le trouver (ADR 0029).

use super::{
    EngineCatalog, UsageSources, UsedElements,
    usage::{
        base_elements, base_is_missing, elements_declared_by, elements_used_by, local_entries,
        with_declared_elements,
    },
};
use crate::{
    config::Config,
    error::{AppError, AppResult},
    plugins::{
        CatalogEntry, CatalogItemType, StoreSource, engine_sources, load_catalog,
        local_catalog_dir, tool_version,
    },
    repository,
    tools_cache::{ToolCacheManager, host_arch},
};
use sqlx::SqlitePool;
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};

/// Plugin de toute installation : entrée, sortie, conditions.
const ESSENTIAL_PLUGIN_ID: &str = "essentiel";

pub(super) struct Survey {
    /// Éléments à figer, dans l'ordre du catalogue : le gagnant d'un conflit d'opération reste le même.
    pub wanted: Vec<String>,
    /// Une référence ne mène à aucun élément : un dépôt encore sans copie peut le porter.
    pub unresolved: bool,
    /// Plugins dont des réglages sont enregistrés.
    pub configured: HashSet<String>,
    /// Éléments de la collection locale : ils restent à l'utilisateur.
    local: HashSet<String>,
    /// Éléments de chaque source ; `None` si son index est illisible.
    listings: Vec<Option<HashSet<String>>>,
}

impl Survey {
    pub async fn take(
        pool: &SqlitePool,
        config: &Config,
        sources: &[StoreSource],
    ) -> AppResult<Self> {
        let local_dir = local_catalog_dir(config);
        let usage = {
            let mut conn = pool.acquire().await?;
            UsageSources::read(&mut conn, &local_dir).await?
        };
        let configured = configured_plugins(pool).await?;
        let roots = engine_sources(pool, config).await?.roots;
        let source_roots: Vec<PathBuf> = sources.iter().map(|source| source.root.clone()).collect();
        let cache = ToolCacheManager::new(&config.tools_cache_dir);
        tokio::task::spawn_blocking(move || {
            let local = standalone_ids(local_entries(&local_dir)?);
            let catalog = EngineCatalog::read(&roots);
            let used = wanted_elements(&catalog, &usage, &configured, &local_dir);
            let is_known = |id: &String| catalog.entry(id).is_some();
            let unresolved = base_is_missing(&catalog)
                || !used.unknown_kinds.is_empty()
                || !used.items.iter().chain(&used.tools).all(is_known);
            let wanted = catalog
                .entries
                .iter()
                .filter(|(_, entry)| is_frozen_or_copied(entry.entry_type))
                .filter(|(_, entry)| {
                    used.items.contains(&entry.id)
                        || used.tools.contains(&entry.id)
                        // Déjà téléchargé : sans son contrat figé, il serait purgé.
                        || (entry.entry_type == CatalogItemType::App
                            && is_cached(&cache, &roots, &entry.id, &entry.version))
                })
                .map(|(_, entry)| entry.id.clone())
                .collect();
            Ok(Self {
                wanted,
                unresolved,
                configured,
                local,
                listings: source_roots.iter().map(|root| listing(root)).collect(),
            })
        })
        .await
        .map_err(|_| AppError::Internal)?
    }

    /// Source qui donne l'élément ; `None` s'il est local, ou donné par le seul `PLUGINS_DIR`.
    pub fn source_of<'a>(
        &self,
        item_id: &str,
        sources: &'a [StoreSource],
    ) -> Option<&'a StoreSource> {
        if self.local.contains(item_id) {
            return None;
        }
        sources
            .iter()
            .zip(&self.listings)
            .find(|(_, listing)| {
                listing
                    .as_ref()
                    .is_some_and(|listing| listing.contains(item_id))
            })
            .map(|(source, _)| source)
    }

    /// La copie de ce dépôt est-elle lisible ?
    pub fn reads(&self, repository_id: &str, sources: &[StoreSource]) -> bool {
        sources.iter().zip(&self.listings).any(|(source, listing)| {
            source.origin.repository_id == repository_id && listing.is_some()
        })
    }
}

fn is_frozen_or_copied(kind: CatalogItemType) -> bool {
    kind.runs_from_frozen_copy() || kind == CatalogItemType::Script
}

/// Éléments autonomes d'un dossier ; `None` si son index est illisible.
fn listing(root: &Path) -> Option<HashSet<String>> {
    Some(standalone_ids(load_catalog(root).ok()?.entries))
}

fn standalone_ids(entries: Vec<CatalogEntry>) -> HashSet<String> {
    entries
        .into_iter()
        .filter(|entry| entry.entry_type.is_installed_by_itself())
        .map(|entry| entry.id)
        .collect()
}

fn is_cached(cache: &ToolCacheManager, roots: &[PathBuf], id: &str, listed: &str) -> bool {
    cache.is_ready(id, &tool_version(roots, id, Some(listed)), host_arch())
}

/// `essentiel`, le socle, ce dont se servent les flux actuels, les tâches non terminées, les modèles et les
/// scripts locaux, les plugins configurés, et ce dont tous ceux-là se servent eux-mêmes.
fn wanted_elements(
    catalog: &EngineCatalog,
    usage: &UsageSources,
    configured: &HashSet<String>,
    local_dir: &Path,
) -> UsedElements {
    let mut used = base_elements(catalog);
    used.items.insert(ESSENTIAL_PLUGIN_ID.to_owned());
    used.items.extend(configured.iter().cloned());
    for graph in usage.graphs() {
        used.extend(elements_used_by(graph, catalog));
    }
    for (root, entry) in &catalog.entries {
        if root == local_dir && entry.entry_type == CatalogItemType::Script {
            used.extend(elements_declared_by(&entry.id, catalog));
        }
    }
    with_declared_elements(used, catalog)
}

/// Plugins dont des réglages sont enregistrés.
pub(in crate::plugins) async fn configured_plugins(
    pool: &SqlitePool,
) -> AppResult<HashSet<String>> {
    Ok(repository::list_all_plugin_settings(pool)
        .await?
        .into_iter()
        .filter(|(_, settings)| {
            serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(settings)
                .is_ok_and(|settings| !settings.is_empty())
        })
        .map(|(id, _)| id)
        .collect())
}
