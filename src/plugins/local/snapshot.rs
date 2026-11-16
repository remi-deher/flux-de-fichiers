//! Lecture de l'instantané consolidé de la librairie locale et résolution sûre des ressources.

use super::{is_safe_filename, normalize_description};
use crate::{
    error::{AppError, AppResult},
    plugins::{
        CatalogEntry, CatalogItemType, catalog_with_roots, is_valid_repository_id, load_catalog,
        vocab::CatalogSection,
    },
    tools_cache::ToolCacheManager,
};
use flux_shared::local::{
    LocalBlockTemplate, LocalFlowTemplate, LocalLibrarySnapshot, LocalResourceSummary,
    LocalScriptSummary, ScriptInterpreter,
};
use serde_json::Value;
use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
};

fn read_local_script(root: &Path, entry: &CatalogEntry) -> Option<LocalScriptSummary> {
    let manifest_path = root.join(&entry.manifest_path);
    let item_dir = manifest_path.parent()?;
    let val: Value = serde_json::from_str(&fs::read_to_string(&manifest_path).ok()?).ok()?;
    let op_val: Value =
        serde_json::from_str(&fs::read_to_string(item_dir.join("operations/run.json")).ok()?)
            .ok()?;
    let runtime = op_val.get("runtime")?;
    let interpreter: ScriptInterpreter =
        serde_json::from_value(runtime.get("interpreter")?.clone()).ok()?;
    let entrypoint = runtime
        .get("entrypoint")
        .and_then(Value::as_str)?
        .to_owned();
    if !is_safe_filename(&entrypoint) {
        return None;
    }
    let code = fs::read_to_string(item_dir.join(&entrypoint)).unwrap_or_default();
    let slug = entry
        .id
        .strip_prefix("script.")
        .unwrap_or(&entry.id)
        .to_owned();
    Some(LocalScriptSummary {
        id: entry.id.clone(),
        slug,
        name: entry.name.clone(),
        description: normalize_description(val.get("description").and_then(Value::as_str)),
        interpreter,
        entrypoint,
        code,
        category: entry.category,
        tags: entry.tags.clone(),
    })
}

fn read_local_resource(root: &Path, entry: &CatalogEntry) -> Option<LocalResourceSummary> {
    let manifest_path = root.join(&entry.manifest_path);
    let item_dir = manifest_path.parent()?;
    let val: Value = serde_json::from_str(&fs::read_to_string(&manifest_path).ok()?).ok()?;
    let asset = val
        .get("files")
        .and_then(Value::as_array)?
        .iter()
        .find(|f| f.get("role").and_then(Value::as_str) == Some("asset"))?;
    let filename = asset.get("path").and_then(Value::as_str)?.to_owned();
    if !is_safe_filename(&filename) {
        return None;
    }
    let size_bytes = fs::metadata(item_dir.join(&filename))
        .map(|m| m.len())
        .unwrap_or(0);
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
        filename,
        size_bytes,
        description: normalize_description(val.get("description").and_then(Value::as_str)),
    })
}

/// Résout en lecture seule le chemin disque sûr et le nom de fichier d'une ressource locale importée.
pub fn local_resource_file(local_dir: &Path, id: &str) -> AppResult<(PathBuf, String)> {
    if !is_valid_repository_id(id) {
        return Err(AppError::NotFound);
    }
    let Ok(index) = load_catalog(local_dir) else {
        return Err(AppError::NotFound);
    };
    let Some(entry) = index.entries.into_iter().find(|entry| {
        entry.id == id
            && entry.entry_type.section() == Some(CatalogSection::Other)
            && entry.entry_type != CatalogItemType::ResourceTemplate
    }) else {
        return Err(AppError::NotFound);
    };
    let rel_manifest = Path::new(&entry.manifest_path);
    if !ToolCacheManager::is_safe_relative_path(rel_manifest) {
        return Err(AppError::NotFound);
    }
    let Some(summary) = read_local_resource(local_dir, &entry) else {
        return Err(AppError::NotFound);
    };
    let Some(item_dir_rel) = rel_manifest.parent() else {
        return Err(AppError::NotFound);
    };
    let canonical_root = fs::canonicalize(local_dir).map_err(|_| AppError::NotFound)?;
    let canonical_item =
        fs::canonicalize(canonical_root.join(item_dir_rel)).map_err(|_| AppError::NotFound)?;
    let canonical_file =
        fs::canonicalize(canonical_item.join(&summary.filename)).map_err(|_| AppError::NotFound)?;
    if !canonical_item.starts_with(&canonical_root)
        || !canonical_file.starts_with(&canonical_item)
        || !canonical_file.is_file()
    {
        return Err(AppError::NotFound);
    }
    Ok((canonical_file, summary.filename))
}

fn read_block_template(
    root: &Path,
    entry: &CatalogEntry,
    is_local: bool,
) -> Option<LocalBlockTemplate> {
    let content = fs::read_to_string(root.join(&entry.manifest_path)).ok()?;
    let val = serde_json::from_str::<Value>(&content).ok()?;
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
    let content = fs::read_to_string(root.join(&entry.manifest_path)).ok()?;
    let val = serde_json::from_str::<Value>(&content).ok()?;
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
