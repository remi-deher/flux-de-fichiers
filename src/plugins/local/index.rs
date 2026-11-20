//! Reconstruction atomique de `index.json` pour le catalogue local.

use super::write_json_pretty;
use crate::{error::AppResult, opaque_token::sha256_hex, tools_cache::ToolCacheManager};
use serde_json::{Map, Value, json};
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};

const LOCAL_FOLDERS: &[&str] = &["scripts", "resources", "templates/blocks", "flows", "packs"];

fn collect_manifests(dir: &Path, out: &mut Vec<(PathBuf, Value)>) {
    let Ok(read_dir) = fs::read_dir(dir) else {
        return;
    };
    let mut subdirs: Vec<PathBuf> = read_dir
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    subdirs.sort();
    for subdir in subdirs {
        let manifest_path = subdir.join("manifest.json");
        if let Ok(content) = fs::read_to_string(&manifest_path)
            && let Ok(value) = serde_json::from_str::<Value>(&content)
        {
            out.push((subdir, value));
        }
    }
}

fn rel_posix(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn hash_declared_files(
    dir: &Path,
    rel_dir: &Path,
    manifest: &Value,
    files_sha256: &mut HashMap<String, String>,
) {
    let Some(files) = manifest.get("files").and_then(Value::as_array) else {
        return;
    };
    for file in files {
        let Some(rel_path) = file.get("path").and_then(Value::as_str) else {
            continue;
        };
        let rel = Path::new(rel_path);
        if !ToolCacheManager::is_safe_relative_path(rel) {
            continue;
        }
        if let Ok(bytes) = fs::read(dir.join(rel)) {
            files_sha256.insert(rel_posix(&rel_dir.join(rel)), sha256_hex(&bytes));
        }
    }
}

fn copy_optional_field(dst: &mut Map<String, Value>, src: &Value, key: &str) {
    if let Some(value) = src.get(key).filter(|v| !v.is_null()) {
        dst.insert(key.to_owned(), value.clone());
    }
}

fn append_script_blocks(
    dir: &Path,
    rel_dir: &Path,
    owner_id: &str,
    manifest: &Value,
    entries: &mut Vec<Value>,
    files_sha256: &mut HashMap<String, String>,
) {
    let Some(files) = manifest.get("files").and_then(Value::as_array) else {
        return;
    };
    for file in files {
        if file.get("role").and_then(Value::as_str) != Some("block") {
            continue;
        }
        let Some(block_rel) = file.get("path").and_then(Value::as_str) else {
            continue;
        };
        let rel = Path::new(block_rel);
        if !ToolCacheManager::is_safe_relative_path(rel) {
            continue;
        }
        let Ok(block_bytes) = fs::read(dir.join(rel)) else {
            continue;
        };
        let Ok(block_val) = serde_json::from_slice::<Value>(&block_bytes) else {
            continue;
        };
        let Some(block_id) = block_val.get("id").and_then(Value::as_str) else {
            continue;
        };
        let Some(block_name) = block_val.get("name").and_then(Value::as_str) else {
            continue;
        };
        let rel_block_path = rel_posix(&rel_dir.join(rel));
        let block_sha = sha256_hex(&block_bytes);
        files_sha256.insert(rel_block_path.clone(), block_sha.clone());

        let mut block_entry = Map::new();
        block_entry.insert("id".into(), Value::String(block_id.to_owned()));
        block_entry.insert("type".into(), Value::String("block".into()));
        block_entry.insert("owner".into(), Value::String(owner_id.to_owned()));
        block_entry.insert(
            "version".into(),
            Value::String(
                block_val
                    .get("version")
                    .and_then(Value::as_str)
                    .unwrap_or("1.0.0")
                    .to_owned(),
            ),
        );
        block_entry.insert("name".into(), Value::String(block_name.to_owned()));
        block_entry.insert("manifestPath".into(), Value::String(rel_block_path));
        block_entry.insert("sha256".into(), Value::String(block_sha));
        copy_optional_field(&mut block_entry, &block_val, "category");
        entries.push(Value::Object(block_entry));
    }
}

/// Régénère atomiquement `index.json` dans `/config/plugins/local` à partir des sous-dossiers présents.
pub fn rebuild_local_index(local_dir: &Path) -> AppResult<()> {
    fs::create_dir_all(local_dir)?;
    let mut manifests = Vec::new();
    for folder in LOCAL_FOLDERS {
        collect_manifests(&local_dir.join(folder), &mut manifests);
    }
    let mut entries = Vec::new();
    let mut files_sha256: HashMap<String, String> = HashMap::new();

    for (dir, value) in manifests {
        let Some(id) = value.get("id").and_then(Value::as_str) else {
            continue;
        };
        let Some(kind) = value.get("type").and_then(Value::as_str) else {
            continue;
        };
        let Some(version) = value.get("version").and_then(Value::as_str) else {
            continue;
        };
        let Some(name) = value.get("name").and_then(Value::as_str) else {
            continue;
        };
        let Ok(rel_dir) = dir.strip_prefix(local_dir) else {
            continue;
        };
        let rel_manifest = rel_posix(&rel_dir.join("manifest.json"));
        let manifest_sha = fs::read(dir.join("manifest.json"))
            .ok()
            .map(|bytes| sha256_hex(&bytes));
        if let Some(ref sha) = manifest_sha {
            files_sha256.insert(rel_manifest.clone(), sha.clone());
        }
        hash_declared_files(&dir, rel_dir, &value, &mut files_sha256);

        let mut entry = Map::new();
        entry.insert("id".into(), Value::String(id.to_owned()));
        entry.insert("type".into(), Value::String(kind.to_owned()));
        entry.insert("version".into(), Value::String(version.to_owned()));
        entry.insert("name".into(), Value::String(name.to_owned()));
        entry.insert("manifestPath".into(), Value::String(rel_manifest));
        if let Some(sha) = manifest_sha {
            entry.insert("sha256".into(), Value::String(sha));
        }
        copy_optional_field(&mut entry, &value, "category");
        copy_optional_field(&mut entry, &value, "capability");
        if let Some(tags) = value.get("tags").and_then(Value::as_array) {
            entry.insert("tags".into(), Value::Array(tags.clone()));
        }
        entries.push(Value::Object(entry));

        if kind == "script" {
            append_script_blocks(&dir, rel_dir, id, &value, &mut entries, &mut files_sha256);
        }
    }

    let index_json = json!({
        "schemaVersion": "1.0.0",
        "generatedFor": "local",
        "filesSha256": files_sha256,
        "entries": entries,
    });
    let tmp = local_dir.join("index.json.tmp");
    write_json_pretty(&tmp, &index_json)?;
    fs::rename(tmp, local_dir.join("index.json"))?;
    Ok(())
}
