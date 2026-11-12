//! Gestionnaire de plugins, chargement du catalogue et inspection des manifestes.

use crate::tools_cache::{ToolCacheManager, host_arch};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};

mod install;
mod probe;
mod secret_settings;
mod settings;
mod sources;
mod sync;
mod vocab;

pub use install::{
    ensure_tool_installed, install_from_distribution, is_installing, parse_distribution,
    read_distribution,
};
pub use probe::{ProbeContract, file_job_may_start, probe_is_ready, required_probe};
pub use sources::{
    catalog_roots, is_valid_repository_id, repository_dir, sync_enabled_repositories,
};
pub use sync::{SyncError, SyncPolicy, SyncReport, sync_repository, validate_index_url};

pub use secret_settings::{mask, migrate_plaintext, seal, secret_names};
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

/// État d'un outil dans le cache de ce nœud.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolStatus {
    Ready,
    /// Téléchargement ou extraction en cours.
    Installing,
    NotInstalled,
}

/// Résumé de l'état d'un outil pour l'API et l'interface utilisateur.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ToolStatusSummary {
    pub id: String,
    pub name: String,
    pub version: String,
    pub status: ToolStatus,
    pub capability: CatalogCapability,
    pub current_choice: bool,
    pub arch: String,
    pub distribution: String,
    pub description: String,
}

/// Index lu entrée par entrée : une fiche inconnue est ignorée sans invalider les autres.
pub struct ParsedIndex {
    pub index: CatalogIndex,
    /// Fiches ignorées, avec la raison.
    pub skipped: Vec<String>,
}

/// Lit un `catalogue/index.json` en tolérant les fiches que ce coordinateur ne comprend pas.
pub fn parse_index_tolerant(content: &str) -> anyhow::Result<ParsedIndex> {
    let value: Value = serde_json::from_str(content)?;
    let schema_version = value
        .get("schemaVersion")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("schemaVersion manquant"))?
        .to_owned();
    let generated_for = value
        .get("generatedFor")
        .and_then(Value::as_str)
        .map(str::to_owned);
    let raw_entries = value
        .get("entries")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow::anyhow!("entries manquant"))?;
    let mut entries = Vec::with_capacity(raw_entries.len());
    let mut skipped = Vec::new();
    for (position, raw) in raw_entries.iter().enumerate() {
        match serde_json::from_value::<CatalogEntry>(raw.clone()) {
            Ok(entry) => entries.push(entry),
            Err(error) => {
                let label = raw
                    .get("id")
                    .and_then(Value::as_str)
                    .map_or_else(|| format!("#{position}"), str::to_owned);
                skipped.push(format!("{label} : {error}"));
            }
        }
    }
    Ok(ParsedIndex {
        index: CatalogIndex {
            schema_version,
            generated_for,
            entries,
        },
        skipped,
    })
}

/// Charge l'index du catalogue depuis un dossier de dépôt ; les fiches inconnues sont journalisées.
pub fn load_catalog(plugins_root: &Path) -> anyhow::Result<CatalogIndex> {
    let index_path = plugins_root.join("catalogue").join("index.json");
    let content = std::fs::read_to_string(&index_path)?;
    let parsed = parse_index_tolerant(&content)?;
    for reason in &parsed.skipped {
        tracing::warn!(root = %plugins_root.display(), reason = %reason, "fiche de catalogue ignorée");
    }
    Ok(parsed.index)
}

/// Charge un manifeste spécifique depuis le répertoire racine des plugins.
pub fn load_manifest(plugins_root: &Path, manifest_path: &str) -> anyhow::Result<PluginManifest> {
    let full_path = plugins_root.join(manifest_path);
    let content = std::fs::read_to_string(&full_path)?;
    let manifest: PluginManifest = serde_json::from_str(&content)?;
    Ok(manifest)
}

/// Entrées du catalogue avec le dossier qui les porte ; le premier dossier gagne sur un identifiant en double.
///
/// La catégorie et les tags viennent du document pointé par l'index. Une catégorie
/// inconnue est déjà normalisée en `other` et n'est pas sérialisée (pas de pastille).
pub fn catalog_with_roots(roots: &[PathBuf]) -> Vec<(PathBuf, CatalogEntry)> {
    let mut seen = HashSet::new();
    let mut listed = Vec::new();
    for root in roots {
        let Ok(catalog) = load_catalog(root) else {
            continue;
        };
        for mut entry in catalog.entries {
            if !seen.insert(entry.id.clone()) {
                continue;
            }
            apply_document_labels(root, &mut entry);
            listed.push((root.clone(), entry));
        }
    }
    listed
}

/// Liste l'ensemble des entrées des dossiers de catalogue ; vide tant qu'aucun dépôt n'est synchronisé.
pub fn list_catalog(roots: &[PathBuf]) -> Vec<CatalogEntry> {
    catalog_with_roots(roots)
        .into_iter()
        .map(|(_, entry)| entry)
        .collect()
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
    roots: &[PathBuf],
    plugin_id: &str,
    saved: &Value,
) -> Vec<ResolvedSettingsField> {
    resolve_settings_fields(saved, &settings_fields(roots, plugin_id))
}

/// Champs déclarés par le schéma de réglages d'un plugin du catalogue ; vide si le plugin est inconnu.
pub fn settings_fields(roots: &[PathBuf], plugin_id: &str) -> Vec<SettingsField> {
    let Some((root, entry)) = catalog_with_roots(roots)
        .into_iter()
        .find(|(_, entry)| entry.id == plugin_id)
    else {
        return Vec::new();
    };
    load_manifest(&root, &entry.manifest_path)
        .ok()
        .and_then(|manifest| manifest.settings_schema)
        .map(|schema| schema.fields)
        .unwrap_or_default()
}

/// Le plugin existe-t-il dans le catalogue ?
pub fn plugin_exists(roots: &[PathBuf], plugin_id: &str) -> bool {
    catalog_with_roots(roots)
        .iter()
        .any(|(_, entry)| entry.id == plugin_id)
}

/// Calcule l'état dynamique de chaque outil déclaré par rapport au cache local du nœud.
pub fn list_tools_status(
    roots: &[PathBuf],
    cache_mgr: &ToolCacheManager,
) -> Vec<ToolStatusSummary> {
    let arch = host_arch();

    catalog_with_roots(roots)
        .into_iter()
        .filter(|(_, entry)| entry.entry_type == CatalogItemType::App)
        .map(|(root, entry)| {
            let description = load_manifest(&root, &entry.manifest_path)
                .ok()
                .and_then(|manifest| manifest.description)
                .unwrap_or_default();

            let is_system_ready = match entry.id.as_str() {
                "app.internal.telemetry" => true,
                "app.mkvtoolnix" => Path::new("/usr/bin/mkvmerge").exists(),
                _ => false,
            };

            let status = if is_system_ready || cache_mgr.is_ready(&entry.id, &entry.version, arch) {
                ToolStatus::Ready
            } else if is_installing(&entry.id) {
                ToolStatus::Installing
            } else {
                ToolStatus::NotInstalled
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
        let entry = CatalogEntry {
            id: "plugin.demo".to_owned(),
            entry_type: CatalogItemType::Plugin,
            version: "1.0.0".to_owned(),
            name: "Démo".to_owned(),
            manifest_path: "plugins/demo.json".to_owned(),
            capability: None,
            current_choice: None,
            distribution: None,
            category: None,
            tags: Vec::new(),
        };
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

        let entries = list_catalog(std::slice::from_ref(&root));
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].category, Some(PluginCategory::Other));
        assert!(!PluginCategory::Other.shows_badge());
        assert_eq!(entries[0].tags, vec!["arr".to_owned(), "tv".to_owned()]);
        let listed = serde_json::to_value(&entries[0])?;
        assert!(listed.get("category").is_none());

        let saved = json!({"url": ""});
        let fields = resolved_plugin_fields(std::slice::from_ref(&root), "plugin.demo", &saved);
        assert_eq!(fields[0].name, "url");
        assert_eq!(fields[0].value, "");
        assert_eq!(fields[1].name, "apiKey");
        assert_eq!(fields[1].value, "");

        let fresh = json!({});
        let proposed = resolved_plugin_fields(std::slice::from_ref(&root), "plugin.demo", &fresh);
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

    #[test]
    fn tool_status_serializes_to_the_names_the_interface_expects() -> Result<(), serde_json::Error>
    {
        assert_eq!(serde_json::to_string(&ToolStatus::Ready)?, "\"ready\"");
        assert_eq!(
            serde_json::to_string(&ToolStatus::Installing)?,
            "\"installing\""
        );
        assert_eq!(
            serde_json::to_string(&ToolStatus::NotInstalled)?,
            "\"not_installed\""
        );
        Ok(())
    }
}
