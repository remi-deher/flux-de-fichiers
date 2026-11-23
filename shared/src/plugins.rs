//! Contrat des plugins : catalogue, dépôts, outils distribués et réglages (ADR 0014).

pub mod store;
pub mod subcategory;
pub mod vocab;

use crate::flows::OwnedBlockSpec;
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use subcategory::SubcategoryId;
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

/// État opérationnel synthétique d'une pièce du catalogue calculé par le central.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CatalogReadiness {
    /// Outils requis prêts et réglages obligatoires configurés (ou non requis).
    Ready,
    /// Au moins un outil requis par cette pièce n'est pas encore installé.
    NeedsTools,
    /// Outils prêts, mais des réglages globaux restent à configurer.
    NeedsConfig,
    /// Élément déclaratif sans prérequis d'installation ni réglages (flux, script, ressource).
    #[default]
    Available,
}

/// Nœud d'un modèle de flux du catalogue pour prévisualisation dans l'interface.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogFlowNode {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub name: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub summary: Vec<String>,
}

/// Connexion entre deux nœuds d'un modèle de flux du catalogue.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogFlowEdge {
    pub from: String,
    pub to: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port: Option<String>,
}

/// Aperçu structuré du graphe d'un flux du catalogue (blocs et connexions).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogFlowPreview {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_mask: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub nodes: Vec<CatalogFlowNode>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub connections: Vec<CatalogFlowEdge>,
}

/// Synthèse d'état calculée côté central pour enrichir les cartes du catalogue (absente d'`index.json`).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogRuntimeSummary {
    pub readiness: CatalogReadiness,
    #[serde(default)]
    pub has_settings: bool,
    #[serde(default)]
    pub configured: bool,
    #[serde(default)]
    pub missing_tools_count: usize,
    #[serde(default)]
    pub blocks_count: usize,
    #[serde(default)]
    pub components_count: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub flow_preview: Option<CatalogFlowPreview>,
}

fn is_false(value: &bool) -> bool {
    !*value
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
    /// Ce que fait la pièce, repris de son manifeste par le central si l'index ne le donne pas.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
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
    /// Sous-catégorie officielle retenue (ADR 0023) ; absente si le dépôt n'en déclare pas ou
    /// si sa valeur n'est pas validée par le registre.
    #[serde(
        default,
        deserialize_with = "subcategory::deserialize_optional_subcategory",
        skip_serializing_if = "Option::is_none"
    )]
    pub subcategory: Option<SubcategoryId>,
    /// Le manifeste déclare une icône SVG présente dans la copie locale ; elle se lit par
    /// `GET /api/v1/plugins/{id}/icon` (ADR 0024).
    #[serde(default, skip_serializing_if = "is_false")]
    pub has_icon: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    /// Plugin propriétaire. Obligatoire pour un bloc, qui ne vit jamais seul dans un dépôt.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner: Option<String>,
    /// Empreinte SHA-256 optionnelle du manifeste pour vérification à la synchronisation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
    /// Date de dernière modification du contenu, calculée par le générateur d'index.
    #[serde(
        default,
        deserialize_with = "vocab::deserialize_optional_date",
        skip_serializing_if = "Option::is_none"
    )]
    pub updated_at: Option<NaiveDate>,
    /// État opérationnel calculé par le central (jamais écrit dans `index.json`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime: Option<CatalogRuntimeSummary>,
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

/// Résultat du test de connectivité d'un plugin configuré par API HTTP.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginConnectionTestResult {
    pub ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status_code: Option<u16>,
    pub latency_ms: u64,
    pub message: String,
}

const fn default_true() -> bool {
    true
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
    #[serde(default = "default_true")]
    pub installable: bool,
    #[serde(
        default,
        deserialize_with = "vocab::deserialize_optional_date",
        skip_serializing_if = "Option::is_none"
    )]
    pub updated_at: Option<NaiveDate>,
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

/// Bilan d'installation d'une pièce du catalogue et des outils qu'elle requiert
/// (`POST /api/v1/plugins/{id}/install`, `POST /api/v1/tools/{id}/install`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DependencyInstallResult {
    pub item_id: String,
    pub installed_tools: Vec<String>,
    pub failed_tools: Vec<(String, String)>,
    pub missing_dependencies: Vec<String>,
    /// Dépôts désignés par des scripts pour fournir une dépendance absente : à ajouter explicitement.
    #[serde(default)]
    pub suggested_repositories: Vec<crate::exposed_variables::RepositoryPointer>,
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
    /// État réel de l'outil dans le cache local du central (renseigné par `GET /api/v1/plugins/{id}`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<ToolStatus>,
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
    /// Variables que ce plugin expose aux scripts (ADR 0025).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub exposures: Vec<crate::exposed_variables::ExposedNamespaceView>,
    #[serde(
        default,
        deserialize_with = "vocab::deserialize_optional_date",
        skip_serializing_if = "Option::is_none"
    )]
    pub updated_at: Option<NaiveDate>,
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
    #[serde(
        default,
        deserialize_with = "vocab::deserialize_optional_date",
        skip_serializing_if = "Option::is_none"
    )]
    pub updated_at: Option<NaiveDate>,
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
    /// Date de publication de la Release des artefacts (`releasedAt` du contrat).
    #[serde(
        default,
        deserialize_with = "vocab::deserialize_optional_date",
        skip_serializing_if = "Option::is_none"
    )]
    pub released_at: Option<NaiveDate>,
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
    /// Sous-catégories déclarées par le dépôt mais non retenues (ADR 0023).
    #[serde(default)]
    pub warnings: Vec<String>,
    #[serde(default)]
    pub entries: Vec<CatalogEntry>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_entry_keeps_index_json_backward_compatible() -> Result<(), serde_json::Error> {
        let raw = r#"{"id":"sonarr","type":"plugin","version":"1.0.0","name":"Sonarr","manifestPath":"sonarr/manifest.json"}"#;
        let entry: CatalogEntry = serde_json::from_str(raw)?;
        assert!(entry.runtime.is_none());

        let serialized = serde_json::to_string(&entry)?;
        assert!(!serialized.contains("runtime"));
        Ok(())
    }

    #[test]
    fn catalog_runtime_summary_and_dependency_status_round_trip() -> Result<(), serde_json::Error> {
        let summary = CatalogRuntimeSummary {
            readiness: CatalogReadiness::NeedsTools,
            has_settings: true,
            configured: false,
            missing_tools_count: 2,
            blocks_count: 3,
            components_count: 0,
            flow_preview: None,
        };
        let json = serde_json::to_string(&summary)?;
        assert!(json.contains("\"readiness\":\"needs_tools\""));
        let decoded: CatalogRuntimeSummary = serde_json::from_str(&json)?;
        assert_eq!(decoded, summary);

        let dep_raw = r#"{"id":"ffmpeg","optional":false}"#;
        let dep: PluginDependency = serde_json::from_str(dep_raw)?;
        assert_eq!(dep.status, None);

        let dep_ready = PluginDependency {
            status: Some(ToolStatus::Ready),
            ..dep
        };
        let dep_json = serde_json::to_string(&dep_ready)?;
        assert!(dep_json.contains("\"status\":\"ready\""));
        Ok(())
    }
}
