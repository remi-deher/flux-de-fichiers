//! Gestion du dépôt local utilisateur (`/config/plugins/local`) sans dépendre d'un dépôt GitHub.

use super::{
    CatalogEntry, CatalogItemType, catalog_with_roots, is_valid_repository_id, load_catalog,
    vocab::CatalogSection,
};
use crate::{
    error::{AppError, AppResult},
    flows::ScriptInterpreter,
    tools_cache::ToolCacheManager,
};
use flux_shared::base64;
use flux_shared::local::{
    CreateBlockTemplateRequest, CreateFlowTemplateRequest, CreateLocalPackRequest,
    LocalBlockTemplate, LocalFlowTemplate, LocalLibrarySnapshot, LocalResourceSummary,
    LocalScriptSummary, UploadLocalResourceRequest, UpsertLocalScriptRequest,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
};

const MAX_SCRIPT_BYTES: usize = 512 * 1024;
const MAX_RESOURCE_BYTES: usize = 5 * 1024 * 1024;
const BLOCK_TEMPLATE_TAG: &str = "block-template";

/// Normalise un libellé en slug ASCII sûr (`a-z0-9-`).
pub fn slugify(raw: &str) -> String {
    let mut out = String::new();
    let mut prev_dash = false;
    for ch in raw.trim().chars().flat_map(char::to_lowercase) {
        let ch = match ch {
            'à' | 'â' | 'ä' => 'a',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'î' | 'ï' => 'i',
            'ô' | 'ö' => 'o',
            'ù' | 'û' | 'ü' => 'u',
            'ç' => 'c',
            other => other,
        };
        if ch.is_ascii_alphanumeric() {
            out.push(ch);
            prev_dash = false;
        } else if !prev_dash && !out.is_empty() {
            out.push('-');
            prev_dash = true;
        }
    }
    let trimmed = out.trim_matches('-');
    if trimmed.is_empty() {
        "item".to_owned()
    } else {
        trimmed.chars().take(48).collect()
    }
}

fn is_safe_filename(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 120
        && !name.starts_with('.')
        && !name.contains(['/', '\\', '\0'])
        && name != ".."
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
}

fn parse_interpreter(raw: &str) -> AppResult<ScriptInterpreter> {
    serde_json::from_value(Value::String(raw.trim().to_ascii_lowercase()))
        .map_err(|_| AppError::InvalidManifest(format!("interpréteur inconnu : {raw}")))
}

fn default_script_extension(interp: ScriptInterpreter) -> &'static str {
    match interp {
        ScriptInterpreter::Bash | ScriptInterpreter::Sh => "sh",
        ScriptInterpreter::Python3 => "py",
        ScriptInterpreter::Node | ScriptInterpreter::Deno | ScriptInterpreter::Bun => "js",
        ScriptInterpreter::Pwsh => "ps1",
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

fn write_json_pretty(path: &Path, value: &Value) -> AppResult<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let formatted = serde_json::to_string_pretty(value)
        .map_err(|err| AppError::InvalidManifest(err.to_string()))?;
    fs::write(path, formatted)?;
    Ok(())
}

fn slug_from_prefixed_id(id: Option<&str>, prefix: &str, name: &str) -> AppResult<String> {
    let candidate = match id.map(str::trim).filter(|s| !s.is_empty()) {
        Some(existing) => {
            if !is_valid_repository_id(existing) {
                return Err(AppError::InvalidManifest(
                    "identifiant local invalide".into(),
                ));
            }
            existing.strip_prefix(prefix).unwrap_or(existing).to_owned()
        }
        None => slugify(name),
    };
    let slug = slugify(&candidate);
    if !is_valid_repository_id(&format!("{prefix}{slug}")) {
        return Err(AppError::InvalidManifest(
            "identifiant local invalide".into(),
        ));
    }
    Ok(slug)
}

/// Enregistre ou met à jour un script local et génère son contrat d'opération + bloc palette.
pub fn save_local_script(
    local_dir: &Path,
    req: UpsertLocalScriptRequest,
) -> AppResult<LocalScriptSummary> {
    let name = req.name.trim();
    if name.is_empty() {
        return Err(AppError::InvalidManifest(
            "le nom du script est requis".into(),
        ));
    }
    if req.code.len() > MAX_SCRIPT_BYTES {
        return Err(AppError::TooLarge);
    }
    let interp = parse_interpreter(&req.interpreter)?;
    let slug = slug_from_prefixed_id(req.id.as_deref(), "script.", name)?;
    let id = format!("script.{slug}");
    let entrypoint = match req
        .filename
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        Some(f) if is_safe_filename(f) => f.to_owned(),
        Some(_) => {
            return Err(AppError::InvalidManifest(
                "nom de fichier script invalide".into(),
            ));
        }
        None => format!("run.{}", default_script_extension(interp)),
    };

    let base_dir = local_dir.join("scripts").join(&slug);
    fs::create_dir_all(base_dir.join("operations"))?;
    fs::create_dir_all(base_dir.join("blocks"))?;

    fs::write(base_dir.join(&entrypoint), req.code.as_bytes())?;

    let op_id = format!("operation.script.{slug}");
    let block_id = format!("block.script.{slug}");
    let description = req
        .description
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned);
    let interp_name = interp.executable();

    let operation_json = json!({
        "schemaVersion": "1.1.0",
        "id": op_id,
        "version": "1.0.0",
        "name": name,
        "providerId": id,
        "providerKind": "script",
        "capability": "script",
        "status": "validated",
        "runtime": {
            "kind": "script",
            "interpreter": interp_name,
            "entrypoint": entrypoint,
            "timeoutSeconds": 300
        }
    });
    write_json_pretty(&base_dir.join("operations/run.json"), &operation_json)?;

    let block_json = json!({
        "schemaVersion": "1.1.0",
        "id": block_id,
        "version": "1.0.0",
        "name": name,
        "description": description.as_deref().unwrap_or("Script local personnalisé (stdio JSON)."),
        "category": "data",
        "operationId": op_id,
        "operationVersion": "1.0.0",
        "sockets": {
            "inputs": [{ "id": "source", "label": "Fichier traité", "dataType": "file", "mediaType": "*/*" }],
            "outputs": [{ "id": "destination", "label": "Fichier courant", "dataType": "file", "mediaType": "*/*" }]
        },
        "ui": {
            "category": "Scripts locaux",
            "domain": "data",
            "fields": []
        }
    });
    write_json_pretty(&base_dir.join("blocks/run.json"), &block_json)?;

    let manifest_json = json!({
        "schemaVersion": "1.1.0",
        "id": id,
        "type": "script",
        "version": "1.0.0",
        "name": name,
        "description": description.as_deref().unwrap_or("Script local personnalisé."),
        "author": "local",
        "license": "UNLICENSED",
        "category": req.category.map(|c| c.as_str()).unwrap_or("other"),
        "tags": req.tags,
        "files": [
            { "path": entrypoint, "role": "script", "mediaType": "text/plain" },
            { "path": "operations/run.json", "role": "operation", "mediaType": "application/json" },
            { "path": "blocks/run.json", "role": "block", "mediaType": "application/json" }
        ]
    });
    write_json_pretty(&base_dir.join("manifest.json"), &manifest_json)?;
    rebuild_local_index(local_dir)?;

    Ok(LocalScriptSummary {
        id,
        slug,
        name: name.to_owned(),
        description,
        interpreter: interp_name.to_owned(),
        entrypoint,
        code: req.code,
        category: req.category,
        tags: req.tags,
    })
}

/// Importe un fichier ressource local (image, sous-titre, son, police, modèle…).
pub fn upload_local_resource(
    local_dir: &Path,
    req: UploadLocalResourceRequest,
) -> AppResult<LocalResourceSummary> {
    let name = req.name.trim();
    if name.is_empty() {
        return Err(AppError::InvalidManifest(
            "le nom de la ressource est requis".into(),
        ));
    }
    if req.entry_type.section() != Some(CatalogSection::Other) {
        return Err(AppError::InvalidManifest(
            "seuls les types de ressource (image, audio, sous-titre, police, modèle, etc.) sont autorisés".into(),
        ));
    }
    let filename = req.filename.trim();
    if !is_safe_filename(filename) {
        return Err(AppError::InvalidManifest("nom de fichier invalide".into()));
    }
    let bytes = base64::decode(&req.content_base64)
        .map_err(|err| AppError::InvalidManifest(err.to_owned()))?;
    if bytes.is_empty() || bytes.len() > MAX_RESOURCE_BYTES {
        return Err(if bytes.len() > MAX_RESOURCE_BYTES {
            AppError::TooLarge
        } else {
            AppError::InvalidManifest("le fichier est vide".into())
        });
    }

    let slug = slugify(name);
    let id = format!("resource.{slug}");
    let base_dir = local_dir.join("resources").join(&slug);
    fs::create_dir_all(&base_dir)?;
    fs::write(base_dir.join(filename), &bytes)?;

    let description = req
        .description
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned);
    let manifest_json = json!({
        "schemaVersion": "1.1.0",
        "id": id,
        "type": req.entry_type.as_str(),
        "version": "1.0.0",
        "name": name,
        "description": description.as_deref().unwrap_or("Ressource importée localement."),
        "author": "local",
        "license": "UNLICENSED",
        "files": [
            { "path": filename, "role": "asset", "mediaType": "application/octet-stream" }
        ]
    });
    write_json_pretty(&base_dir.join("manifest.json"), &manifest_json)?;
    rebuild_local_index(local_dir)?;

    Ok(LocalResourceSummary {
        id,
        slug,
        name: name.to_owned(),
        entry_type: req.entry_type,
        filename: filename.to_owned(),
        size_bytes: bytes.len() as u64,
        description,
    })
}

/// Enregistre un modèle de blocs (sous-graphe réutilisable dans l'éditeur).
pub fn save_local_block_template(
    local_dir: &Path,
    req: CreateBlockTemplateRequest,
) -> AppResult<LocalBlockTemplate> {
    let name = req.name.trim();
    if name.is_empty() {
        return Err(AppError::InvalidManifest(
            "le nom du modèle de blocs est requis".into(),
        ));
    }
    let slug = slug_from_prefixed_id(req.id.as_deref(), "template.block.", name)?;
    let id = format!("template.block.{slug}");
    let base_dir = local_dir.join("templates/blocks").join(&slug);
    fs::create_dir_all(&base_dir)?;

    let description = req
        .description
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned);
    let mut tags = req.tags;
    if !tags.iter().any(|t| t == BLOCK_TEMPLATE_TAG) {
        tags.push(BLOCK_TEMPLATE_TAG.to_owned());
    }
    let manifest_json = json!({
        "schemaVersion": "1.1.0",
        "id": id,
        "type": "resource.template",
        "version": "1.0.0",
        "name": name,
        "description": description.as_deref().unwrap_or("Modèle de blocs réutilisable."),
        "author": "local",
        "license": "UNLICENSED",
        "tags": tags,
        "definition": req.definition,
    });
    write_json_pretty(&base_dir.join("manifest.json"), &manifest_json)?;
    rebuild_local_index(local_dir)?;

    Ok(LocalBlockTemplate {
        id,
        name: name.to_owned(),
        description,
        definition: req.definition,
        tags,
        is_local: true,
    })
}

/// Enregistre un modèle de flow local (`type: "flow"`).
pub fn save_local_flow_template(
    local_dir: &Path,
    req: CreateFlowTemplateRequest,
) -> AppResult<LocalFlowTemplate> {
    let name = req.name.trim();
    if name.is_empty() {
        return Err(AppError::InvalidManifest(
            "le nom du modèle de flow est requis".into(),
        ));
    }
    let slug = slug_from_prefixed_id(req.id.as_deref(), "flow.template.", name)?;
    let id = format!("flow.template.{slug}");
    let base_dir = local_dir.join("flows").join(&slug);
    fs::create_dir_all(&base_dir)?;

    let description = req
        .description
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned);
    let file_mask = req
        .file_mask
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("*")
        .to_owned();

    let manifest_json = json!({
        "schemaVersion": "1.1.0",
        "id": id,
        "type": "flow",
        "version": "1.0.0",
        "name": name,
        "description": description.as_deref().unwrap_or("Modèle de flow local."),
        "author": "local",
        "license": "UNLICENSED",
        "fileMask": file_mask,
        "definition": req.definition,
    });
    write_json_pretty(&base_dir.join("manifest.json"), &manifest_json)?;
    rebuild_local_index(local_dir)?;

    Ok(LocalFlowTemplate {
        id,
        name: name.to_owned(),
        description,
        file_mask,
        version: "1.0.0".to_owned(),
        definition: req.definition,
        is_local: true,
    })
}

/// Crée un pack local regroupant des scripts, ressources, flows ou plugins.
pub fn save_local_pack(local_dir: &Path, req: CreateLocalPackRequest) -> AppResult<CatalogEntry> {
    let name = req.name.trim();
    if name.is_empty() {
        return Err(AppError::InvalidManifest(
            "le nom du pack est requis".into(),
        ));
    }
    if req.components.is_empty() {
        return Err(AppError::InvalidManifest(
            "sélectionnez au moins un composant".into(),
        ));
    }
    let slug = slug_from_prefixed_id(req.id.as_deref(), "pack.local.", name)?;
    let id = format!("pack.local.{slug}");
    let base_dir = local_dir.join("packs").join(&slug);
    fs::create_dir_all(&base_dir)?;

    let description = req
        .description
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("Pack local assemblé depuis la librairie.");

    let installation_json = json!({
        "schemaVersion": "1.0.0",
        "packId": id,
        "packVersion": "1.0.0",
        "kind": "declarative-bundle",
        "components": req.components,
        "entrypoint": req.entrypoint,
        "runtimeImplemented": true
    });
    write_json_pretty(&base_dir.join("installation.json"), &installation_json)?;

    let manifest_json = json!({
        "schemaVersion": "1.1.0",
        "id": id,
        "type": "pack",
        "version": "1.0.0",
        "name": name,
        "description": description,
        "author": "local",
        "license": "UNLICENSED",
        "files": [
            { "path": "installation.json", "role": "config", "mediaType": "application/json" }
        ],
        "pack": {
            "kind": "declarative-bundle",
            "installation": "installation.json",
            "runtimeImplemented": true
        }
    });
    write_json_pretty(&base_dir.join("manifest.json"), &manifest_json)?;
    rebuild_local_index(local_dir)?;

    Ok(CatalogEntry {
        id,
        entry_type: CatalogItemType::Pack,
        version: "1.0.0".to_owned(),
        name: name.to_owned(),
        manifest_path: format!("packs/{slug}/manifest.json"),
        capability: None,
        current_choice: None,
        distribution: None,
        category: None,
        tags: Vec::new(),
        owner: None,
        sha256: None,
    })
}

/// Supprime un élément du catalogue local (par son `id`) et reconstruit `index.json`.
pub fn delete_local_item(local_dir: &Path, id: &str) -> AppResult<()> {
    if !is_valid_repository_id(id) {
        return Err(AppError::NotFound);
    }
    let Ok(index) = load_catalog(local_dir) else {
        return Err(AppError::NotFound);
    };
    let Some(entry) = index
        .entries
        .into_iter()
        .find(|entry| entry.id == id && entry.entry_type.can_stand_alone())
    else {
        return Err(AppError::NotFound);
    };
    let rel_manifest = Path::new(&entry.manifest_path);
    if !ToolCacheManager::is_safe_relative_path(rel_manifest) {
        return Err(AppError::NotFound);
    }
    let Some(item_dir_rel) = rel_manifest.parent() else {
        return Err(AppError::NotFound);
    };
    let canonical_root = fs::canonicalize(local_dir)?;
    let canonical_item = fs::canonicalize(canonical_root.join(item_dir_rel))?;
    if !canonical_item.starts_with(&canonical_root) || canonical_item == canonical_root {
        return Err(AppError::NotFound);
    }
    fs::remove_dir_all(canonical_item)?;
    rebuild_local_index(local_dir)?;
    Ok(())
}

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

/// Régénère atomiquement `index.json` dans `/config/plugins/local` à partir des sous-dossiers présents.
pub fn rebuild_local_index(local_dir: &Path) -> AppResult<()> {
    fs::create_dir_all(local_dir)?;
    let mut manifests = Vec::new();
    for folder in ["scripts", "resources", "templates/blocks", "flows", "packs"] {
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
        let rel_manifest = rel_dir
            .join("manifest.json")
            .to_string_lossy()
            .replace('\\', "/");
        if let Ok(bytes) = fs::read(dir.join("manifest.json")) {
            files_sha256.insert(rel_manifest.clone(), sha256_hex(&bytes));
        }
        let mut entry = json!({
            "id": id,
            "type": kind,
            "version": version,
            "name": name,
            "manifestPath": rel_manifest,
        });
        if let Some(tags) = value.get("tags").and_then(Value::as_array) {
            entry["tags"] = Value::Array(tags.clone());
        }
        entries.push(entry);

        if kind == "script"
            && let Some(files) = value.get("files").and_then(Value::as_array)
        {
            for file in files {
                if file.get("role").and_then(Value::as_str) != Some("block") {
                    continue;
                }
                let Some(block_rel) = file.get("path").and_then(Value::as_str) else {
                    continue;
                };
                let block_path = dir.join(block_rel);
                let Ok(block_bytes) = fs::read(&block_path) else {
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
                let rel_block_path = rel_dir.join(block_rel).to_string_lossy().replace('\\', "/");
                files_sha256.insert(rel_block_path.clone(), sha256_hex(&block_bytes));
                entries.push(json!({
                    "id": block_id,
                    "type": "block",
                    "owner": id,
                    "version": block_val.get("version").and_then(Value::as_str).unwrap_or("1.0.0"),
                    "name": block_name,
                    "manifestPath": rel_block_path,
                }));
            }
        }
    }

    let index_json = json!({
        "schemaVersion": "1.0.0",
        "filesSha256": files_sha256,
        "entries": entries,
    });
    let tmp = local_dir.join("index.json.tmp");
    write_json_pretty(&tmp, &index_json)?;
    fs::rename(tmp, local_dir.join("index.json"))?;
    Ok(())
}

fn read_local_script(root: &Path, entry: &CatalogEntry) -> Option<LocalScriptSummary> {
    let manifest_path = root.join(&entry.manifest_path);
    let item_dir = manifest_path.parent()?;
    let val: Value = serde_json::from_str(&fs::read_to_string(&manifest_path).ok()?).ok()?;
    let op_val: Value =
        serde_json::from_str(&fs::read_to_string(item_dir.join("operations/run.json")).ok()?)
            .ok()?;
    let runtime = op_val.get("runtime")?;
    let interpreter = runtime
        .get("interpreter")
        .and_then(Value::as_str)?
        .to_owned();
    let entrypoint = runtime
        .get("entrypoint")
        .and_then(Value::as_str)?
        .to_owned();
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
        description: val
            .get("description")
            .and_then(Value::as_str)
            .map(str::to_owned),
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
        description: val
            .get("description")
            .and_then(Value::as_str)
            .map(str::to_owned),
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
                CatalogItemType::Pack => {
                    snapshot.packs.push(entry);
                }
                CatalogItemType::ResourceTemplate => {
                    let manifest_path = local_dir.join(&entry.manifest_path);
                    if let Ok(content) = fs::read_to_string(&manifest_path)
                        && let Ok(val) = serde_json::from_str::<Value>(&content)
                        && let Some(definition) = val.get("definition").cloned()
                    {
                        seen_blocks.insert(entry.id.clone());
                        snapshot.block_templates.push(LocalBlockTemplate {
                            id: entry.id,
                            name: entry.name,
                            description: val
                                .get("description")
                                .and_then(Value::as_str)
                                .map(str::to_owned),
                            definition,
                            tags: entry.tags,
                            is_local: true,
                        });
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
                if let Ok(content) = fs::read_to_string(root.join(&entry.manifest_path))
                    && let Ok(val) = serde_json::from_str::<Value>(&content)
                    && let Some(definition) = val.get("definition").cloned()
                {
                    snapshot.flow_templates.push(LocalFlowTemplate {
                        id: entry.id,
                        name: entry.name,
                        description: val
                            .get("description")
                            .and_then(Value::as_str)
                            .map(str::to_owned),
                        file_mask: val
                            .get("fileMask")
                            .and_then(Value::as_str)
                            .unwrap_or("*")
                            .to_owned(),
                        version: entry.version,
                        definition,
                        is_local,
                    });
                }
            }
            CatalogItemType::ResourceTemplate if seen_blocks.insert(entry.id.clone()) => {
                if let Ok(content) = fs::read_to_string(root.join(&entry.manifest_path))
                    && let Ok(val) = serde_json::from_str::<Value>(&content)
                    && let Some(definition) = val.get("definition").cloned()
                {
                    snapshot.block_templates.push(LocalBlockTemplate {
                        id: entry.id,
                        name: entry.name,
                        description: val
                            .get("description")
                            .and_then(Value::as_str)
                            .map(str::to_owned),
                        definition,
                        tags: entry.tags,
                        is_local,
                    });
                }
            }
            _ => {}
        }
    }

    snapshot
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flows::OperationRegistry;
    use flux_shared::plugins::PackComponent;

    #[test]
    fn local_script_upload_templates_and_pack_roundtrip_without_github() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let local_dir = dir.path().join("local");

        let script = save_local_script(
            &local_dir,
            UpsertLocalScriptRequest {
                id: None,
                name: "Tag MKV".into(),
                description: Some("Ajoute un tag".into()),
                interpreter: "sh".into(),
                code: "cat >/dev/null\nprintf '{\"status\":\"ok\"}'".into(),
                filename: None,
                category: None,
                tags: vec!["mkv".into()],
            },
        )?;
        assert_eq!(script.id, "script.tag-mkv");

        let sub = upload_local_resource(
            &local_dir,
            UploadLocalResourceRequest {
                name: "Sous-titres FR".into(),
                entry_type: CatalogItemType::ResourceSubtitle,
                filename: "fr.srt".into(),
                content_base64: base64::encode(b"1\n00:00:01,000 --> 00:00:02,000\nSalut\n"),
                description: Some("Piste FR".into()),
            },
        )?;
        assert_eq!(sub.id, "resource.sous-titres-fr");

        let block_tpl = save_local_block_template(
            &local_dir,
            CreateBlockTemplateRequest {
                id: None,
                name: "Chaîne Remux".into(),
                description: None,
                definition: json!({"nodes": [], "connections": []}),
                tags: vec![],
            },
        )?;
        assert_eq!(block_tpl.id, "template.block.chaine-remux");

        let flow_tpl = save_local_flow_template(
            &local_dir,
            CreateFlowTemplateRequest {
                id: None,
                name: "Modèle HEVC".into(),
                description: Some("Pipeline type".into()),
                file_mask: Some("*.mkv".into()),
                definition: json!({"nodes": [], "connections": []}),
            },
        )?;
        assert_eq!(flow_tpl.id, "flow.template.modele-hevc");

        let pack = save_local_pack(
            &local_dir,
            CreateLocalPackRequest {
                id: None,
                name: "Mon Pack".into(),
                description: None,
                components: vec![PackComponent {
                    id: script.id.clone(),
                    version: "1.0.0".into(),
                    kind: CatalogItemType::Script,
                }],
                entrypoint: None,
            },
        )?;
        assert_eq!(pack.id, "pack.local.mon-pack");

        let roots = [local_dir.clone()];
        let reg = OperationRegistry::load_from_roots(&roots);
        assert!(reg.get("operation.script.tag-mkv").is_some());

        let snap = build_local_snapshot(&roots, &local_dir);
        assert_eq!(snap.scripts.len(), 1);
        assert_eq!(snap.resources.len(), 1);
        assert_eq!(snap.block_templates.len(), 1);
        assert_eq!(snap.flow_templates.len(), 1);
        assert_eq!(snap.packs.len(), 1);

        delete_local_item(&local_dir, &sub.id)?;
        let snap_after = build_local_snapshot(&roots, &local_dir);
        assert!(snap_after.resources.is_empty());
        Ok(())
    }
}
