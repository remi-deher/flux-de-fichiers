//! Lecture et installation déclarative des packs de plugins (`packs/*/installation.json`).

use super::{
    CatalogItemType, RequiredTool, SyncPolicy, catalog_with_roots,
    dependencies::install_tool_with_optional_pool,
};
use crate::{
    error::{AppError, AppResult},
    tools_cache::ToolCacheManager,
};
pub use flux_shared::plugins::{PackDetailView, PackInstallResult, PackInstallation};
use serde_json::Value;
use sqlx::SqlitePool;
use std::path::{Path, PathBuf};

/// Charge le contrat `installation.json` d'un pack du catalogue synchronisé.
pub fn read_pack_installation(roots: &[PathBuf], pack_id: &str) -> AppResult<PackInstallation> {
    let Some((root, entry)) = catalog_with_roots(roots)
        .into_iter()
        .find(|(_, entry)| entry.id == pack_id && entry.entry_type == CatalogItemType::Pack)
    else {
        return Err(AppError::NotFound);
    };

    let manifest_path = root.join(&entry.manifest_path);
    let manifest_raw = std::fs::read_to_string(&manifest_path).map_err(|_| AppError::NotFound)?;
    let manifest_json: Value = serde_json::from_str(&manifest_raw)
        .map_err(|error| AppError::InvalidManifest(error.to_string()))?;
    let declared = manifest_json
        .get("pack")
        .and_then(|pack| pack.get("installation"))
        .and_then(Value::as_str)
        .unwrap_or("installation.json");
    if !ToolCacheManager::is_safe_relative_path(Path::new(declared)) {
        return Err(AppError::InvalidManifest(
            "chemin d'installation de pack invalide".into(),
        ));
    }

    let install_path = manifest_path
        .parent()
        .map(|parent| parent.join(declared))
        .unwrap_or_else(|| root.join(declared));
    let raw = std::fs::read_to_string(&install_path).map_err(|_| AppError::NotFound)?;
    let plan: PackInstallation =
        serde_json::from_str(&raw).map_err(|error| AppError::InvalidManifest(error.to_string()))?;
    if plan.pack_id != pack_id {
        return Err(AppError::InvalidManifest(format!(
            "packId attendu {pack_id}, reçu {}",
            plan.pack_id
        )));
    }
    Ok(plan)
}

/// Construit la fiche détaillée d'un pack (métadonnées du manifeste + plan d'installation).
pub fn build_pack_detail(roots: &[PathBuf], pack_id: &str) -> AppResult<PackDetailView> {
    let Some((root, entry)) = catalog_with_roots(roots)
        .into_iter()
        .find(|(_, entry)| entry.id == pack_id && entry.entry_type == CatalogItemType::Pack)
    else {
        return Err(AppError::NotFound);
    };
    let installation = read_pack_installation(roots, pack_id)?;
    let manifest = super::load_manifest(&root, &entry.manifest_path).ok();
    Ok(PackDetailView {
        id: entry.id,
        name: manifest
            .as_ref()
            .map(|m| m.name.clone())
            .unwrap_or(entry.name),
        version: manifest
            .as_ref()
            .map(|m| m.version.clone())
            .unwrap_or(entry.version),
        description: manifest.as_ref().and_then(|m| m.description.clone()),
        author: manifest.as_ref().and_then(|m| m.author.clone()),
        license: manifest.as_ref().and_then(|m| m.license.clone()),
        category: manifest
            .as_ref()
            .and_then(|m| m.category)
            .or(entry.category),
        tags: manifest
            .as_ref()
            .filter(|m| !m.tags.is_empty())
            .map(|m| m.tags.clone())
            .unwrap_or(entry.tags),
        compatibility: manifest.and_then(|m| m.compatibility).unwrap_or_default(),
        installation,
        updated_at: entry.updated_at,
    })
}

/// Installe les outils et contrôle la présence des composants déclarés par un pack.
pub async fn install_pack(
    roots: &[PathBuf],
    cache: &ToolCacheManager,
    policy: &SyncPolicy,
    pack_id: &str,
) -> AppResult<PackInstallResult> {
    install_pack_with_optional_pool(None, roots, cache, policy, pack_id).await
}

/// Variante vérifiant également l'immuabilité TOFU des outils installés via la base SQLite.
pub async fn install_pack_verified(
    pool: &SqlitePool,
    roots: &[PathBuf],
    cache: &ToolCacheManager,
    policy: &SyncPolicy,
    pack_id: &str,
) -> AppResult<PackInstallResult> {
    install_pack_with_optional_pool(Some(pool), roots, cache, policy, pack_id).await
}

async fn install_pack_with_optional_pool(
    pool: Option<&SqlitePool>,
    roots: &[PathBuf],
    cache: &ToolCacheManager,
    policy: &SyncPolicy,
    pack_id: &str,
) -> AppResult<PackInstallResult> {
    let plan = read_pack_installation(roots, pack_id)?;
    let present_ids: std::collections::HashSet<String> = catalog_with_roots(roots)
        .into_iter()
        .map(|(_, entry)| entry.id)
        .collect();

    let mut installed_tools = Vec::new();
    let mut failed_tools = Vec::new();
    let mut verified_components = Vec::new();
    let mut missing_components = Vec::new();

    for component in &plan.components {
        match component.kind {
            CatalogItemType::App => {
                let tool = RequiredTool {
                    id: component.id.clone(),
                    version: component.version.clone(),
                };
                let outcome =
                    install_tool_with_optional_pool(pool, roots, cache, &tool, policy).await;
                match outcome {
                    Ok(_) => installed_tools.push(component.id.clone()),
                    Err(error) => failed_tools.push((component.id.clone(), error.to_string())),
                }
            }
            CatalogItemType::Pack => {}
            CatalogItemType::Plugin
            | CatalogItemType::Block
            | CatalogItemType::Flow
            | CatalogItemType::Script
            | CatalogItemType::Preset
            | CatalogItemType::ResourceFont
            | CatalogItemType::ResourceImage
            | CatalogItemType::ResourceAudio
            | CatalogItemType::ResourceSubtitle
            | CatalogItemType::ResourceColor
            | CatalogItemType::ResourceLanguage
            | CatalogItemType::ResourceTemplate => {
                if present_ids.contains(&component.id) {
                    verified_components.push(component.id.clone());
                } else {
                    missing_components.push(component.id.clone());
                }
            }
        }
    }

    Ok(PackInstallResult {
        pack_id: plan.pack_id,
        pack_version: plan.pack_version,
        installed_tools,
        failed_tools,
        verified_components,
        missing_components,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_sibling_pack_installation_contract() -> anyhow::Result<()> {
        let sibling = Path::new(env!("CARGO_MANIFEST_DIR")).join("../flux-de-fichiers-plugins");
        if !sibling.join("packs/media/installation.json").is_file() {
            return Ok(());
        }
        let plan = read_pack_installation(&[sibling], "media")?;
        assert_eq!(plan.pack_id, "media");
        assert!(plan.components.iter().any(|c| c.id == "mkvtoolnix"));
        Ok(())
    }

    #[tokio::test]
    async fn install_pack_reports_verified_plugins_and_missing_tools() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let root = dir.path();
        std::fs::create_dir_all(root.join("packs/demo"))?;
        std::fs::write(
            root.join("index.json"),
            r#"{"schemaVersion":"1.0.0","entries":[
                {"id":"pack.demo","type":"pack","version":"1.0.0","name":"Demo","manifestPath":"packs/demo/manifest.json"},
                {"id":"video","type":"plugin","version":"1.0.0","name":"Vid","manifestPath":"plugins/v.json"}
            ]}"#,
        )?;
        std::fs::write(
            root.join("packs/demo/manifest.json"),
            r#"{"id":"pack.demo","type":"pack","version":"1.0.0","pack":{"installation":"installation.json"}}"#,
        )?;
        std::fs::write(
            root.join("packs/demo/installation.json"),
            r#"{"schemaVersion":"1.0.0","packId":"pack.demo","packVersion":"1.0.0","kind":"declarative-bundle","components":[
                {"id":"video","version":"1.0.0","kind":"plugin"},
                {"id":"plugin.absent","version":"1.0.0","kind":"plugin"},
                {"id":"mkvtoolnix","version":"1.0.0","kind":"app"}
            ]}"#,
        )?;

        let cache = ToolCacheManager::new(root.join("cache"));
        let result = install_pack(
            &[root.to_path_buf()],
            &cache,
            &SyncPolicy::default(),
            "pack.demo",
        )
        .await?;
        assert_eq!(result.verified_components, vec!["video"]);
        assert_eq!(result.missing_components, vec!["plugin.absent"]);
        assert_eq!(result.failed_tools.len(), 1);
        assert_eq!(result.failed_tools[0].0, "mkvtoolnix");
        Ok(())
    }
}
