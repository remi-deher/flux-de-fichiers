//! Contrat des plugins : catalogue, dépôts, outils distribués et réglages (ADR 0014).

pub mod vocab;

use crate::flows::OwnedBlockSpec;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use vocab::{CatalogCapability, CatalogItemType, PluginCategory};

/// Dépôt distant de distribution de plugins, outils et flux.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "sqlx", derive(sqlx::FromRow))]
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
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateRepositoryRequest {
    pub id: Option<String>,
    pub name: String,
    pub index_url: String,
}

/// Fiche d'index dans `index.json` à la racine du dépôt.
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
    /// Plugin propriétaire. Obligatoire pour un bloc, qui ne vit jamais seul dans un dépôt.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner: Option<String>,
    /// Empreinte SHA-256 optionnelle du manifeste pour vérification à la synchronisation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
}

/// État d'un outil dans le cache local.
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

/// Type de champ connu des manifestes Sonarr et Radarr. Une valeur inconnue est refusée.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SettingsFieldType {
    Url,
    Secret,
    String,
}

/// Champ prêt à être affiché. `value` est la proposition, jamais une écriture en base.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedSettingsField {
    pub name: String,
    pub label: String,
    #[serde(rename = "type")]
    pub field_type: SettingsFieldType,
    pub required: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub value: String,
    /// Secret : une valeur est enregistrée (elle n'est jamais renvoyée).
    #[serde(rename = "isSet")]
    pub is_set: bool,
}

/// Réponse de `GET /api/v1/plugins/{id}/settings` : secrets jamais renvoyés (seulement `isSet`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginSettingsView {
    pub plugin_id: String,
    /// Valeurs enregistrées, secrets retirés.
    pub settings: Map<String, Value>,
    pub fields: Vec<ResolvedSettingsField>,
}

/// Composant déclaré dans un fichier `installation.json` de pack.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PackComponent {
    pub id: String,
    pub version: String,
    pub kind: CatalogItemType,
}

/// Contrat déclaratif `packs/*/installation.json`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PackInstallation {
    pub schema_version: String,
    pub pack_id: String,
    pub pack_version: String,
    pub kind: String,
    #[serde(default)]
    pub components: Vec<PackComponent>,
    #[serde(default)]
    pub entrypoint: Option<PackComponent>,
    #[serde(default)]
    pub runtime_implemented: Option<bool>,
}

/// Bilan d'installation d'un pack déclaratif.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PackInstallResult {
    pub pack_id: String,
    pub pack_version: String,
    pub installed_tools: Vec<String>,
    pub failed_tools: Vec<(String, String)>,
    pub verified_components: Vec<String>,
    pub missing_components: Vec<String>,
}

/// Dépendance déclarée dans le manifeste d'un plugin.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginDependency {
    pub id: String,
    #[serde(default)]
    pub version_range: Option<String>,
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub optional: bool,
}

/// Compatibilité déclarée par le manifeste d'un plugin.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginCompatibility {
    #[serde(default)]
    pub app_min: Option<String>,
    #[serde(default)]
    pub platforms: Vec<String>,
}

/// Fiche détaillée d'un plugin pour le tiroir d'inspection (`GET /api/v1/plugins/{id}`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginDetailView {
    pub id: String,
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub author: Option<String>,
    #[serde(default)]
    pub license: Option<String>,
    #[serde(default)]
    pub capability: Option<CatalogCapability>,
    #[serde(default, deserialize_with = "vocab::deserialize_optional_category")]
    pub category: Option<PluginCategory>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub dependencies: Vec<PluginDependency>,
    #[serde(default)]
    pub compatibility: PluginCompatibility,
    #[serde(default)]
    pub has_settings: bool,
    #[serde(default)]
    pub blocks: Vec<OwnedBlockSpec>,
}

/// Fiche détaillée d'un pack pour le tiroir à onglets (`GET /api/v1/packs/{id}`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PackDetailView {
    pub id: String,
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub author: Option<String>,
    #[serde(default)]
    pub license: Option<String>,
    #[serde(default, deserialize_with = "vocab::deserialize_optional_category")]
    pub category: Option<PluginCategory>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub compatibility: PluginCompatibility,
    pub installation: PackInstallation,
}

/// Artefact déclaré dans `distribution.v1.json` pour une architecture donnée.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolArtifactSummary {
    pub arch: String,
    pub archive_format: String,
    pub archive_sha256: String,
    pub binary_relative_path: String,
    pub host_supported: bool,
}

/// Fiche détaillée d'un outil distribué pour le tiroir à onglets (`GET /api/v1/tools/{id}`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolDetailView {
    pub summary: ToolStatusSummary,
    #[serde(default)]
    pub author: Option<String>,
    #[serde(default)]
    pub license: Option<String>,
    #[serde(default)]
    pub interface: Option<String>,
    #[serde(default)]
    pub invocation: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub compatibility: PluginCompatibility,
    #[serde(default)]
    pub tool_version: Option<String>,
    #[serde(default)]
    pub release_status: Option<String>,
    #[serde(default)]
    pub distribution_mode: Option<String>,
    #[serde(default)]
    pub artifacts: Vec<ToolArtifactSummary>,
    #[serde(default)]
    pub used_by_plugins: Vec<String>,
}

/// Fiche détaillée d'un dépôt de plugins (`GET /api/v1/plugins/repositories/{id}`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RepositoryDetailView {
    pub repository: PluginRepository,
    pub synced_on_disk: bool,
    #[serde(default)]
    pub schema_version: Option<String>,
    #[serde(default)]
    pub release_version: Option<String>,
    pub entries_count: usize,
    pub skipped_count: usize,
    #[serde(default)]
    pub entries: Vec<CatalogEntry>,
}
