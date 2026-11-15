//! Cache en mémoire du catalogue de plugins, invalidé lors des synchronisations et mutations de dépôts.

use super::{
    CatalogEntry, CatalogItemType, SettingsField, ToolStatusSummary, catalog_with_roots,
    default_tool_version, flow_block_catalog, load_manifest, parse_distribution, read_distribution,
    settings_fields as load_settings_fields, summarize_tool,
};
use crate::tools_cache::ToolCacheManager;
use flux_shared::flows::OwnedBlockSpec;
use std::{path::PathBuf, sync::Arc};
use tokio::sync::RwLock;

#[derive(Clone)]
struct CachedSnapshot {
    roots: Vec<PathBuf>,
    entries: Vec<(PathBuf, CatalogEntry)>,
    blocks: Vec<OwnedBlockSpec>,
    tool_descriptions: Vec<(CatalogEntry, String)>,
}

/// Cache thread-safe des métadonnées du catalogue lues depuis les dépôts synchronisés.
#[derive(Clone, Default)]
pub struct CatalogCache {
    inner: Arc<RwLock<Option<CachedSnapshot>>>,
}

impl CatalogCache {
    pub fn new() -> Self {
        Self::default()
    }

    /// Vide le cache afin que la prochaine lecture recharge les dépôts depuis le disque.
    pub async fn invalidate(&self) {
        *self.inner.write().await = None;
    }

    async fn snapshot(&self, roots: &[PathBuf]) -> CachedSnapshot {
        if let Some(cached) = self.inner.read().await.as_ref()
            && cached.roots == roots
        {
            return cached.clone();
        }
        let owned_roots = roots.to_vec();
        let loaded = tokio::task::spawn_blocking(move || build_snapshot(owned_roots))
            .await
            .unwrap_or_else(|_| build_snapshot(roots.to_vec()));
        let mut guard = self.inner.write().await;
        if let Some(cached) = guard.as_ref()
            && cached.roots == roots
        {
            return cached.clone();
        }
        *guard = Some(loaded.clone());
        loaded
    }

    pub async fn exposed_catalog(&self, roots: &[PathBuf]) -> Vec<CatalogEntry> {
        self.snapshot(roots)
            .await
            .entries
            .into_iter()
            .filter_map(|(_, entry)| entry.entry_type.section().map(|_| entry))
            .collect()
    }

    pub async fn blocks(&self, roots: &[PathBuf]) -> Vec<OwnedBlockSpec> {
        self.snapshot(roots).await.blocks
    }

    pub async fn tools_status(
        &self,
        roots: &[PathBuf],
        cache_mgr: &ToolCacheManager,
    ) -> Vec<ToolStatusSummary> {
        self.snapshot(roots)
            .await
            .tool_descriptions
            .into_iter()
            .map(|(entry, description)| summarize_tool(entry, description, cache_mgr))
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
        if let Ok(value) = read_distribution(roots, tool_id)
            && let Ok(dist) = parse_distribution(&value.to_string())
            && !dist.tool_version.trim().is_empty()
        {
            return dist.tool_version;
        }
        snapshot
            .entries
            .iter()
            .find(|(_, entry)| entry.id == tool_id)
            .map(|(_, entry)| entry.version.clone())
            .filter(|version| !version.trim().is_empty())
            .unwrap_or_else(|| default_tool_version(tool_id).to_owned())
    }
}

fn build_snapshot(roots: Vec<PathBuf>) -> CachedSnapshot {
    let entries = catalog_with_roots(&roots);
    let blocks = flow_block_catalog(&roots);
    let tool_descriptions = entries
        .iter()
        .filter(|(_, entry)| entry.entry_type == CatalogItemType::App)
        .map(|(root, entry)| {
            let description = load_manifest(root, &entry.manifest_path)
                .ok()
                .and_then(|manifest| manifest.description)
                .unwrap_or_default();
            (entry.clone(), description)
        })
        .collect();
    CachedSnapshot {
        roots,
        entries,
        blocks,
        tool_descriptions,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn cache_reflects_invalidation_after_disk_change() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        std::fs::create_dir_all(dir.path().join("catalogue"))?;
        std::fs::write(
            dir.path().join("catalogue/index.json"),
            r#"{"schemaVersion":"1.0.0","entries":[
                {"id":"plugin.a","type":"plugin","version":"1.0.0","name":"A","manifestPath":"a.json"}
            ]}"#,
        )?;
        let cache = CatalogCache::new();
        let roots = [dir.path().to_path_buf()];
        assert_eq!(cache.exposed_catalog(&roots).await.len(), 1);

        std::fs::write(
            dir.path().join("catalogue/index.json"),
            r#"{"schemaVersion":"1.0.0","entries":[
                {"id":"plugin.a","type":"plugin","version":"1.0.0","name":"A","manifestPath":"a.json"},
                {"id":"app.mkvtoolnix","type":"app","version":"2.1.0","name":"MKV","manifestPath":"mkv.json"}
            ]}"#,
        )?;
        // Sans invalidation, le cache garde la vue précédente.
        assert_eq!(cache.exposed_catalog(&roots).await.len(), 1);

        cache.invalidate().await;
        assert_eq!(cache.exposed_catalog(&roots).await.len(), 2);
        assert_eq!(
            cache.resolve_tool_version(&roots, "app.mkvtoolnix").await,
            "2.1.0"
        );
        Ok(())
    }
}
