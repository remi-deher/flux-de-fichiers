//! Contrat des plugins : catalogue, dépôts, outils distribués et réglages (ADR 0014).

pub mod vocab;

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
