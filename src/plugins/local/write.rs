//! Opérations d'écriture et de suppression dans le catalogue local.

use super::{
    BLOCK_TEMPLATE_TAG, MAX_RESOURCE_BYTES, declared_file, ensure_new_target_absent,
    is_safe_filename, local_item_dir, normalize_description, rebuild_local_index, resolve_slug,
    write_json_pretty,
};
use crate::{
    error::{AppError, AppResult},
    plugins::{CatalogEntry, CatalogItemType, is_valid_repository_id, vocab::CatalogSection},
};
use forgearr_shared::{
    base64,
    flows::FlowGraph,
    local::{
        CreateBlockTemplateRequest, CreateFlowTemplateRequest, CreateLocalPackRequest,
        LocalBlockTemplate, LocalFlowTemplate, LocalResourceSummary, UploadLocalResourceRequest,
    },
    plugins::PackComponent,
};
use serde_json::{Map, Value, json};
use std::{collections::HashSet, fs, path::Path};

fn remove_stale_file(base_dir: &Path, old_rel: Option<&str>, new_rel: &str) {
    let Some(old) = old_rel else {
        return;
    };
    if old != new_rel && old != "manifest.json" && is_safe_filename(old) {
        let _ = fs::remove_file(base_dir.join(old));
    }
}

fn read_previous_resource_asset(base_dir: &Path) -> Option<String> {
    let raw = fs::read_to_string(base_dir.join("manifest.json")).ok()?;
    let val: Value = serde_json::from_str(&raw).ok()?;
    declared_file(&val, "asset").map(str::to_owned)
}

fn validate_flow_graph(definition: &Value) -> AppResult<()> {
    let parsed: FlowGraph = serde_json::from_value(definition.clone()).map_err(|err| {
        AppError::InvalidManifest(format!("définition de graphe invalide : {err}"))
    })?;
    if !parsed.connections.is_empty() {
        let node_ids: HashSet<&str> = parsed.nodes.iter().map(|n| n.id.as_str()).collect();
        for conn in &parsed.connections {
            if !node_ids.contains(conn.from.as_str()) || !node_ids.contains(conn.to.as_str()) {
                return Err(AppError::InvalidManifest(
                    "connexion vers un nœud inexistant dans la définition".into(),
                ));
            }
        }
    }
    Ok(())
}

fn validate_pack_component(component: &PackComponent) -> AppResult<()> {
    if !is_valid_repository_id(&component.id) || component.version.trim().is_empty() {
        return Err(AppError::InvalidManifest(
            "composant de pack invalide".into(),
        ));
    }
    if !CatalogItemType::Pack.may_depend_on(component.kind) {
        return Err(AppError::InvalidManifest(format!(
            "un pack ne peut pas embarquer un composant de type {}",
            component.kind.as_str()
        )));
    }
    Ok(())
}

/// Importe ou remplace un fichier ressource local (image, sous-titre, son, police, modèle…).
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

    let (slug, is_update) = resolve_slug(req.id.as_deref(), "resource.", name)?;
    let id = format!("resource.{slug}");
    let base_dir = local_dir.join("resources").join(&slug);
    ensure_new_target_absent(&base_dir, is_update)?;
    let previous_asset = read_previous_resource_asset(&base_dir);

    fs::create_dir_all(&base_dir)?;
    fs::write(base_dir.join(filename), &bytes)?;

    let description = normalize_description(req.description.as_deref());
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
    remove_stale_file(&base_dir, previous_asset.as_deref(), filename);
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
    validate_flow_graph(&req.definition)?;
    let (slug, is_update) = resolve_slug(req.id.as_deref(), "template.block.", name)?;
    let id = format!("template.block.{slug}");
    let base_dir = local_dir.join("templates/blocks").join(&slug);
    ensure_new_target_absent(&base_dir, is_update)?;
    fs::create_dir_all(&base_dir)?;

    let description = normalize_description(req.description.as_deref());
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
    validate_flow_graph(&req.definition)?;
    let definition =
        forgearr_shared::flows::legacy_references::upgrade_legacy_definition_value(&req.definition)
            .unwrap_or_else(|| req.definition.clone());
    let (slug, is_update) = resolve_slug(req.id.as_deref(), "flow.template.", name)?;
    let id = format!("flow.template.{slug}");
    let base_dir = local_dir.join("flows").join(&slug);
    ensure_new_target_absent(&base_dir, is_update)?;
    fs::create_dir_all(&base_dir)?;

    let description = normalize_description(req.description.as_deref());
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
        "definition": definition,
    });
    write_json_pretty(&base_dir.join("manifest.json"), &manifest_json)?;
    rebuild_local_index(local_dir)?;

    Ok(LocalFlowTemplate {
        id,
        name: name.to_owned(),
        description,
        file_mask,
        version: "1.0.0".to_owned(),
        definition,
        is_local: true,
    })
}

/// Crée ou met à jour un pack local regroupant des scripts, ressources, flows ou plugins.
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
    for component in &req.components {
        validate_pack_component(component)?;
    }
    if let Some(ref entrypoint) = req.entrypoint {
        validate_pack_component(entrypoint)?;
    }

    let (slug, is_update) = resolve_slug(req.id.as_deref(), "pack.local.", name)?;
    let id = format!("pack.local.{slug}");
    let base_dir = local_dir.join("packs").join(&slug);
    ensure_new_target_absent(&base_dir, is_update)?;
    fs::create_dir_all(&base_dir)?;

    let description = normalize_description(req.description.as_deref())
        .unwrap_or_else(|| "Pack local assemblé depuis la librairie.".to_owned());

    let mut installation = Map::new();
    installation.insert("schemaVersion".into(), json!("1.0.0"));
    installation.insert("packId".into(), json!(id));
    installation.insert("packVersion".into(), json!("1.0.0"));
    installation.insert("kind".into(), json!("declarative-bundle"));
    installation.insert("components".into(), json!(req.components));
    if let Some(entrypoint) = req.entrypoint {
        installation.insert("entrypoint".into(), json!(entrypoint));
    }
    installation.insert("runtimeImplemented".into(), json!(true));
    write_json_pretty(
        &base_dir.join("installation.json"),
        &Value::Object(installation),
    )?;

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
        description: Some(description),
        capability: None,
        current_choice: None,
        distribution: None,
        category: None,
        subcategory: None,
        has_icon: false,
        tags: Vec::new(),
        owner: None,
        sha256: None,
        updated_at: None,
        runtime: None,
    })
}

/// Supprime un élément du catalogue local (par son `id`) et reconstruit `index.json`. L'appelant a contrôlé que
/// rien ne s'en sert.
pub(in crate::plugins) fn delete_local_item(local_dir: &Path, id: &str) -> AppResult<()> {
    fs::remove_dir_all(local_item_dir(local_dir, id)?)?;
    rebuild_local_index(local_dir)
}
