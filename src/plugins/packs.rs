//! Lecture des packs de plugins (`packs/*/installation.json`) ; le Store installe leurs composants.

use super::{CatalogItemType, catalog_with_roots};
use crate::{
    error::{AppError, AppResult},
    tools_cache::ToolCacheManager,
};
pub use forgearr_shared::plugins::{PackDetailView, PackInstallation};
use serde_json::Value;
use std::path::{Path, PathBuf};

/// Charge le contrat `installation.json` d'un pack du catalogue synchronisé.
pub fn read_pack_installation(roots: &[PathBuf], pack_id: &str) -> AppResult<PackInstallation> {
    let Some((root, entry)) = catalog_with_roots(roots)
        .into_iter()
        .find(|(_, entry)| entry.id == pack_id && entry.entry_type == CatalogItemType::Pack)
    else {
        return Err(AppError::NotFound);
    };

    if !ToolCacheManager::is_safe_relative_path(Path::new(&entry.manifest_path)) {
        return Err(AppError::InvalidManifest(
            "chemin de manifeste refusé".into(),
        ));
    }
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
}
