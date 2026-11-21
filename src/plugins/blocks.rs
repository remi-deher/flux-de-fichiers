//! Blocs déclarés dans les dépôts de plugins → fiches pour la palette et l'inspecteur.
//!
//! Aucun bloc n'est embarqué par le moteur : sans dépôt synchronisé ni dossier local, le
//! catalogue retourné est vide.

use super::{CatalogItemType, catalog_with_roots};
use forgearr_shared::flows::{OwnedBlockSpec, OwnedFieldKind, OwnedFieldSpec};
use serde_json::Value;
use std::path::{Path, PathBuf};

/// Manifeste d'un bloc `block.v1.1` (identité, sockets/ports, champs UI et opération).
#[derive(Clone, Debug)]
struct PluginBlockDocument {
    id: String,
    aliases: Vec<String>,
    name: String,
    description: String,
    category: String,
    domain: Option<String>,
    has_input: bool,
    ports: Vec<(String, String)>,
    terminal: bool,
    suggested_next: Vec<String>,
    operation_id: Option<String>,
    fields: Vec<OwnedFieldSpec>,
    field_defaults: serde_json::Map<String, Value>,
}

/// Charge les blocs des dépôts synchronisés ; vide tant qu'aucun dépôt n'est disponible.
///
/// Les exemples (`*.example`) et macros non exécutables (`macro.*`) sont exclus.
pub fn flow_block_catalog(roots: &[PathBuf]) -> Vec<OwnedBlockSpec> {
    let mut catalog: Vec<OwnedBlockSpec> = Vec::new();
    for (root, entry) in catalog_with_roots(roots) {
        if entry.entry_type != CatalogItemType::Block {
            continue;
        }
        if entry.id.contains(".example") || entry.id.starts_with("macro.") {
            continue;
        }
        let Ok(document) = load_block_document(&root, &entry.manifest_path) else {
            continue;
        };
        let mut owned = document_to_spec(&document);
        owned.owner = entry.owner.clone();
        if let Some(existing) = catalog.iter_mut().find(|spec| spec.kind == owned.kind) {
            *existing = owned;
        } else {
            catalog.push(owned);
        }
    }
    catalog
}

fn string_list(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|item| item.as_str().map(str::to_owned))
        .collect()
}

fn parse_ui_ports(value: Option<&Value>) -> Vec<(String, String)> {
    value
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|port| {
            let id = port.get("id")?.as_str()?.to_owned();
            let label = port
                .get("label")
                .and_then(Value::as_str)
                .unwrap_or(id.as_str())
                .to_owned();
            Some((id, label))
        })
        .collect()
}

fn load_block_document(root: &Path, manifest_path: &str) -> anyhow::Result<PluginBlockDocument> {
    let content = std::fs::read_to_string(root.join(manifest_path))?;
    let value: Value = serde_json::from_str(&content)?;
    let id = value
        .get("id")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("id manquant"))?
        .to_owned();
    let aliases = string_list(value.get("aliases"));
    let name = value
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or(id.as_str())
        .to_owned();
    let description = value
        .get("description")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned();
    let category = value
        .pointer("/ui/category")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .or_else(|| {
            value
                .get("category")
                .and_then(Value::as_str)
                .map(category_label)
        })
        .unwrap_or_else(|| "Plugins".to_owned());
    let domain = value
        .pointer("/ui/domain")
        .and_then(Value::as_str)
        .map(str::to_owned);
    let has_input = value
        .pointer("/ui/hasInput")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let terminal = value
        .pointer("/ui/terminal")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let ports = parse_ui_ports(value.pointer("/ui/ports"));
    let suggested_next = string_list(value.pointer("/ui/suggestedNext"));
    let operation_id = value
        .get("operationId")
        .and_then(Value::as_str)
        .map(str::to_owned);
    let mut fields = Vec::new();
    let mut field_defaults = serde_json::Map::new();
    if let Some(raw_fields) = value.pointer("/ui/fields").and_then(Value::as_array) {
        for raw in raw_fields {
            let Some(field) = parse_ui_field(raw) else {
                continue;
            };
            if let Some(default) = raw.get("default") {
                field_defaults.insert(field.name.clone(), default.clone());
            }
            fields.push(field);
        }
    }
    Ok(PluginBlockDocument {
        id,
        aliases,
        name,
        description,
        category,
        domain,
        has_input,
        ports,
        terminal,
        suggested_next,
        operation_id,
        fields,
        field_defaults,
    })
}

fn category_label(raw: &str) -> String {
    match raw {
        "data" => "Données".into(),
        "io" => "Entrée/Sortie".into(),
        other => {
            let mut chars = other.chars();
            match chars.next() {
                Some(first) => format!("{}{}", first.to_uppercase(), chars.as_str()),
                None => "Plugins".into(),
            }
        }
    }
}

fn parse_ui_field(raw: &Value) -> Option<OwnedFieldSpec> {
    let name = raw.get("name")?.as_str()?.to_owned();
    let label = raw
        .get("label")
        .and_then(Value::as_str)
        .unwrap_or(name.as_str())
        .to_owned();
    let help = raw
        .get("description")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned();
    let required = raw
        .get("required")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let default = raw.get("default").cloned();
    let field_type = raw.get("type").and_then(Value::as_str).unwrap_or("string");
    let kind = match field_type {
        "boolean" => OwnedFieldKind::Bool,
        "headers" => OwnedFieldKind::Headers,
        "worker" => OwnedFieldKind::Worker,
        "condition" => OwnedFieldKind::Condition,
        "flow" => OwnedFieldKind::Flow,
        "select" => {
            let options = raw
                .get("options")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|option| {
                    let value = option.get("value")?.as_str()?.to_owned();
                    let label = option
                        .get("label")
                        .and_then(Value::as_str)
                        .unwrap_or(value.as_str())
                        .to_owned();
                    Some((value, label))
                })
                .collect::<Vec<_>>();
            if options.is_empty() {
                OwnedFieldKind::Text
            } else {
                OwnedFieldKind::Select(options)
            }
        }
        "range" => {
            let min = raw.get("min").and_then(Value::as_i64).unwrap_or(1);
            let max = raw.get("max").and_then(Value::as_i64).unwrap_or(min);
            let options = (min..=max)
                .map(|value| (value.to_string(), value.to_string()))
                .collect();
            OwnedFieldKind::Select(options)
        }
        _ => OwnedFieldKind::Text,
    };
    Some(OwnedFieldSpec {
        name,
        label,
        kind,
        required,
        help,
        default,
    })
}

fn document_to_spec(document: &PluginBlockDocument) -> OwnedBlockSpec {
    let mut spec = OwnedBlockSpec {
        kind: document.id.clone(),
        aliases: document.aliases.clone(),
        label: document.name.clone(),
        category: document.category.clone(),
        domain: document.domain.clone(),
        description: document.description.clone(),
        has_input: document.has_input,
        ports: document.ports.clone(),
        terminal: document.terminal,
        fields: document.fields.clone(),
        suggested_next: document.suggested_next.clone(),
        from_plugin: true,
        operation_id: document.operation_id.clone(),
        owner: None,
    };
    // Prefixe les défauts du manifeste pour les listes de choix.
    apply_declared_defaults(&mut spec, &document.field_defaults);
    spec
}

/// Place la valeur par défaut déclarée en tête de chaque select.
fn apply_declared_defaults(spec: &mut OwnedBlockSpec, defaults: &serde_json::Map<String, Value>) {
    for field in &mut spec.fields {
        let Some(default) = defaults.get(&field.name) else {
            continue;
        };
        if let OwnedFieldKind::Select(options) = &mut field.kind {
            let key = match default {
                Value::String(text) => text.clone(),
                Value::Number(number) => number.to_string(),
                Value::Bool(flag) => flag.to_string(),
                _ => continue,
            };
            if let Some(index) = options.iter().position(|(value, _)| value == &key) {
                let chosen = options.remove(index);
                options.insert(0, chosen);
            }
        }
    }
}

/// Config initiale d'un bloc plugin : respecte les défauts du manifeste.
pub fn plugin_default_config(spec: &OwnedBlockSpec, roots: &[PathBuf]) -> Value {
    let mut config = spec.default_config();
    for (root, entry) in catalog_with_roots(roots) {
        if !spec.matches_kind(&entry.id) {
            continue;
        }
        let Ok(document) = load_block_document(&root, &entry.manifest_path) else {
            continue;
        };
        if let Some(object) = config.as_object_mut() {
            for (name, value) in document.field_defaults {
                let normalized = match value {
                    Value::Number(number) => Value::String(number.to_string()),
                    other => other,
                };
                object.insert(name, normalized);
            }
        }
        break;
    }
    config
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn empty_roots_yields_empty_catalog() {
        assert!(flow_block_catalog(&[]).is_empty());
    }

    #[test]
    fn sibling_plugin_blocks_load_all_plugin_manifests() -> anyhow::Result<()> {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../flux-de-fichiers-plugins");
        let index = root.join("index.json");
        if !index.is_file() {
            return Ok(());
        }
        let catalog = flow_block_catalog(std::slice::from_ref(&root));
        let encode = catalog
            .iter()
            .find(|spec| spec.kind == "video.encode")
            .ok_or_else(|| anyhow::anyhow!("bloc d'encodage vidéo absent"))?;
        assert!(encode.from_plugin);
        assert_eq!(encode.owner.as_deref(), Some("video"));
        assert!(
            encode
                .fields
                .iter()
                .any(|field| field.name == "codec"
                    && matches!(field.kind, OwnedFieldKind::Select(_)))
        );
        for expected in [
            "essentiel.file-input",
            "essentiel.condition",
            "video.mkvpropedit",
            "audio.encode",
            "sonarr.rescan",
            "radarr.rename",
            "essentiel.webhook",
            "essentiel.file-output",
        ] {
            assert!(
                catalog
                    .iter()
                    .any(|spec| spec.kind == expected && spec.from_plugin),
                "bloc plugin manquant : {expected}"
            );
        }
        let input = catalog
            .iter()
            .find(|spec| spec.kind == "essentiel.file-input")
            .ok_or_else(|| anyhow::anyhow!("file-input absent"))?;
        assert!(!input.has_input);
        let condition = catalog
            .iter()
            .find(|spec| spec.kind == "essentiel.condition")
            .ok_or_else(|| anyhow::anyhow!("condition absente"))?;
        assert_eq!(condition.ports.len(), 2);
        let output = catalog
            .iter()
            .find(|spec| spec.kind == "essentiel.file-output")
            .ok_or_else(|| anyhow::anyhow!("file-output absent"))?;
        assert!(output.terminal);
        assert!(
            catalog
                .iter()
                .all(|spec| !spec.kind.contains(".example") && !spec.kind.starts_with("macro."))
        );
        Ok(())
    }

    #[test]
    fn range_and_headers_fields_are_parsed() {
        let range = serde_json::json!({
            "name": "quality",
            "label": "Qualité",
            "type": "range",
            "min": 1,
            "max": 3,
            "default": 2
        });
        let Some(field) = parse_ui_field(&range) else {
            panic!("un champ range doit être lu");
        };
        match field.kind {
            OwnedFieldKind::Select(options) => {
                assert_eq!(options.len(), 3);
                assert_eq!(options[0].0, "1");
            }
            other => panic!("attendu Select, reçu {other:?}"),
        }
        let headers = serde_json::json!({
            "name": "custom_headers",
            "label": "En-têtes",
            "type": "headers"
        });
        let Some(field) = parse_ui_field(&headers) else {
            panic!("un champ headers doit être lu");
        };
        assert_eq!(field.kind, OwnedFieldKind::Headers);
        let worker = serde_json::json!({"name": "workerId", "label": "Worker", "type": "worker"});
        let Some(field) = parse_ui_field(&worker) else {
            panic!("un champ worker doit être lu");
        };
        assert_eq!(field.kind, OwnedFieldKind::Worker);
        let condition = serde_json::json!({"name": "group", "label": "Règles", "type": "condition"});
        let Some(field) = parse_ui_field(&condition) else {
            panic!("un champ condition doit être lu");
        };
        assert_eq!(field.kind, OwnedFieldKind::Condition);
        let flow = serde_json::json!({"name": "flowId", "label": "Sous-flux", "type": "flow"});
        let Some(field) = parse_ui_field(&flow) else {
            panic!("un champ flux doit être lu");
        };
        assert_eq!(field.kind, OwnedFieldKind::Flow);
    }
}
