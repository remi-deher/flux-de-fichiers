//! Lecture de l'instantané consolidé de la librairie locale et résolution sûre des ressources.

use super::{declared_file, is_safe_filename, item_dir, normalize_description, read_local_script};
use crate::{
    error::{AppError, AppResult},
    plugins::{
        CatalogEntry, CatalogItemType, catalog_with_roots, load_catalog, vocab::CatalogSection,
    },
};
use forgearr_shared::local::{
    LocalBlockTemplate, LocalFlowTemplate, LocalLibrarySnapshot, LocalResourceSummary,
};
use serde_json::Value;
use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
};

fn read_manifest(root: &Path, entry: &CatalogEntry) -> Option<Value> {
    let text = fs::read_to_string(root.join(&entry.manifest_path)).ok()?;
    serde_json::from_str(&text).ok()
}

/// Chemin du fichier d'une ressource, et son nom.
fn asset_of(manifest: &Value) -> Option<(&str, &str)> {
    let path = declared_file(manifest, "asset")?;
    let filename = Path::new(path).file_name()?.to_str()?;
    is_safe_filename(filename).then_some((path, filename))
}

fn read_local_resource(root: &Path, entry: &CatalogEntry) -> Option<LocalResourceSummary> {
    let manifest = read_manifest(root, entry)?;
    let (path, filename) = asset_of(&manifest)?;
    let file = root.join(&entry.manifest_path).parent()?.join(path);
    let size_bytes = fs::metadata(file).map_or(0, |metadata| metadata.len());
    let slug = entry
        .id
        .strip_prefix("resource.")
        .unwrap_or(&entry.id)
        .to_owned();
    Some(LocalResourceSummary {
        id: entry.id.clone(),
        slug,
        name: entry.name.clone(),
        entry_type: entry.entry_type,
        filename: filename.to_owned(),
        size_bytes,
        description: normalize_description(manifest.get("description").and_then(Value::as_str)),
    })
}

/// Fichier d'une ressource et son nom, dans le premier de ces dossiers de catalogue qui la porte.
pub fn resource_file(roots: &[PathBuf], id: &str) -> AppResult<(PathBuf, String)> {
    let (root, entry) = catalog_with_roots(roots)
        .into_iter()
        .find(|(_, entry)| {
            entry.id == id
                && entry.entry_type.section() == Some(CatalogSection::Other)
                && entry.entry_type != CatalogItemType::ResourceTemplate
        })
        .ok_or(AppError::NotFound)?;
    let dir = item_dir(&root, &entry)?;
    let manifest = read_manifest(&root, &entry).ok_or(AppError::NotFound)?;
    let (path, filename) = asset_of(&manifest).ok_or(AppError::NotFound)?;
    let file = fs::canonicalize(dir.join(path)).map_err(|_| AppError::NotFound)?;
    if !file.starts_with(&dir) || !file.is_file() {
        return Err(AppError::NotFound);
    }
    Ok((file, filename.to_owned()))
}

fn read_block_template(
    root: &Path,
    entry: &CatalogEntry,
    is_local: bool,
) -> Option<LocalBlockTemplate> {
    let val = read_manifest(root, entry)?;
    let definition = val.get("definition").cloned()?;
    Some(LocalBlockTemplate {
        id: entry.id.clone(),
        name: entry.name.clone(),
        description: normalize_description(val.get("description").and_then(Value::as_str)),
        definition,
        tags: entry.tags.clone(),
        is_local,
    })
}

fn read_flow_template(
    root: &Path,
    entry: &CatalogEntry,
    is_local: bool,
) -> Option<LocalFlowTemplate> {
    let val = read_manifest(root, entry)?;
    let definition = val.get("definition").cloned()?;
    Some(LocalFlowTemplate {
        id: entry.id.clone(),
        name: entry.name.clone(),
        description: normalize_description(val.get("description").and_then(Value::as_str)),
        file_mask: val
            .get("fileMask")
            .and_then(Value::as_str)
            .unwrap_or("*")
            .to_owned(),
        version: entry.version.clone(),
        definition,
        is_local,
    })
}

/// Construit la vue complète de la librairie locale et des modèles disponibles (locaux + dépôts).
pub fn build_local_snapshot(roots: &[PathBuf], local_dir: &Path) -> LocalLibrarySnapshot {
    let mut snapshot = LocalLibrarySnapshot::default();
    let local_canon = fs::canonicalize(local_dir).ok();
    let mut seen_flows = HashSet::new();
    let mut seen_blocks = HashSet::new();

    if let Ok(local_index) = load_catalog(local_dir) {
        for entry in local_index.entries {
            match entry.entry_type {
                CatalogItemType::Script => {
                    if let Some(script) = read_local_script(local_dir, &entry) {
                        snapshot.scripts.push(script);
                    }
                }
                CatalogItemType::Pack => snapshot.packs.push(entry),
                CatalogItemType::ResourceTemplate => {
                    if let Some(tpl) = read_block_template(local_dir, &entry, true) {
                        seen_blocks.insert(tpl.id.clone());
                        snapshot.block_templates.push(tpl);
                    } else if let Some(res) = read_local_resource(local_dir, &entry) {
                        snapshot.resources.push(res);
                    }
                }
                t if t.section() == Some(CatalogSection::Other) => {
                    if let Some(res) = read_local_resource(local_dir, &entry) {
                        snapshot.resources.push(res);
                    }
                }
                _ => {}
            }
        }
    }

    for (root, entry) in catalog_with_roots(roots) {
        let is_local = local_canon
            .as_ref()
            .zip(fs::canonicalize(&root).ok())
            .is_some_and(|(l, r)| l == &r);
        match entry.entry_type {
            CatalogItemType::Flow if seen_flows.insert(entry.id.clone()) => {
                if let Some(tpl) = read_flow_template(&root, &entry, is_local) {
                    snapshot.flow_templates.push(tpl);
                }
            }
            CatalogItemType::ResourceTemplate if seen_blocks.insert(entry.id.clone()) => {
                if let Some(tpl) = read_block_template(&root, &entry, is_local) {
                    snapshot.block_templates.push(tpl);
                }
            }
            _ => {}
        }
    }

    snapshot
}
