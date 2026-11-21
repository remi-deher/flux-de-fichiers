//! Gestionnaire de plugins, chargement du catalogue et inspection des manifestes.

use crate::tools_cache::{ToolCacheManager, host_arch};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};

mod blocks;
mod cache;
mod dependencies;
mod icon;
mod install;
mod legacy_ids;
mod local;
mod packs;
mod probe;
mod readiness;
mod script_preview;
mod secret_settings;
mod settings;
mod sources;
mod subcategory;
mod sync;

pub use blocks::{flow_block_catalog, plugin_default_config};
pub use cache::CatalogCache;
pub use dependencies::{
    DependencyInstallResult, DependencyPlan, RequiredTool, install_with_dependencies,
    resolve_dependencies,
};
pub use install::{
    ensure_tool_installed, ensure_tool_installed_verified, ensure_tools_for_requirements,
    install_from_distribution, is_installing, parse_distribution, read_distribution,
};
pub use legacy_ids::{
    legacy_plugin_id, migrate_legacy_block_kinds, migrate_legacy_plugin_secrets,
    rename_legacy_block_kinds,
};
pub use local::{
    build_local_snapshot, delete_local_item, local_resource_file, save_local_block_template,
    save_local_flow_template, save_local_pack, save_local_script, upload_local_resource,
};
pub use packs::{
    PackInstallResult, PackInstallation, build_pack_detail, install_pack, install_pack_verified,
    read_pack_installation,
};
pub use probe::{ProbeContract, file_job_may_start, probe_is_ready, required_probe};
pub use readiness::{attach_dependency_statuses, compute_entry_runtime, is_plugin_configured};
pub use sources::{
    catalog_roots, is_valid_repository_id, local_catalog_dir, repository_dir,
    sync_enabled_repositories,
};
pub use sync::{
    REQUIRE_IMMUTABLE_RELEASES_ENV, SyncError, SyncPolicy, SyncReport, is_mutable_branch_url,
    sync_repository, sync_repository_verified, validate_index_url,
};

pub use forgearr_shared::plugins::vocab::{
    self, CatalogCapability, CatalogItemType, PluginCategory,
};
pub use forgearr_shared::plugins::{
    CatalogEntry, CatalogFlowEdge, CatalogFlowNode, CatalogFlowPreview, CatalogReadiness,
    CatalogRuntimeSummary, CreateRepositoryRequest, PackDetailView, PluginCompatibility,
    PluginConnectionTestResult, PluginDependency, PluginDetailView, PluginRepository,
    PluginSettingsView, RepositoryDetailView, ToolArtifactSummary, ToolDetailView, ToolStatus,
    ToolStatusSummary,
};
pub use script_preview::read_repository_script;
pub use secret_settings::{
    decrypt_all, mask, migrate_plaintext, plugin_secret_aad, reseal_legacy_settings,
    rotate_settings, seal, secret_names,
};
pub use settings::{
    ResolvedSettingsField, SettingsField, SettingsFieldType, SettingsSchema, resolve_setting_value,
    resolve_settings_fields,
};

/// Résout dynamiquement la version d'un outil depuis `distribution.v1.json`, sinon le catalogue.
pub fn resolve_tool_version(roots: &[PathBuf], tool_id: &str) -> String {
    if let Ok(value) = read_distribution(roots, tool_id)
        && let Ok(dist) = parse_distribution(&value.to_string())
        && !dist.tool_version.trim().is_empty()
    {
        return dist.tool_version;
    }
    catalog_with_roots(roots)
        .into_iter()
        .find(|(_, entry)| entry.id == tool_id)
        .map(|(_, entry)| entry.version)
        .filter(|version| !version.trim().is_empty())
        .unwrap_or_else(|| "1.0.0".to_owned())
}

/// Conteneur de l'index du catalogue.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CatalogIndex {
    pub schema_version: String,
    #[serde(default)]
    pub generated_for: Option<String>,
    #[serde(default)]
    pub release_version: Option<String>,
    #[serde(default)]
    pub files_sha256: std::collections::HashMap<String, String>,
    pub entries: Vec<CatalogEntry>,
}

/// Fichier déclaré dans `files[]` d'un manifeste.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ManifestFileEntry {
    pub path: String,
    #[serde(default)]
    pub role: Option<String>,
    #[serde(default)]
    pub media_type: Option<String>,
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
    pub capability: Option<CatalogCapability>,
    #[serde(default)]
    pub tool: Option<ManifestToolDetails>,
    #[serde(default)]
    pub settings_schema: Option<SettingsSchema>,
    #[serde(default)]
    pub files: Vec<ManifestFileEntry>,
    #[serde(default)]
    pub dependencies: Vec<PluginDependency>,
    #[serde(default)]
    pub compatibility: Option<PluginCompatibility>,
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

/// Index lu entrée par entrée : une fiche inconnue est ignorée sans invalider les autres.
pub struct ParsedIndex {
    pub index: CatalogIndex,
    /// Fiches ignorées, avec la raison.
    pub skipped: Vec<String>,
}

/// Lit un `index.json` à la racine en tolérant les fiches que ce central ne comprend pas.
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
    let release_version = value
        .get("releaseVersion")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned);
    let files_sha256 = value
        .get("filesSha256")
        .and_then(Value::as_object)
        .map(|obj| {
            obj.iter()
                .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_owned())))
                .collect()
        })
        .unwrap_or_default();
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
    drop_orphan_blocks(&mut entries, &mut skipped);
    Ok(ParsedIndex {
        index: CatalogIndex {
            schema_version,
            generated_for,
            release_version,
            files_sha256,
            entries,
        },
        skipped,
    })
}

/// Un bloc appartient à un plugin ou script du même index : sans propriétaire connu, il est écarté.
fn drop_orphan_blocks(entries: &mut Vec<CatalogEntry>, skipped: &mut Vec<String>) {
    let owners: HashSet<String> = entries
        .iter()
        .filter(|entry| {
            matches!(
                entry.entry_type,
                CatalogItemType::Plugin | CatalogItemType::Script
            )
        })
        .map(|entry| entry.id.clone())
        .collect();
    entries.retain(|entry| {
        let orphan = !entry.entry_type.can_stand_alone()
            && !entry
                .owner
                .as_ref()
                .is_some_and(|owner| owners.contains(owner));
        if orphan {
            skipped.push(format!(
                "{} : un bloc doit appartenir à un plugin du dépôt (owner)",
                entry.id
            ));
        }
        !orphan
    });
}

/// Charge l'index du catalogue depuis un dossier de dépôt ; les fiches inconnues sont journalisées.
pub fn load_catalog(plugins_root: &Path) -> anyhow::Result<CatalogIndex> {
    let index_path = plugins_root.join("index.json");
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
            apply_document_metadata(root, &mut entry);
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

/// Pièces visibles par l'utilisateur : tout le catalogue sauf les blocs, qui appartiennent à un plugin.
pub fn exposed_catalog(roots: &[PathBuf]) -> Vec<CatalogEntry> {
    list_catalog(roots)
        .into_iter()
        .filter(|entry| entry.entry_type.section().is_some())
        .collect()
}

/// Complète l'entrée d'index avec son manifeste : catégorie, sous-catégorie, étiquettes et description.
fn apply_document_metadata(root: &Path, entry: &mut CatalogEntry) {
    let document = std::fs::read_to_string(root.join(&entry.manifest_path))
        .ok()
        .and_then(|content| serde_json::from_str::<Value>(&content).ok());
    let labels = document.as_ref().map(vocab::labels_from_document);
    if let Some(labels) = &labels {
        if labels.category_declared {
            entry.category = labels.category;
        }
        if labels.tags_declared {
            entry.tags = labels.tags.clone();
        }
    }
    subcategory::resolve_entry_subcategory(
        entry,
        labels
            .as_ref()
            .and_then(|labels| labels.subcategory.as_deref()),
    );
    entry.has_icon = icon::entry_has_icon(root, entry, document.as_ref());
    if entry.description.is_none() {
        entry.description = document
            .as_ref()
            .and_then(|value| value.get("description"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .map(str::to_owned);
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

/// Icône SVG d'un plugin du catalogue, relue et validée à chaque appel ; `None` sans icône valide.
///
/// Comme pour le catalogue, le premier dossier qui porte l'identifiant l'emporte.
pub fn plugin_icon_svg(roots: &[PathBuf], plugin_id: &str) -> Option<String> {
    for root in roots {
        let Ok(content) = std::fs::read_to_string(root.join("index.json")) else {
            continue;
        };
        let Ok(parsed) = parse_index_tolerant(&content) else {
            continue;
        };
        if let Some(entry) = parsed
            .index
            .entries
            .iter()
            .find(|entry| entry.id == plugin_id)
        {
            return icon::read_valid_icon(root, &entry.manifest_path);
        }
    }
    None
}

/// Le plugin existe-t-il dans le catalogue ?
pub fn plugin_exists(roots: &[PathBuf], plugin_id: &str) -> bool {
    catalog_with_roots(roots)
        .iter()
        .any(|(_, entry)| entry.id == plugin_id)
}

/// Construit la fiche détaillée d'un plugin à partir de son entrée de catalogue, de son manifeste et de ses blocs.
pub fn build_plugin_detail(
    root: &Path,
    entry: &CatalogEntry,
    all_blocks: &[forgearr_shared::flows::OwnedBlockSpec],
) -> PluginDetailView {
    let manifest = load_manifest(root, &entry.manifest_path).ok();
    let blocks = all_blocks
        .iter()
        .filter(|block| block.owner.as_deref() == Some(entry.id.as_str()))
        .cloned()
        .collect();
    match manifest {
        Some(manifest) => PluginDetailView {
            id: entry.id.clone(),
            name: manifest.name,
            version: manifest.version,
            description: manifest.description,
            author: manifest.author,
            license: manifest.license,
            capability: manifest.capability.or_else(|| entry.capability.clone()),
            category: manifest.category.or(entry.category),
            tags: if manifest.tags.is_empty() {
                entry.tags.clone()
            } else {
                manifest.tags
            },
            dependencies: manifest.dependencies,
            compatibility: manifest.compatibility.unwrap_or_default(),
            has_settings: manifest
                .settings_schema
                .is_some_and(|schema| !schema.fields.is_empty()),
            blocks,
            updated_at: entry.updated_at,
        },
        None => PluginDetailView {
            id: entry.id.clone(),
            name: entry.name.clone(),
            version: entry.version.clone(),
            description: None,
            author: None,
            license: None,
            capability: entry.capability.clone(),
            category: entry.category,
            tags: entry.tags.clone(),
            dependencies: Vec::new(),
            compatibility: PluginCompatibility::default(),
            has_settings: false,
            blocks,
            updated_at: entry.updated_at,
        },
    }
}

/// Construit la fiche détaillée d'un outil distribué (distribution, compatibilité et plugins utilisateurs).
pub fn build_tool_detail(
    roots: &[PathBuf],
    cache_mgr: &ToolCacheManager,
    tool_id: &str,
) -> Option<ToolDetailView> {
    let entries = catalog_with_roots(roots);
    let (root, entry) = entries
        .iter()
        .find(|(_, entry)| entry.id == tool_id && entry.entry_type == CatalogItemType::App)?;
    let manifest = load_manifest(root, &entry.manifest_path).ok();
    let description = manifest
        .as_ref()
        .and_then(|m| m.description.clone())
        .unwrap_or_default();
    let distribution = read_distribution(roots, tool_id)
        .ok()
        .and_then(|value| parse_distribution(&value.to_string()).ok());
    let installable = distribution
        .as_ref()
        .is_some_and(distribution_is_installable);
    let summary = summarize_tool(entry.clone(), description, installable, cache_mgr);
    let host = host_arch();
    let artifacts = distribution
        .as_ref()
        .map(|dist| {
            dist.artifacts
                .iter()
                .map(|art| ToolArtifactSummary {
                    arch: art.arch.clone(),
                    archive_format: art.archive_format.clone(),
                    archive_sha256: art.archive_sha256.clone(),
                    binary_relative_path: art.binary_relative_path.clone(),
                    host_supported: art.arch == host,
                })
                .collect()
        })
        .unwrap_or_default();
    let used_by_plugins = entries
        .iter()
        .filter(|(_, item)| item.entry_type == CatalogItemType::Plugin)
        .filter(|(plugin_root, item)| {
            load_manifest(plugin_root, &item.manifest_path)
                .ok()
                .is_some_and(|m| m.dependencies.iter().any(|dep| dep.id == tool_id))
        })
        .map(|(_, item)| item.id.clone())
        .collect();
    let tags = manifest
        .as_ref()
        .filter(|m| !m.tags.is_empty())
        .map(|m| m.tags.clone())
        .unwrap_or_else(|| entry.tags.clone());
    Some(ToolDetailView {
        summary,
        author: manifest.as_ref().and_then(|m| m.author.clone()),
        license: manifest.as_ref().and_then(|m| m.license.clone()),
        interface: manifest
            .as_ref()
            .and_then(|m| m.tool.as_ref().and_then(|t| t.interface.clone())),
        invocation: manifest
            .as_ref()
            .and_then(|m| m.tool.as_ref().and_then(|t| t.invocation.clone())),
        tags,
        compatibility: manifest.and_then(|m| m.compatibility).unwrap_or_default(),
        tool_version: distribution.as_ref().map(|d| d.tool_version.clone()),
        release_status: distribution.as_ref().and_then(|d| d.release_status.clone()),
        released_at: distribution.as_ref().and_then(|d| d.released_at),
        distribution_mode: distribution.map(|d| d.distribution_mode),
        artifacts,
        used_by_plugins,
    })
}

/// Construit la fiche détaillée d'un dépôt de plugins à partir de sa copie locale synchronisée.
pub fn build_repository_detail(
    repository: PluginRepository,
    repos_dir: &Path,
) -> RepositoryDetailView {
    let Some(dir) = repository_dir(repos_dir, &repository.id) else {
        return RepositoryDetailView {
            repository,
            synced_on_disk: false,
            schema_version: None,
            release_version: None,
            entries_count: 0,
            skipped_count: 0,
            warnings: Vec::new(),
            entries: Vec::new(),
        };
    };
    let index_path = dir.join("index.json");
    let Ok(content) = std::fs::read_to_string(&index_path) else {
        return RepositoryDetailView {
            repository,
            synced_on_disk: false,
            schema_version: None,
            release_version: None,
            entries_count: 0,
            skipped_count: 0,
            warnings: Vec::new(),
            entries: Vec::new(),
        };
    };
    match parse_index_tolerant(&content) {
        Ok(parsed) => {
            let entries_count = parsed.index.entries.len();
            let warnings = subcategory::repository_warnings(&parsed.index.entries, |entry| {
                std::fs::read_to_string(dir.join(&entry.manifest_path)).ok()
            });
            let entries = parsed
                .index
                .entries
                .into_iter()
                .filter(|entry| entry.entry_type.section().is_some())
                .collect();
            RepositoryDetailView {
                repository,
                synced_on_disk: true,
                schema_version: Some(parsed.index.schema_version),
                release_version: parsed.index.release_version,
                entries_count,
                skipped_count: parsed.skipped.len(),
                warnings,
                entries,
            }
        }
        Err(_) => RepositoryDetailView {
            repository,
            synced_on_disk: true,
            schema_version: None,
            release_version: None,
            entries_count: 0,
            skipped_count: 0,
            warnings: Vec::new(),
            entries: Vec::new(),
        },
    }
}

fn distribution_is_installable(dist: &crate::tools_cache::ToolDistribution) -> bool {
    dist.release_status.as_deref() != Some("contract-only")
        && install::artifact_for_host(dist).is_some()
}

/// Indique si l'outil dispose d'un contrat distribuable pour l'architecture de cet hôte.
pub fn is_tool_installable(roots: &[PathBuf], tool_id: &str) -> bool {
    let Ok(raw) = read_distribution(roots, tool_id) else {
        return false;
    };
    let Ok(dist) = parse_distribution(&raw.to_string()) else {
        return false;
    };
    distribution_is_installable(&dist)
}

/// Résumé d'un outil du catalogue : prêt s'il est dans le cache local, sinon installable selon son contrat.
pub fn summarize_tool(
    entry: CatalogEntry,
    description: String,
    installable: bool,
    cache_mgr: &ToolCacheManager,
) -> ToolStatusSummary {
    let arch = host_arch();
    // La télémétrie est interne au binaire ; tout autre outil vient du cache local.
    let is_system_ready = entry.id == "telemetrie";
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
        capability: entry.capability.unwrap_or_default(),
        current_choice: entry.current_choice.unwrap_or(false),
        arch: arch.to_string(),
        distribution: entry
            .distribution
            .unwrap_or_else(|| "autonomous-binary".into()),
        description,
        installable,
        updated_at: entry.updated_at,
    }
}

/// Calcule l'état dynamique de chaque outil déclaré par rapport au cache local.
pub fn list_tools_status(
    roots: &[PathBuf],
    cache_mgr: &ToolCacheManager,
) -> Vec<ToolStatusSummary> {
    catalog_with_roots(roots)
        .into_iter()
        .filter(|(_, entry)| entry.entry_type == CatalogItemType::App)
        .map(|(root, entry)| {
            let description = load_manifest(&root, &entry.manifest_path)
                .ok()
                .and_then(|manifest| manifest.description)
                .unwrap_or_default();
            let installable = is_tool_installable(roots, &entry.id);
            summarize_tool(entry, description, installable, cache_mgr)
        })
        .collect()
}

/// Outils d'un catalogue lu à distance : l'état vient du cache local, sans description
/// (les manifestes restent sur le central).
pub fn tool_statuses_for_entries(
    entries: Vec<CatalogEntry>,
    cache_mgr: &ToolCacheManager,
) -> Vec<ToolStatusSummary> {
    entries
        .into_iter()
        .filter(|entry| entry.entry_type == CatalogItemType::App)
        .map(|entry| summarize_tool(entry, String::new(), true, cache_mgr))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::path::Path;

    #[test]
    fn unknown_catalog_types_and_empty_capabilities_are_rejected() {
        let bad_type = r#"{"schemaVersion":"1.0.0","entries":[{"id":"x","type":"extension","version":"1.0.0","name":"X","manifestPath":"x.json"}]}"#;
        assert!(serde_json::from_str::<CatalogIndex>(bad_type).is_err());
        let empty_capability = r#"{"schemaVersion":"1.0.0","entries":[{"id":"x","type":"app","version":"1.0.0","name":"X","manifestPath":"x.json","capability":"   "}]}"#;
        assert!(serde_json::from_str::<CatalogIndex>(empty_capability).is_err());
        let custom_capability = r#"{"schemaVersion":"1.0.0","entries":[{"id":"x","type":"app","version":"1.0.0","name":"X","manifestPath":"x.json","capability":"studio"}]}"#;
        assert!(serde_json::from_str::<CatalogIndex>(custom_capability).is_ok());
    }

    #[test]
    fn the_catalog_takes_each_description_from_its_manifest() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        std::fs::write(
            dir.path().join("index.json"),
            r#"{"schemaVersion":"1.0.0","entries":[
                {"id":"archives","type":"plugin","version":"1.0.0","name":"Archives","manifestPath":"archives.json"},
                {"id":"muet","type":"plugin","version":"1.0.0","name":"Muet","manifestPath":"muet.json"},
                {"id":"indexe","type":"plugin","version":"1.0.0","name":"Indexé","manifestPath":"indexe.json","description":"Donnée par l'index."}
            ]}"#,
        )?;
        std::fs::write(
            dir.path().join("archives.json"),
            r#"{"description":"  Extraction des archives téléchargées.  "}"#,
        )?;
        std::fs::write(dir.path().join("muet.json"), r#"{"description":"   "}"#)?;
        std::fs::write(
            dir.path().join("indexe.json"),
            r#"{"description":"Celle du manifeste."}"#,
        )?;
        let catalog = list_catalog(&[dir.path().to_path_buf()]);
        let description = |id: &str| {
            catalog
                .iter()
                .find(|entry| entry.id == id)
                .and_then(|entry| entry.description.clone())
        };
        assert_eq!(
            description("archives").as_deref(),
            Some("Extraction des archives téléchargées.")
        );
        assert_eq!(description("muet"), None);
        assert_eq!(
            description("indexe").as_deref(),
            Some("Donnée par l'index.")
        );
        Ok(())
    }

    #[test]
    fn other_category_is_kept_in_memory_and_omitted_from_json() -> Result<(), serde_json::Error> {
        let entry = CatalogEntry {
            id: "plugin.demo".to_owned(),
            entry_type: CatalogItemType::Plugin,
            version: "1.0.0".to_owned(),
            name: "Démo".to_owned(),
            manifest_path: "plugins/demo.json".to_owned(),
            description: None,
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
        std::fs::create_dir_all(root.join("plugins"))?;
        std::fs::write(
            root.join("index.json"),
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
    fn listed_catalog_keeps_only_official_subcategories_under_their_category() -> anyhow::Result<()>
    {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/plugin-subcategory-test");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("plugins"))?;
        let plugins = [
            (
                "plex",
                r#""category": "media", "subcategory": "media-servers""#,
            ),
            (
                "jellyfin",
                r#""category": "media", "subcategory": "Media Servers""#,
            ),
            ("custom", r#""category": "media", "subcategory": "studio""#),
            (
                "webhook",
                r#""category": "io", "subcategory": "media-servers""#,
            ),
            ("bare", r#""category": "media""#),
        ];
        let mut index_entries = Vec::new();
        for (name, labels) in plugins {
            index_entries.push(format!(
                r#"{{"id": "plugin.{name}", "type": "plugin", "version": "1.0.0", "name": "{name}", "manifestPath": "plugins/{name}.json"}}"#
            ));
            std::fs::write(
                root.join(format!("plugins/{name}.json")),
                format!(
                    r#"{{"schemaVersion": "1.1.0", "id": "plugin.{name}", "type": "plugin", "version": "1.0.0", "name": "{name}", {labels}}}"#
                ),
            )?;
        }
        std::fs::write(
            root.join("index.json"),
            format!(
                r#"{{"schemaVersion": "1.0.0", "entries": [{}]}}"#,
                index_entries.join(",")
            ),
        )?;

        let entries = list_catalog(std::slice::from_ref(&root));
        let subcategory_of = |name: &str| {
            entries
                .iter()
                .find(|entry| entry.id == format!("plugin.{name}"))
                .and_then(|entry| entry.subcategory.as_ref().map(ToString::to_string))
        };
        assert_eq!(subcategory_of("plex").as_deref(), Some("media-servers"));
        assert_eq!(subcategory_of("jellyfin"), None);
        assert_eq!(subcategory_of("custom"), None);
        assert_eq!(subcategory_of("webhook"), None);
        assert_eq!(subcategory_of("bare"), None);
        assert_eq!(entries.len(), 5);

        let listed = serde_json::to_value(&entries)?;
        let serialized: Vec<bool> = listed
            .as_array()
            .map(|items| {
                items
                    .iter()
                    .map(|item| item.get("subcategory").is_some())
                    .collect()
            })
            .unwrap_or_default();
        assert_eq!(serialized.iter().filter(|present| **present).count(), 1);

        std::fs::remove_dir_all(&root)?;
        Ok(())
    }

    #[test]
    fn sibling_plugin_repository_loads_with_these_enums() -> anyhow::Result<()> {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../flux-de-fichiers-plugins");
        let index = root.join("index.json");
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

        for plugin_id in ["sonarr", "radarr"] {
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

    #[test]
    fn blocks_without_a_known_owner_plugin_are_dropped() -> anyhow::Result<()> {
        let raw = r#"{"schemaVersion":"1.0.0","entries":[
            {"id":"plugin.a","type":"plugin","version":"1.0.0","name":"A","manifestPath":"a.json"},
            {"id":"block.a.run","type":"block","owner":"plugin.a","version":"1.0.0","name":"Run","manifestPath":"plugins/a/blocks/run.json"},
            {"id":"block.alone","type":"block","version":"1.0.0","name":"Seul","manifestPath":"b.json"},
            {"id":"block.lost","type":"block","owner":"plugin.absent","version":"1.0.0","name":"Perdu","manifestPath":"c.json"},
            {"id":"block.wrong","type":"block","owner":"app.x","version":"1.0.0","name":"Faux","manifestPath":"d.json"},
            {"id":"app.x","type":"app","version":"1.0.0","name":"X","manifestPath":"x.json"}]}"#;
        let parsed = parse_index_tolerant(raw)?;
        let ids: Vec<_> = parsed.index.entries.iter().map(|e| e.id.as_str()).collect();
        assert_eq!(ids, vec!["plugin.a", "block.a.run", "app.x"]);
        assert_eq!(parsed.skipped.len(), 3);
        Ok(())
    }

    #[test]
    fn exposed_catalog_never_lists_blocks() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        std::fs::write(
            dir.path().join("index.json"),
            r#"{"schemaVersion":"1.0.0","entries":[
                {"id":"plugin.a","type":"plugin","version":"1.0.0","name":"A","manifestPath":"a.json"},
                {"id":"block.a.run","type":"block","owner":"plugin.a","version":"1.0.0","name":"Run","manifestPath":"b.json"},
                {"id":"pack.x","type":"pack","version":"1.0.0","name":"X","manifestPath":"x.json"}]}"#,
        )?;
        let roots = [dir.path().to_path_buf()];
        assert_eq!(list_catalog(&roots).len(), 3);
        let ids: Vec<_> = exposed_catalog(&roots)
            .into_iter()
            .map(|entry| entry.id)
            .collect();
        assert_eq!(ids, vec!["plugin.a", "pack.x"]);
        Ok(())
    }

    #[test]
    fn build_plugin_detail_filters_owned_blocks_and_reads_manifest() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        std::fs::create_dir_all(dir.path().join("plugins/sonarr"))?;
        std::fs::write(
            dir.path().join("plugins/sonarr/manifest.json"),
            r#"{
                "schemaVersion": "1.1.0",
                "id": "sonarr",
                "type": "plugin",
                "version": "1.2.0",
                "name": "Sonarr Integration",
                "description": "Synchronisation Sonarr",
                "author": "forgearr",
                "license": "GPL-2.0-or-later",
                "capability": "integration",
                "dependencies": [{"id": "app.curl", "versionRange": "^8.0", "kind": "app", "optional": true}],
                "compatibility": {"appMin": "1.0.0", "platforms": ["linux/amd64"]},
                "settingsSchema": {"fields": [{"name": "url", "label": "URL", "type": "url", "required": true}]}
            }"#,
        )?;
        let entry = CatalogEntry {
            id: "sonarr".into(),
            entry_type: CatalogItemType::Plugin,
            version: "1.2.0".into(),
            name: "Sonarr Integration".into(),
            manifest_path: "plugins/sonarr/manifest.json".into(),
            description: None,
            capability: None,
            current_choice: None,
            distribution: None,
            category: None,
            subcategory: None,
            has_icon: false,
            tags: vec!["arr".into()],
            owner: None,
            sha256: None,
            updated_at: None,
            runtime: None,
        };
        let block = |kind: &str, owner: Option<&str>| forgearr_shared::flows::OwnedBlockSpec {
            kind: kind.into(),
            aliases: Vec::new(),
            label: kind.into(),
            category: "Intégrations".into(),
            domain: None,
            description: String::new(),
            has_input: true,
            ports: Vec::new(),
            terminal: false,
            fields: Vec::new(),
            suggested_next: Vec::new(),
            from_plugin: true,
            operation_id: None,
            owner: owner.map(str::to_owned),
        };
        let detail = build_plugin_detail(
            dir.path(),
            &entry,
            &[
                block("sonarr.rescan", Some("sonarr")),
                block("radarr.rescan", Some("radarr")),
            ],
        );
        assert_eq!(detail.id, "sonarr");
        assert!(detail.has_settings);
        assert_eq!(detail.author.as_deref(), Some("forgearr"));
        assert_eq!(detail.compatibility.platforms, vec!["linux/amd64"]);
        assert_eq!(detail.dependencies.len(), 1);
        assert_eq!(detail.blocks.len(), 1);
        assert_eq!(detail.blocks[0].kind, "sonarr.rescan");
        Ok(())
    }

    #[test]
    fn is_tool_installable_distinguishes_released_from_contract_only() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let arch = host_arch();
        for (folder, id, status) in [
            ("released-tool", "released-tool", "released"),
            ("pending-tool", "pending-tool", "contract-only"),
        ] {
            let tool_dir = dir.path().join("apps").join(folder);
            std::fs::create_dir_all(&tool_dir)?;
            std::fs::write(
                tool_dir.join("distribution.v1.json"),
                serde_json::json!({
                    "schemaVersion": "1.0.0",
                    "toolId": id,
                    "toolVersion": "1.0.0",
                    "distributionMode": "autonomous-binary",
                    "releaseStatus": status,
                    "artifacts": [{
                        "arch": arch,
                        "os": "linux",
                        "archiveUrl": "https://example.invalid/tool.tar.gz",
                        "archiveFormat": "tar.gz",
                        "archiveSha256": "0000",
                        "binaryRelativePath": "tool"
                    }]
                })
                .to_string(),
            )?;
        }
        let roots = [dir.path().to_path_buf()];
        assert!(is_tool_installable(&roots, "released-tool"));
        assert!(!is_tool_installable(&roots, "pending-tool"));
        assert!(!is_tool_installable(&roots, "unknown"));
        Ok(())
    }
}
