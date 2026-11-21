//! Calcul de l'état opérationnel (`CatalogRuntimeSummary`) des entrées du catalogue
//! et de l'état des dépendances d'un plugin à partir du cache d'outils et des réglages enregistrés.

use super::{
    CatalogEntry, CatalogFlowEdge, CatalogFlowNode, CatalogFlowPreview, CatalogItemType,
    CatalogReadiness, CatalogRuntimeSummary, PluginDetailView, SettingsField, ToolStatus,
    load_manifest, read_pack_installation,
};
use forgearr_shared::flows::OwnedBlockSpec;
use serde_json::Value;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

const MAX_NODE_SUMMARY_ITEMS: usize = 3;

fn format_config_value(val: &Value) -> Option<String> {
    match val {
        Value::String(s) if !s.trim().is_empty() => Some(s.trim().to_owned()),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(if *b { "oui" } else { "non" }.to_owned()),
        Value::Array(items) if !items.is_empty() => {
            let parts: Vec<String> = items
                .iter()
                .filter_map(|item| item.as_str().map(str::to_owned))
                .take(MAX_NODE_SUMMARY_ITEMS)
                .collect();
            (!parts.is_empty()).then(|| parts.join(", "))
        }
        _ => None,
    }
}

/// Extrait l'aperçu structuré d'un manifeste JSON de flux (`definition.nodes` / `definition.connections`
/// ou format graphe `nodes` / `edges`).
pub fn parse_flow_preview(raw: &Value) -> Option<CatalogFlowPreview> {
    let def = raw.get("definition").unwrap_or(raw);
    let raw_nodes = def.get("nodes").and_then(Value::as_array)?;
    if raw_nodes.is_empty() {
        return None;
    }
    let nodes: Vec<CatalogFlowNode> = raw_nodes
        .iter()
        .filter_map(|node| {
            let id = node.get("id").and_then(Value::as_str)?.to_owned();
            let kind = node
                .get("type")
                .or_else(|| node.get("kind"))
                .and_then(Value::as_str)
                .unwrap_or("block")
                .to_owned();
            let name = node
                .get("name")
                .or_else(|| node.get("label"))
                .and_then(Value::as_str)
                .filter(|s| !s.trim().is_empty())
                .unwrap_or(&kind)
                .to_owned();
            let summary: Vec<String> = node
                .get("config")
                .and_then(Value::as_object)
                .map(|obj| {
                    obj.iter()
                        .filter_map(|(k, v)| {
                            format_config_value(v).map(|val| format!("{k}: {val}"))
                        })
                        .take(MAX_NODE_SUMMARY_ITEMS)
                        .collect()
                })
                .unwrap_or_default();
            Some(CatalogFlowNode {
                id,
                kind,
                name,
                summary,
            })
        })
        .collect();
    if nodes.is_empty() {
        return None;
    }
    let raw_edges = def
        .get("connections")
        .or_else(|| def.get("edges"))
        .and_then(Value::as_array);
    let connections: Vec<CatalogFlowEdge> = raw_edges
        .into_iter()
        .flatten()
        .filter_map(|edge| {
            let from = edge
                .get("from")
                .or_else(|| edge.get("source"))
                .and_then(Value::as_str)?
                .to_owned();
            let to = edge
                .get("to")
                .or_else(|| edge.get("target"))
                .and_then(Value::as_str)?
                .to_owned();
            let port = edge
                .get("port")
                .or_else(|| edge.get("sourcePort"))
                .and_then(Value::as_str)
                .filter(|s| !s.trim().is_empty())
                .map(str::to_owned);
            Some(CatalogFlowEdge { from, to, port })
        })
        .collect();
    let file_mask = raw
        .get("fileMask")
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty())
        .map(str::to_owned);
    Some(CatalogFlowPreview {
        file_mask,
        nodes,
        connections,
    })
}

fn load_flow_preview(root: &Path, manifest_path: &str) -> Option<CatalogFlowPreview> {
    let full_path = root.join(manifest_path);
    let text = std::fs::read_to_string(full_path).ok()?;
    let value: Value = serde_json::from_str(&text).ok()?;
    parse_flow_preview(&value)
}

/// Détermine si les réglages enregistrés satisfont les champs du schéma d'un plugin.
pub fn is_plugin_configured(fields: &[SettingsField], saved_json: Option<&str>) -> bool {
    if fields.is_empty() {
        return true;
    }
    let Some(raw) = saved_json else {
        return false;
    };
    let Ok(Value::Object(map)) = serde_json::from_str::<Value>(raw) else {
        return false;
    };
    let has_non_empty = |key: &str| {
        map.get(key)
            .and_then(|v| v.as_str())
            .is_some_and(|s| !s.trim().is_empty())
    };
    let required: Vec<&SettingsField> = fields.iter().filter(|f| f.required).collect();
    if required.is_empty() {
        fields.iter().any(|f| has_non_empty(&f.name))
    } else {
        required.iter().all(|f| has_non_empty(&f.name))
    }
}

/// Renseigne l'état réel de chaque dépendance dans une fiche détaillée de plugin.
pub fn attach_dependency_statuses(
    detail: &mut PluginDetailView,
    tool_statuses: &HashMap<String, ToolStatus>,
) {
    for dep in &mut detail.dependencies {
        if let Some(status) = tool_statuses.get(&dep.id).copied() {
            dep.status = Some(status);
        }
    }
}

/// Enrichit chaque entrée exposée du catalogue avec son `CatalogRuntimeSummary`.
pub fn compute_entry_runtime(
    roots: &[PathBuf],
    root: &Path,
    entry: &CatalogEntry,
    all_blocks: &[OwnedBlockSpec],
    tool_statuses: &HashMap<String, ToolStatus>,
    saved_settings: &HashMap<String, String>,
) -> CatalogRuntimeSummary {
    match entry.entry_type {
        CatalogItemType::Plugin => {
            let blocks_count = all_blocks
                .iter()
                .filter(|b| b.owner.as_deref() == Some(entry.id.as_str()))
                .count();
            let manifest = load_manifest(root, &entry.manifest_path).ok();
            let (fields, deps) = match manifest {
                Some(m) => (
                    m.settings_schema.map(|s| s.fields).unwrap_or_default(),
                    m.dependencies,
                ),
                None => (Vec::new(), Vec::new()),
            };
            let has_settings = !fields.is_empty();
            let configured = !has_settings
                || is_plugin_configured(&fields, saved_settings.get(&entry.id).map(String::as_str));
            let missing_tools_count = deps
                .iter()
                .filter(|dep| !dep.optional)
                .filter(|dep| {
                    tool_statuses
                        .get(&dep.id)
                        .is_some_and(|status| *status != ToolStatus::Ready)
                })
                .count();
            let readiness = if missing_tools_count > 0 {
                CatalogReadiness::NeedsTools
            } else if has_settings && !configured {
                CatalogReadiness::NeedsConfig
            } else {
                CatalogReadiness::Ready
            };
            CatalogRuntimeSummary {
                readiness,
                has_settings,
                configured,
                missing_tools_count,
                blocks_count,
                components_count: 0,
                flow_preview: None,
            }
        }
        CatalogItemType::Pack => {
            let installation = read_pack_installation(roots, &entry.id).ok();
            let components = installation.map(|i| i.components).unwrap_or_default();
            let components_count = components.len();
            let missing_tools_count = components
                .iter()
                .filter(|comp| comp.kind == CatalogItemType::App)
                .filter(|comp| {
                    tool_statuses
                        .get(&comp.id)
                        .is_some_and(|status| *status != ToolStatus::Ready)
                })
                .count();
            let readiness = if missing_tools_count > 0 {
                CatalogReadiness::NeedsTools
            } else {
                CatalogReadiness::Ready
            };
            CatalogRuntimeSummary {
                readiness,
                has_settings: false,
                configured: true,
                missing_tools_count,
                blocks_count: 0,
                components_count,
                flow_preview: None,
            }
        }
        CatalogItemType::Flow | CatalogItemType::Preset => {
            let flow_preview = load_flow_preview(root, &entry.manifest_path);
            let blocks_count = flow_preview.as_ref().map_or(0, |p| p.nodes.len());
            CatalogRuntimeSummary {
                readiness: CatalogReadiness::Available,
                has_settings: false,
                configured: true,
                missing_tools_count: 0,
                blocks_count,
                components_count: 0,
                flow_preview,
            }
        }
        _ => CatalogRuntimeSummary {
            readiness: CatalogReadiness::Available,
            has_settings: false,
            configured: true,
            missing_tools_count: 0,
            blocks_count: 0,
            components_count: 0,
            flow_preview: None,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugins::SettingsFieldType;
    use forgearr_shared::plugins::{PluginCompatibility, PluginDependency};

    fn field(name: &str, required: bool) -> SettingsField {
        SettingsField {
            name: name.to_owned(),
            label: name.to_owned(),
            field_type: SettingsFieldType::String,
            required,
            description: None,
            default_value: None,
        }
    }

    #[test]
    fn plugin_configured_checks_required_fields() {
        let fields = vec![field("url", true), field("apiKey", true)];
        assert!(!is_plugin_configured(&fields, None));
        assert!(!is_plugin_configured(
            &fields,
            Some(r#"{"url":"http://sonarr:8989"}"#)
        ));
        assert!(!is_plugin_configured(
            &fields,
            Some(r#"{"url":"http://sonarr:8989","apiKey":"   "}"#)
        ));
        assert!(is_plugin_configured(
            &fields,
            Some(r#"{"url":"http://sonarr:8989","apiKey":"enc:v1:abc"}"#)
        ));
    }

    #[test]
    fn plugin_without_required_fields_checks_any_non_empty_value() {
        let fields = vec![field("webhook", false)];
        assert!(!is_plugin_configured(&fields, None));
        assert!(!is_plugin_configured(&fields, Some(r#"{"webhook":""}"#)));
        assert!(is_plugin_configured(
            &fields,
            Some(r#"{"webhook":"https://hooks.example.test"}"#)
        ));
    }

    #[test]
    fn attach_dependency_statuses_matches_known_tools() {
        let mut detail = PluginDetailView {
            id: "video".into(),
            name: "Video".into(),
            version: "1.0.0".into(),
            description: None,
            author: None,
            license: None,
            capability: None,
            category: None,
            tags: Vec::new(),
            dependencies: vec![
                PluginDependency {
                    id: "ffmpeg".into(),
                    version_range: None,
                    kind: Some("app".into()),
                    optional: false,
                    status: None,
                },
                PluginDependency {
                    id: "unknown".into(),
                    version_range: None,
                    kind: None,
                    optional: true,
                    status: None,
                },
            ],
            compatibility: PluginCompatibility::default(),
            has_settings: false,
            blocks: Vec::new(),
            exposures: Vec::new(),
            updated_at: None,
        };
        let statuses = HashMap::from([("ffmpeg".into(), ToolStatus::Ready)]);
        attach_dependency_statuses(&mut detail, &statuses);
        assert_eq!(detail.dependencies[0].status, Some(ToolStatus::Ready));
        assert_eq!(detail.dependencies[1].status, None);
    }

    #[test]
    fn parse_flow_preview_extracts_nodes_edges_and_file_mask() {
        let raw = serde_json::json!({
            "fileMask": "*.mkv, *.mp4",
            "definition": {
                "nodes": [
                    {"id": "n1", "type": "file-input", "name": "Source"},
                    {"id": "n2", "type": "condition", "name": "HEVC ?", "config": {"target": "hevc"}},
                    {"id": "n3", "type": "file-output", "name": "Sortie"}
                ],
                "connections": [
                    {"from": "n1", "to": "n2"},
                    {"from": "n2", "to": "n3", "port": "true"}
                ]
            }
        });
        let Some(preview) = parse_flow_preview(&raw) else {
            panic!("expected flow preview");
        };
        assert_eq!(preview.file_mask.as_deref(), Some("*.mkv, *.mp4"));
        assert_eq!(preview.nodes.len(), 3);
        assert_eq!(preview.nodes[1].summary, vec!["target: hevc".to_owned()]);
        assert_eq!(preview.connections.len(), 2);
        assert_eq!(preview.connections[1].port.as_deref(), Some("true"));
    }
}
