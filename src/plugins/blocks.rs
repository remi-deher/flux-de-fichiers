//! Blocs déclarés dans les dépôts de plugins → fiches pour la palette et l'inspecteur.

use super::{CatalogItemType, catalog_with_roots};
use flux_shared::flows::{
    FlowBlockKind, OwnedBlockSpec, OwnedFieldKind, OwnedFieldSpec, native_owned_catalog,
};
use serde_json::Value;
use std::path::{Path, PathBuf};

/// Manifeste minimal d'un bloc `block.v1.1` (champs UI + opération).
#[derive(Clone, Debug)]
struct PluginBlockDocument {
    id: String,
    name: String,
    description: String,
    category: String,
    operation_id: Option<String>,
    fields: Vec<OwnedFieldSpec>,
    field_defaults: serde_json::Map<String, Value>,
}

/// Charge les blocs des dépôts synchronisés et les fusionne avec le catalogue natif.
///
/// Les fiches plugins (`type: block`) priment sur les natives de même `kind` pour le libellé,
/// la description et les champs. Les exemples (`*.example`) et macros non exécutables sont
/// exclus. Les blocs natifs sans équivalent plugin restent disponibles.
pub fn flow_block_catalog(roots: &[PathBuf]) -> Vec<OwnedBlockSpec> {
    let mut catalog = native_owned_catalog();
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
        let owned = document_to_spec(&document);
        if let Some(existing) = catalog.iter_mut().find(|spec| spec.kind == owned.kind) {
            *existing = owned;
        } else {
            catalog.push(owned);
        }
    }
    catalog
}

fn load_block_document(root: &Path, manifest_path: &str) -> anyhow::Result<PluginBlockDocument> {
    let content = std::fs::read_to_string(root.join(manifest_path))?;
    let value: Value = serde_json::from_str(&content)?;
    let id = value
        .get("id")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("id manquant"))?
        .to_owned();
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
        .or_else(|| value.get("category").and_then(Value::as_str))
        .map(category_label)
        .unwrap_or_else(|| "Plugins".to_owned());
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
        name,
        description,
        category,
        operation_id,
        fields,
        field_defaults,
    })
}

fn category_label(raw: &str) -> String {
    match raw {
        "media" => "Média".into(),
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
    let field_type = raw.get("type").and_then(Value::as_str).unwrap_or("string");
    let kind = match field_type {
        "boolean" => OwnedFieldKind::Bool,
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
    })
}

fn document_to_spec(document: &PluginBlockDocument) -> OwnedBlockSpec {
    let kind = FlowBlockKind::parse(&document.id);
    let kind_str = if matches!(kind, FlowBlockKind::Other(_)) {
        document.id.clone()
    } else {
        kind.as_str().to_owned()
    };
    let mut spec = OwnedBlockSpec {
        kind: kind_str,
        label: document.name.clone(),
        category: document.category.clone(),
        description: document.description.clone(),
        has_input: true,
        ports: Vec::new(),
        terminal: false,
        fields: document.fields.clone(),
        from_plugin: true,
        operation_id: document.operation_id.clone(),
    };
    // Prefixe les défauts du manifeste pour default_config via réordonnancement des selects.
    apply_declared_defaults(&mut spec, &document.field_defaults);
    spec
}

/// Place la valeur par défaut déclarée en tête de chaque select, pour `default_config`.
fn apply_declared_defaults(spec: &mut OwnedBlockSpec, defaults: &serde_json::Map<String, Value>) {
    for field in &mut spec.fields {
        let Some(default) = defaults.get(&field.name) else {
            continue;
        };
        match &mut field.kind {
            OwnedFieldKind::Select(options) => {
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
            OwnedFieldKind::Bool => {}
            OwnedFieldKind::Text | OwnedFieldKind::Headers => {}
        }
    }
}

/// Config initiale d'un bloc plugin : respecte les défauts du manifeste.
pub fn plugin_default_config(spec: &OwnedBlockSpec, roots: &[PathBuf]) -> Value {
    let mut config = spec.default_config();
    for (root, entry) in catalog_with_roots(roots) {
        if entry.id != spec.kind && FlowBlockKind::parse(&entry.id).as_str() != spec.kind {
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
    fn sibling_plugin_blocks_enrich_the_palette() -> anyhow::Result<()> {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../flux-de-fichiers-plugins");
        let index = root.join("catalogue/index.json");
        if !index.is_file() {
            return Ok(());
        }
        let catalog = flow_block_catalog(std::slice::from_ref(&root));
        let encode = catalog
            .iter()
            .find(|spec| spec.kind == "block.video.encode")
            .ok_or_else(|| anyhow::anyhow!("bloc d'encodage vidéo absent"))?;
        assert!(encode.from_plugin);
        assert!(
            encode
                .fields
                .iter()
                .any(|field| field.name == "codec"
                    && matches!(field.kind, OwnedFieldKind::Select(_)))
        );
        assert!(
            catalog
                .iter()
                .any(|spec| spec.kind == "block.audio.encode" && spec.from_plugin)
        );
        assert!(
            catalog
                .iter()
                .all(|spec| !spec.kind.contains(".example") && !spec.kind.starts_with("macro."))
        );
        Ok(())
    }

    #[test]
    fn range_fields_become_selects() {
        let raw = serde_json::json!({
            "name": "quality",
            "label": "Qualité",
            "type": "range",
            "min": 1,
            "max": 3,
            "default": 2
        });
        let Some(field) = parse_ui_field(&raw) else {
            panic!("un champ range doit être lu");
        };
        match field.kind {
            OwnedFieldKind::Select(options) => {
                assert_eq!(options.len(), 3);
                assert_eq!(options[0].0, "1");
            }
            other => panic!("attendu Select, reçu {other:?}"),
        }
    }
}
