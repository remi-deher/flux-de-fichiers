//! Calcul de l'état opérationnel (`CatalogRuntimeSummary`) des entrées du catalogue
//! et de l'état des dépendances d'un plugin à partir du cache d'outils et des réglages enregistrés.

use super::{
    CatalogEntry, CatalogItemType, CatalogReadiness, CatalogRuntimeSummary, PluginDetailView,
    SettingsField, ToolStatus, load_manifest, read_pack_installation,
};
use forgearr_shared::flows::OwnedBlockSpec;
use serde_json::Value;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

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
            }
        }
        _ => CatalogRuntimeSummary {
            readiness: CatalogReadiness::Available,
            has_settings: false,
            configured: true,
            missing_tools_count: 0,
            blocks_count: 0,
            components_count: 0,
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
            updated_at: None,
        };
        let statuses = HashMap::from([("ffmpeg".into(), ToolStatus::Ready)]);
        attach_dependency_statuses(&mut detail, &statuses);
        assert_eq!(detail.dependencies[0].status, Some(ToolStatus::Ready));
        assert_eq!(detail.dependencies[1].status, None);
    }
}
