//! Cache en mémoire du catalogue de plugins, une lecture gardée par jeu de dossiers (moteur, chaque
//! dépôt du Store), invalidé lors des synchronisations, installations et mutations de dépôts.

use super::{
    CatalogEntry, CatalogItemType, PluginDetailView, SettingsField, ToolStatusSummary,
    build_plugin_detail, catalog_with_roots, exposure_views, flow_block_catalog,
    is_tool_installable, load_manifest, resolve_tool_version,
    settings_fields as load_settings_fields, summarize_tool, tool_version,
};
use crate::{flows::OperationRegistry, live::ChangeNotifier, tools_cache::ToolCacheManager};
use forgearr_shared::{flows::OwnedBlockSpec, live::ChangeTopic};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};
use tokio::sync::RwLock;

struct CachedTool {
    entry: CatalogEntry,
    description: String,
    installable: bool,
    /// Version du contrat de distribution, sinon celle du catalogue.
    version: String,
}

impl CachedTool {
    fn summary(&self, cache_mgr: &ToolCacheManager) -> ToolStatusSummary {
        summarize_tool(
            self.entry.clone(),
            self.description.clone(),
            self.installable,
            cache_mgr,
        )
    }
}

struct CachedSnapshot {
    entries: Vec<(PathBuf, CatalogEntry)>,
    blocks: Vec<OwnedBlockSpec>,
    operations: OperationRegistry,
    tools: Vec<CachedTool>,
}

/// Une lecture par jeu de dossiers, dans leur ordre de priorité.
type Snapshots = HashMap<Vec<PathBuf>, Arc<CachedSnapshot>>;

/// Cache thread-safe des métadonnées du catalogue lues depuis les dépôts synchronisés.
#[derive(Clone, Default)]
pub struct CatalogCache {
    inner: Arc<RwLock<Snapshots>>,
    /// Nombre d'invalidations : une lecture commencée avant la dernière n'est pas gardée.
    generation: Arc<AtomicU64>,
    changes: ChangeNotifier,
}

impl CatalogCache {
    pub fn new(changes: ChangeNotifier) -> Self {
        Self {
            changes,
            ..Self::default()
        }
    }

    pub fn changes(&self) -> &ChangeNotifier {
        &self.changes
    }

    /// Vide le cache afin que la prochaine lecture recharge les dépôts depuis le disque, puis signale
    /// le changement. À appeler après l'écriture.
    pub async fn invalidate(&self) {
        {
            let mut guard = self.inner.write().await;
            self.generation.fetch_add(1, Ordering::SeqCst);
            guard.clear();
        }
        self.changes.notify(ChangeTopic::Catalog);
    }

    async fn snapshot(&self, roots: &[PathBuf]) -> Arc<CachedSnapshot> {
        if let Some(cached) = self.inner.read().await.get(roots) {
            return Arc::clone(cached);
        }
        let generation = self.generation.load(Ordering::SeqCst);
        let owned_roots = roots.to_vec();
        let loaded = tokio::task::spawn_blocking(move || build_snapshot(&owned_roots))
            .await
            .unwrap_or_else(|_| build_snapshot(roots));
        let loaded = Arc::new(loaded);
        let mut guard = self.inner.write().await;
        if let Some(cached) = guard.get(roots) {
            return Arc::clone(cached);
        }
        if self.generation.load(Ordering::SeqCst) == generation {
            guard.insert(roots.to_vec(), Arc::clone(&loaded));
        }
        loaded
    }

    /// Toutes les entrées du catalogue, blocs compris, avec le dossier qui porte chacune.
    pub async fn entries(&self, roots: &[PathBuf]) -> Vec<(PathBuf, CatalogEntry)> {
        self.snapshot(roots).await.entries.clone()
    }

    pub async fn exposed_catalog(&self, roots: &[PathBuf]) -> Vec<CatalogEntry> {
        self.snapshot(roots)
            .await
            .entries
            .iter()
            .filter(|(_, entry)| entry.entry_type.section().is_some())
            .map(|(_, entry)| entry.clone())
            .collect()
    }

    /// Catalogue exposé enrichi de l'état opérationnel (`CatalogRuntimeSummary`) de chaque pièce.
    pub async fn exposed_catalog_enriched(
        &self,
        roots: &[PathBuf],
        cache_mgr: &ToolCacheManager,
        saved_settings: &HashMap<String, String>,
    ) -> Vec<CatalogEntry> {
        let snapshot = self.snapshot(roots).await;
        let tool_statuses: HashMap<String, super::ToolStatus> = snapshot
            .tools
            .iter()
            .map(|tool| {
                let summary = tool.summary(cache_mgr);
                (summary.id, summary.status)
            })
            .collect();
        let owned_roots = roots.to_vec();
        let saved = saved_settings.clone();
        tokio::task::spawn_blocking(move || {
            snapshot
                .entries
                .iter()
                .filter_map(|(root, entry)| {
                    entry.entry_type.section()?;
                    let runtime = super::compute_entry_runtime(
                        &owned_roots,
                        root,
                        entry,
                        &snapshot.blocks,
                        &tool_statuses,
                        &saved,
                    );
                    Some(CatalogEntry {
                        runtime: Some(runtime),
                        ..entry.clone()
                    })
                })
                .collect()
        })
        .await
        .unwrap_or_default()
    }

    pub async fn blocks(&self, roots: &[PathBuf]) -> Vec<OwnedBlockSpec> {
        self.snapshot(roots).await.blocks.clone()
    }

    pub async fn plugin_detail(
        &self,
        roots: &[PathBuf],
        plugin_id: &str,
    ) -> Option<PluginDetailView> {
        let snapshot = self.snapshot(roots).await;
        let (root, entry) = snapshot.entries.iter().find(|(_, entry)| {
            entry.id == plugin_id
                && matches!(
                    entry.entry_type,
                    CatalogItemType::Plugin | CatalogItemType::Script
                )
        })?;
        let root = root.clone();
        let entry = entry.clone();
        let exposures = exposure_views(&snapshot.operations)
            .into_iter()
            .filter(|view| view.provider == plugin_id)
            .collect();
        let mut detail = tokio::task::spawn_blocking(move || {
            build_plugin_detail(&root, &entry, &snapshot.blocks)
        })
        .await
        .ok()?;
        detail.exposures = exposures;
        Some(detail)
    }

    pub async fn operations(&self, roots: &[PathBuf]) -> OperationRegistry {
        self.snapshot(roots).await.operations.clone()
    }

    pub async fn tools_status(
        &self,
        roots: &[PathBuf],
        cache_mgr: &ToolCacheManager,
    ) -> Vec<ToolStatusSummary> {
        self.snapshot(roots)
            .await
            .tools
            .iter()
            .map(|tool| tool.summary(cache_mgr))
            .collect()
    }

    pub async fn plugin_exists(&self, roots: &[PathBuf], plugin_id: &str) -> bool {
        self.snapshot(roots)
            .await
            .entries
            .iter()
            .any(|(_, entry)| entry.id == plugin_id)
    }

    pub async fn settings_fields(&self, roots: &[PathBuf], plugin_id: &str) -> Vec<SettingsField> {
        let snapshot = self.snapshot(roots).await;
        if !snapshot
            .entries
            .iter()
            .any(|(_, entry)| entry.id == plugin_id)
        {
            return Vec::new();
        }
        let owned_roots = roots.to_vec();
        let id = plugin_id.to_owned();
        tokio::task::spawn_blocking(move || load_settings_fields(&owned_roots, &id))
            .await
            .unwrap_or_default()
    }

    pub async fn resolve_tool_version(&self, roots: &[PathBuf], tool_id: &str) -> String {
        let snapshot = self.snapshot(roots).await;
        snapshot
            .tools
            .iter()
            .find(|tool| tool.entry.id == tool_id)
            .map_or_else(
                // Contrat sans fiche d'application : lu sur disque.
                || resolve_tool_version(roots, tool_id),
                |tool| tool.version.clone(),
            )
    }
}

fn build_snapshot(roots: &[PathBuf]) -> CachedSnapshot {
    let entries = catalog_with_roots(roots);
    let blocks = flow_block_catalog(roots);
    let operations = OperationRegistry::load_from_roots(roots);
    let tools = entries
        .iter()
        .filter(|(_, entry)| entry.entry_type == CatalogItemType::App)
        .map(|(root, entry)| CachedTool {
            entry: entry.clone(),
            description: load_manifest(root, &entry.manifest_path)
                .ok()
                .and_then(|manifest| manifest.description)
                .unwrap_or_default(),
            installable: is_tool_installable(roots, &entry.id),
            version: tool_version(roots, &entry.id, Some(&entry.version)),
        })
        .collect();
    CachedSnapshot {
        entries,
        blocks,
        operations,
        tools,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn cache_reflects_invalidation_after_disk_change() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        std::fs::write(
            dir.path().join("index.json"),
            r#"{"schemaVersion":"1.0.0","entries":[
                {"id":"plugin.a","type":"plugin","version":"1.0.0","name":"A","manifestPath":"a.json"}
            ]}"#,
        )?;
        let cache = CatalogCache::default();
        let roots = [dir.path().to_path_buf()];
        assert_eq!(cache.exposed_catalog(&roots).await.len(), 1);

        std::fs::write(
            dir.path().join("index.json"),
            r#"{"schemaVersion":"1.0.0","entries":[
                {"id":"plugin.a","type":"plugin","version":"1.0.0","name":"A","manifestPath":"a.json"},
                {"id":"mkvtoolnix","type":"app","version":"2.1.0","name":"MKV","manifestPath":"mkv.json"}
            ]}"#,
        )?;
        // Sans invalidation, le cache garde la vue précédente.
        assert_eq!(cache.exposed_catalog(&roots).await.len(), 1);

        cache.invalidate().await;
        assert_eq!(cache.exposed_catalog(&roots).await.len(), 2);
        assert_eq!(
            cache.resolve_tool_version(&roots, "mkvtoolnix").await,
            "2.1.0"
        );
        Ok(())
    }

    fn write_plugin(dir: &std::path::Path, id: &str) -> std::io::Result<()> {
        std::fs::write(
            dir.join("index.json"),
            format!(
                r#"{{"schemaVersion":"1.0.0","entries":[{{"id":"{id}","type":"plugin","version":"1.0.0","name":"{id}","manifestPath":"a.json"}}]}}"#
            ),
        )
    }

    async fn ids(cache: &CatalogCache, roots: &[PathBuf]) -> Vec<String> {
        cache
            .entries(roots)
            .await
            .into_iter()
            .map(|(_, entry)| entry.id)
            .collect()
    }

    #[tokio::test]
    async fn two_sets_of_roots_do_not_evict_each_other() -> anyhow::Result<()> {
        let (engine, store) = (tempfile::tempdir()?, tempfile::tempdir()?);
        write_plugin(engine.path(), "installe")?;
        write_plugin(store.path(), "vitrine")?;
        let cache = CatalogCache::default();
        let engine_roots = [engine.path().to_path_buf()];
        let store_roots = [store.path().to_path_buf()];
        assert_eq!(ids(&cache, &engine_roots).await, ["installe"]);
        assert_eq!(ids(&cache, &store_roots).await, ["vitrine"]);

        // Les deux lectures sont gardées malgré les changements sur disque…
        write_plugin(engine.path(), "installe.2")?;
        write_plugin(store.path(), "vitrine.2")?;
        assert_eq!(ids(&cache, &engine_roots).await, ["installe"]);
        assert_eq!(ids(&cache, &store_roots).await, ["vitrine"]);

        // … et l'invalidation les vide toutes deux.
        cache.invalidate().await;
        assert_eq!(ids(&cache, &engine_roots).await, ["installe.2"]);
        assert_eq!(ids(&cache, &store_roots).await, ["vitrine.2"]);
        Ok(())
    }
}
