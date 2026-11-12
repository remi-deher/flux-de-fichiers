//! Gestionnaire de plugins, chargement du catalogue et inspection des manifestes.

use crate::tools_cache::{ToolCacheManager, host_arch};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::Path;

mod probe;
mod settings;
mod vocab;

pub use probe::{ProbeContract, file_job_may_start, probe_is_ready, required_probe};

pub use settings::{
    ResolvedSettingsField, SettingsField, SettingsFieldType, SettingsSchema, resolve_setting_value,
    resolve_settings_fields,
};
pub use vocab::{CatalogCapability, CatalogItemType, PluginCategory};

/// Dépôt distant de distribution de plugins, outils et flux.
#[derive(Clone, Debug, Serialize, Deserialize, sqlx::FromRow, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PluginRepository {
    pub id: String,
    pub name: String,
    pub index_url: String,
    pub is_official: bool,
    pub enabled: bool,
    pub last_synced_at: Option<String>,
    pub created_at: String,
}

/// Requête d'enregistrement d'un nouveau dépôt de plugins.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateRepositoryRequest {
    pub id: Option<String>,
    pub name: String,
    pub index_url: String,
}

/// Fiche d'index dans `catalogue/index.json`.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CatalogEntry {
    pub id: String,
    #[serde(rename = "type")]
    pub entry_type: CatalogItemType,
    pub version: String,
    pub name: String,
    pub manifest_path: String,
    #[serde(default)]
    pub capability: Option<CatalogCapability>,
    #[serde(default)]
    pub current_choice: Option<bool>,
    #[serde(default)]
    pub distribution: Option<String>,
    #[serde(
        default,
        deserialize_with = "vocab::deserialize_optional_category",
        skip_serializing_if = "vocab::hides_category_badge"
    )]
    pub category: Option<PluginCategory>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
}

/// Conteneur de l'index du catalogue.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CatalogIndex {
    pub schema_version: String,
    #[serde(default)]
    pub generated_for: Option<String>,
    pub entries: Vec<CatalogEntry>,
}

/// Manifeste individuel d'un composant (`manifest.json`).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PluginManifest {
    pub schema_version: String,
    pub id: String,
    #[serde(rename = "type")]
    pub entry_type: CatalogItemType,
    pub version: String,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub author: Option<String>,
    #[serde(default)]
    pub license: Option<String>,
    #[serde(default)]
    pub tool: Option<ManifestToolDetails>,
    #[serde(default)]
    pub settings_schema: Option<SettingsSchema>,
    #[serde(
        default,
        deserialize_with = "vocab::deserialize_optional_category",
        skip_serializing_if = "vocab::hides_category_badge"
    )]
    pub category: Option<PluginCategory>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ManifestToolDetails {
    #[serde(default)]
    pub capability: Option<CatalogCapability>,
    #[serde(default)]
    pub interface: Option<String>,
    #[serde(default)]
    pub invocation: Option<String>,
    #[serde(default)]
    pub current_choice: Option<bool>,
}

/// Résumé de l'état d'un outil pour l'API et l'interface utilisateur.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ToolStatusSummary {
    pub id: String,
    pub name: String,
    pub version: String,
    pub status: String,
    pub capability: CatalogCapability,
    pub current_choice: bool,
    pub arch: String,
    pub distribution: String,
    pub description: String,
}

/// Charge l'index du catalogue depuis le répertoire racine des plugins.
pub fn load_catalog(plugins_root: &Path) -> anyhow::Result<CatalogIndex> {
    let index_path = plugins_root.join("catalogue").join("index.json");
    let content = std::fs::read_to_string(&index_path)?;
    let index: CatalogIndex = serde_json::from_str(&content)?;
    Ok(index)
}

/// Charge un manifeste spécifique depuis le répertoire racine des plugins.
pub fn load_manifest(plugins_root: &Path, manifest_path: &str) -> anyhow::Result<PluginManifest> {
    let full_path = plugins_root.join(manifest_path);
    let content = std::fs::read_to_string(&full_path)?;
    let manifest: PluginManifest = serde_json::from_str(&content)?;
    Ok(manifest)
}

/// Liste l'ensemble des entrées du catalogue avec repli par défaut si non trouvé sur disque.
///
/// La catégorie et les tags viennent du document pointé par l'index. Une catégorie
/// inconnue est déjà normalisée en `other` et n'est pas sérialisée (pas de pastille).
pub fn list_catalog(plugins_root: Option<&Path>) -> Vec<CatalogEntry> {
    if let Some(root) = plugins_root
        && let Ok(catalog) = load_catalog(root)
    {
        return catalog
            .entries
            .into_iter()
            .map(|mut entry| {
                apply_document_labels(root, &mut entry);
                entry
            })
            .collect();
    }
    default_catalog_entries()
}

fn apply_document_labels(root: &Path, entry: &mut CatalogEntry) {
    let Ok(content) = std::fs::read_to_string(root.join(&entry.manifest_path)) else {
        return;
    };
    let Ok(value) = serde_json::from_str::<Value>(&content) else {
        return;
    };
    let labels = vocab::labels_from_document(&value);
    if labels.category_declared {
        entry.category = labels.category;
    }
    if labels.tags_declared {
        entry.tags = labels.tags;
    }
}

/// Champs du formulaire : valeur sauvée, sinon défaut du manifeste, sinon vide.
///
/// Ne lit la base pour rien d'autre et n'écrit aucun défaut.
pub fn resolved_plugin_fields(
    plugins_root: Option<&Path>,
    plugin_id: &str,
    saved: &Value,
) -> Vec<ResolvedSettingsField> {
    let Some(root) = plugins_root else {
        return Vec::new();
    };
    let Ok(catalog) = load_catalog(root) else {
        return Vec::new();
    };
    let Some(entry) = catalog.entries.iter().find(|entry| entry.id == plugin_id) else {
        return Vec::new();
    };
    let Ok(manifest) = load_manifest(root, &entry.manifest_path) else {
        return Vec::new();
    };
    let Some(schema) = manifest.settings_schema else {
        return Vec::new();
    };
    resolve_settings_fields(saved, &schema.fields)
}

/// Calcule l'état dynamique de chaque outil déclaré par rapport au cache local du nœud.
pub fn list_tools_status(
    plugins_root: Option<&Path>,
    cache_mgr: &ToolCacheManager,
) -> Vec<ToolStatusSummary> {
    let arch = host_arch();
    let entries = list_catalog(plugins_root);

    entries
        .into_iter()
        .filter(|entry| entry.entry_type == CatalogItemType::App)
        .map(|entry| {
            let mut description = String::new();
            if let Some(root) = plugins_root
                && let Ok(manifest) = load_manifest(root, &entry.manifest_path)
                && let Some(text) = manifest.description
            {
                description = text;
            }
            if description.is_empty() {
                description = default_tool_description(&entry.id);
            }

            let is_system_ready = match entry.id.as_str() {
                "app.internal.telemetry" => true,
                "app.mkvtoolnix" => Path::new("/usr/bin/mkvmerge").exists(),
                _ => false,
            };

            let status = if is_system_ready || cache_mgr.is_ready(&entry.id, &entry.version, arch) {
                "ready".to_string()
            } else {
                "not_installed".to_string()
            };

            ToolStatusSummary {
                id: entry.id,
                name: entry.name,
                version: entry.version,
                status,
                capability: entry.capability.unwrap_or(CatalogCapability::Other),
                current_choice: entry.current_choice.unwrap_or(false),
                arch: arch.to_string(),
                distribution: entry
                    .distribution
                    .unwrap_or_else(|| "autonomous-binary".into()),
                description,
            }
        })
        .collect()
}

fn default_tool_description(tool_id: &str) -> String {
    match tool_id {
        "app.mkvtoolnix" => "Outil de remuxage sans réencodage validé. Distribution Option B (binaire autonome hors conteneur applicatif). Matroska uniquement.".into(),
        "app.mp4box" => "Outil de référence pour le multiplexage, l'optimisation streaming faststart et la manipulation de conteneurs MP4 sans réencodage (GPAC).".into(),
        "app.mediainfo" => "Outil universel d'analyse et d'extraction de métadonnées techniques multimédias (codecs, résolutions, profils HDR10/Dolby Vision, pistes audio et sous-titres).".into(),
        "app.ffmpeg" => "Outil d'analyse multimédia et génération de médias synthétiques de tests. Réservé aux tests et scénarios de simulation.".into(),
        "app.internal.telemetry" => "Mesure périodique CPU, RAM, GPU (/dev/dri) et stockage local. Confinement strict sous UID 10001 (app) sans socket Docker.".into(),
        "app.postgresql" => "Moteur de base de données relationnelle historique (remplacé par SQLite local /config dans l'ADR 0005).".into(),
        _ => "Outil modulaire distribué.".into(),
    }
}

struct OfflineEntry {
    id: &'static str,
    entry_type: CatalogItemType,
    version: &'static str,
    name: &'static str,
    manifest_path: &'static str,
    capability: Option<CatalogCapability>,
    current_choice: Option<bool>,
    distribution: Option<&'static str>,
}

fn offline_entry(spec: OfflineEntry) -> CatalogEntry {
    CatalogEntry {
        id: spec.id.to_owned(),
        entry_type: spec.entry_type,
        version: spec.version.to_owned(),
        name: spec.name.to_owned(),
        manifest_path: spec.manifest_path.to_owned(),
        capability: spec.capability,
        current_choice: spec.current_choice,
        distribution: spec.distribution.map(str::to_owned),
        category: None,
        tags: Vec::new(),
    }
}

fn default_catalog_entries() -> Vec<CatalogEntry> {
    vec![
        offline_entry(OfflineEntry {
            id: "app.mkvtoolnix",
            entry_type: CatalogItemType::App,
            version: "1.0.0",
            name: "MKVToolNix (mkvmerge)",
            manifest_path: "tools/mkvtoolnix/manifest.json",
            capability: Some(CatalogCapability::Remux),
            current_choice: Some(true),
            distribution: Some("autonomous-binary"),
        }),
        offline_entry(OfflineEntry {
            id: "app.ffmpeg",
            entry_type: CatalogItemType::App,
            version: "1.0.0",
            name: "FFmpeg (distribué)",
            manifest_path: "tools/ffmpeg/manifest.json",
            capability: Some(CatalogCapability::Analyze),
            current_choice: Some(false),
            distribution: Some("autonomous-binary"),
        }),
        offline_entry(OfflineEntry {
            id: "app.mediainfo",
            entry_type: CatalogItemType::App,
            version: "1.0.0",
            name: "MediaInfo CLI",
            manifest_path: "tools/mediainfo/manifest.json",
            capability: Some(CatalogCapability::Probe),
            current_choice: Some(true),
            distribution: Some("autonomous-binary"),
        }),
        offline_entry(OfflineEntry {
            id: "app.mp4box",
            entry_type: CatalogItemType::App,
            version: "2.4.0",
            name: "MP4Box (GPAC)",
            manifest_path: "tools/mp4box/manifest.json",
            capability: Some(CatalogCapability::Remux),
            current_choice: Some(false),
            distribution: Some("autonomous-binary"),
        }),
        offline_entry(OfflineEntry {
            id: "app.internal.telemetry",
            entry_type: CatalogItemType::App,
            version: "0.1.0",
            name: "Sonde interne de télémétrie",
            manifest_path: "tools/internal-telemetry/manifest.json",
            capability: Some(CatalogCapability::Other),
            current_choice: Some(true),
            distribution: Some("autonomous-binary"),
        }),
        offline_entry(OfflineEntry {
            id: "plugin.sonarr",
            entry_type: CatalogItemType::Plugin,
            version: "1.0.0",
            name: "Sonarr Integration",
            manifest_path: "plugins/sonarr/manifest.json",
            capability: Some(CatalogCapability::Integration),
            current_choice: Some(true),
            distribution: None,
        }),
        offline_entry(OfflineEntry {
            id: "plugin.radarr",
            entry_type: CatalogItemType::Plugin,
            version: "1.0.0",
            name: "Radarr Integration",
            manifest_path: "plugins/radarr/manifest.json",
            capability: Some(CatalogCapability::Integration),
            current_choice: Some(true),
            distribution: None,
        }),
        offline_entry(OfflineEntry {
            id: "macro.remux-verify.example",
            entry_type: CatalogItemType::Block,
            version: "0.1.0",
            name: "Macro remux + vérifier (exemple)",
            manifest_path: "blocks/macro-remux-verify.example/manifest.json",
            capability: None,
            current_choice: None,
            distribution: None,
        }),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::path::Path;

    #[test]
    fn unknown_catalog_strings_are_rejected() {
        let bad_type = r#"{"schemaVersion":"1.0.0","entries":[{"id":"x","type":"extension","version":"1.0.0","name":"X","manifestPath":"x.json"}]}"#;
        assert!(serde_json::from_str::<CatalogIndex>(bad_type).is_err());
        let bad_capability = r#"{"schemaVersion":"1.0.0","entries":[{"id":"x","type":"app","version":"1.0.0","name":"X","manifestPath":"x.json","capability":"studio"}]}"#;
        assert!(serde_json::from_str::<CatalogIndex>(bad_capability).is_err());
    }

    #[test]
    fn other_category_is_kept_in_memory_and_omitted_from_json() -> Result<(), serde_json::Error> {
        let entry = offline_entry(OfflineEntry {
            id: "plugin.demo",
            entry_type: CatalogItemType::Plugin,
            version: "1.0.0",
            name: "Démo",
            manifest_path: "plugins/demo.json",
            capability: None,
            current_choice: None,
            distribution: None,
        });
        let mut hidden = entry.clone();
        hidden.category = Some(PluginCategory::Other);
        hidden.tags = vec!["arr".into(), "tv".into()];
        let json = serde_json::to_value(&hidden)?;
        assert!(json.get("category").is_none());
        assert_eq!(json["tags"], serde_json::json!(["arr", "tv"]));
        assert_eq!(json["type"], "plugin");

        let mut visible = entry;
        visible.category = Some(PluginCategory::Media);
        let json = serde_json::to_value(&visible)?;
        assert_eq!(json["category"], "media");
        assert!(json.get("tags").is_none());
        Ok(())
    }

    #[test]
    fn listed_catalog_normalizes_category_and_proposes_defaults_without_writing_them()
    -> anyhow::Result<()> {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/plugin-catalog-test");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("catalogue"))?;
        std::fs::create_dir_all(root.join("plugins"))?;
        std::fs::write(
            root.join("catalogue/index.json"),
            r#"{
                "schemaVersion": "1.0.0",
                "entries": [{
                    "id": "plugin.demo",
                    "type": "plugin",
                    "version": "1.0.0",
                    "name": "Démo",
                    "manifestPath": "plugins/demo.json",
                    "capability": "integration"
                }]
            }"#,
        )?;
        std::fs::write(
            root.join("plugins/demo.json"),
            r#"{
                "schemaVersion": "1.1.0",
                "id": "plugin.demo",
                "type": "plugin",
                "version": "1.0.0",
                "name": "Démo",
                "category": "studio",
                "tags": ["arr", "tv"],
                "settingsSchema": {
                    "fields": [
                        {"name": "url", "label": "URL", "type": "url", "default": "http://example.test", "required": true},
                        {"name": "apiKey", "label": "Clé", "type": "secret", "required": true}
                    ]
                }
            }"#,
        )?;

        let entries = list_catalog(Some(&root));
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].category, Some(PluginCategory::Other));
        assert!(!PluginCategory::Other.shows_badge());
        assert_eq!(entries[0].tags, vec!["arr".to_owned(), "tv".to_owned()]);
        let listed = serde_json::to_value(&entries[0])?;
        assert!(listed.get("category").is_none());

        let saved = json!({"url": ""});
        let fields = resolved_plugin_fields(Some(&root), "plugin.demo", &saved);
        assert_eq!(fields[0].name, "url");
        assert_eq!(fields[0].value, "");
        assert_eq!(fields[1].name, "apiKey");
        assert_eq!(fields[1].value, "");

        let fresh = json!({});
        let proposed = resolved_plugin_fields(Some(&root), "plugin.demo", &fresh);
        assert_eq!(proposed[0].value, "http://example.test");
        assert_eq!(fresh, json!({}));

        std::fs::remove_dir_all(&root)?;
        Ok(())
    }

    #[test]
    fn sibling_plugin_repository_loads_with_these_enums() -> anyhow::Result<()> {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../flux-de-fichiers-plugins");
        let index = root.join("catalogue/index.json");
        if !index.is_file() {
            return Ok(());
        }
        let catalog = load_catalog(&root)?;
        assert!(!catalog.entries.is_empty());
        for entry in &catalog.entries {
            let content = std::fs::read_to_string(root.join(&entry.manifest_path))?;
            let value: Value = serde_json::from_str(&content)?;
            if value.get("type").is_some() {
                let manifest = load_manifest(&root, &entry.manifest_path)?;
                assert_eq!(manifest.id, entry.id);
                assert_eq!(manifest.entry_type, entry.entry_type);
            }
            if let Some(raw) = value.get("category").and_then(Value::as_str) {
                let category = PluginCategory::from_label(raw);
                if matches!(raw, "media" | "data" | "io") {
                    assert!(category.shows_badge());
                } else {
                    assert!(!category.shows_badge());
                }
            }
        }

        for plugin_id in ["plugin.sonarr", "plugin.radarr"] {
            let entry = catalog
                .entries
                .iter()
                .find(|entry| entry.id == plugin_id)
                .ok_or_else(|| anyhow::anyhow!("fiche absente: {plugin_id}"))?;
            let manifest = load_manifest(&root, &entry.manifest_path)?;
            let schema = manifest
                .settings_schema
                .ok_or_else(|| anyhow::anyhow!("settingsSchema absent pour {plugin_id}"))?;
            let saved = json!({"url": ""});
            let resolved = resolve_settings_fields(&saved, &schema.fields);
            let url = resolved
                .iter()
                .find(|field| field.name == "url")
                .ok_or_else(|| anyhow::anyhow!("champ url absent"))?;
            assert_eq!(url.value, "");
            let fresh = json!({});
            let proposed = resolve_settings_fields(&fresh, &schema.fields);
            let proposed_url = proposed
                .iter()
                .find(|field| field.name == "url")
                .ok_or_else(|| anyhow::anyhow!("champ url absent"))?;
            let declared = schema
                .fields
                .iter()
                .find(|field| field.name == "url")
                .and_then(|field| field.default_value.clone())
                .unwrap_or_default();
            assert_eq!(proposed_url.value, declared);
            assert!(fresh.get("url").is_none());
            assert!(saved.get("pathMappingRemote").is_none());
        }
        Ok(())
    }
}
