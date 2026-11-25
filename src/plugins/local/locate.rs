//! Dossier d'un élément dans un dossier de catalogue, sans jamais en sortir.

use crate::{
    error::{AppError, AppResult},
    plugins::{CatalogEntry, CatalogItemType, is_valid_repository_id, load_catalog},
    tools_cache::ToolCacheManager,
};
use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
};

/// Dossier canonique de l'élément ; `NotFound` s'il manque ou n'est pas sous `root`.
pub(super) fn item_dir(root: &Path, entry: &CatalogEntry) -> AppResult<PathBuf> {
    let manifest = Path::new(&entry.manifest_path);
    let folder = manifest
        .parent()
        .filter(|_| ToolCacheManager::is_safe_relative_path(manifest))
        .ok_or(AppError::NotFound)?;
    let root = fs::canonicalize(root).map_err(|_| AppError::NotFound)?;
    let dir = fs::canonicalize(root.join(folder)).map_err(|_| AppError::NotFound)?;
    if dir == root || !dir.starts_with(&root) {
        return Err(AppError::NotFound);
    }
    Ok(dir)
}

/// Dossier d'un élément autonome de la collection locale.
pub(in crate::plugins) fn local_item_dir(local_dir: &Path, id: &str) -> AppResult<PathBuf> {
    item_dir(local_dir, &local_entry(local_dir, id)?)
}

/// Dossier d'un script de la collection, où qu'il soit rangé ; `None` s'il n'y est pas.
pub(super) fn script_item_dir(local_dir: &Path, id: &str) -> AppResult<Option<PathBuf>> {
    match local_entry(local_dir, id) {
        Ok(entry) if entry.entry_type == CatalogItemType::Script => {
            item_dir(local_dir, &entry).map(Some)
        }
        Ok(_) | Err(AppError::NotFound) => Ok(None),
        Err(error) => Err(error),
    }
}

fn local_entry(local_dir: &Path, id: &str) -> AppResult<CatalogEntry> {
    if !is_valid_repository_id(id) {
        return Err(AppError::NotFound);
    }
    load_catalog(local_dir)
        .map_err(|_| AppError::NotFound)?
        .entries
        .into_iter()
        .find(|entry| entry.id == id && entry.entry_type.can_stand_alone())
        .ok_or(AppError::NotFound)
}

/// Premier fichier que `files[]` déclare sous ce rôle.
pub(super) fn declared_file<'a>(manifest: &'a Value, role: &str) -> Option<&'a str> {
    manifest
        .get("files")?
        .as_array()?
        .iter()
        .find(|file| file.get("role").and_then(Value::as_str) == Some(role))?
        .get("path")?
        .as_str()
        .filter(|path| ToolCacheManager::is_safe_relative_path(Path::new(path)))
}
