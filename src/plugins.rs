//! Gestionnaire de plugins, chargement du catalogue et inspection des manifestes.

use crate::tools_cache::{ToolCacheManager, host_arch};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Fiche d'index dans `catalogue/index.json`.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CatalogEntry {
    pub id: String,
    #[serde(rename = "type")]
    pub entry_type: String,
    pub version: String,
    pub name: String,
    pub manifest_path: String,
    #[serde(default)]
    pub capability: Option<String>,
    #[serde(default)]
    pub current_choice: Option<bool>,
    #[serde(default)]
    pub distribution: Option<String>,
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
    pub entry_type: String,
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
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ManifestToolDetails {
    #[serde(default)]
    pub capability: Option<String>,
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
    pub status: String, // "ready" | "locked" | "not_installed"
    pub capability: String,
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
pub fn list_catalog(plugins_root: Option<&Path>) -> Vec<CatalogEntry> {
    if let Some(root) = plugins_root
        && let Ok(catalog) = load_catalog(root)
    {
        return catalog.entries;
    }
    default_catalog_entries()
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
        .filter(|e| e.entry_type == "tool")
        .map(|entry| {
            let mut description = String::new();
            if let Some(root) = plugins_root
                && let Ok(m) = load_manifest(root, &entry.manifest_path)
                && let Some(d) = m.description
            {
                description = d;
            }
            if description.is_empty() {
                description = default_tool_description(&entry.id);
            }

            let status = if entry.id == "tool.internal.telemetry"
                || cache_mgr.is_ready(&entry.id, &entry.version, arch)
            {
                "ready".to_string()
            } else {
                "not_installed".to_string()
            };

            ToolStatusSummary {
                id: entry.id,
                name: entry.name,
                version: entry.version,
                status,
                capability: entry.capability.unwrap_or_else(|| "other".into()),
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
        "tool.mkvtoolnix" => "Outil de remuxage sans réencodage validé. Distribution Option B (binaire autonome hors conteneur applicatif). Matroska uniquement.".into(),
        "tool.mp4box" => "Outil de référence pour le multiplexage, l'optimisation streaming faststart et la manipulation de conteneurs MP4 sans réencodage (GPAC).".into(),
        "tool.mediainfo" => "Outil universel d'analyse et d'extraction de métadonnées techniques multimédias (codecs, résolutions, profils HDR10/Dolby Vision, pistes audio et sous-titres).".into(),
        "tool.ffmpeg" => "Outil d'analyse multimédia et génération de médias synthétiques de tests. Réservé aux tests et scénarios de simulation.".into(),
        "tool.internal.telemetry" => "Mesure périodique CPU, RAM, GPU (/dev/dri) et stockage local. Confinement strict sous UID 10001 (app) sans socket Docker.".into(),
        "tool.postgresql" => "Moteur de base de données relationnelle historique (remplacé par SQLite local /config dans l'ADR 0005).".into(),
        _ => "Outil modulaire distribué.".into(),
    }
}

fn default_catalog_entries() -> Vec<CatalogEntry> {
    vec![
        CatalogEntry {
            id: "tool.mkvtoolnix".into(),
            entry_type: "tool".into(),
            version: "1.0.0".into(),
            name: "MKVToolNix (mkvmerge)".into(),
            manifest_path: "tools/mkvtoolnix/manifest.json".into(),
            capability: Some("remux".into()),
            current_choice: Some(true),
            distribution: Some("autonomous-binary".into()),
        },
        CatalogEntry {
            id: "tool.ffmpeg".into(),
            entry_type: "tool".into(),
            version: "1.0.0".into(),
            name: "FFmpeg (distribué)".into(),
            manifest_path: "tools/ffmpeg/manifest.json".into(),
            capability: Some("analyze".into()),
            current_choice: Some(false),
            distribution: Some("autonomous-binary".into()),
        },
        CatalogEntry {
            id: "tool.internal.telemetry".into(),
            entry_type: "tool".into(),
            version: "0.1.0".into(),
            name: "Sonde interne de télémétrie".into(),
            manifest_path: "tools/internal-telemetry/manifest.json".into(),
            capability: Some("other".into()),
            current_choice: Some(true),
            distribution: Some("autonomous-binary".into()),
        },
        CatalogEntry {
            id: "macro.remux-verify.example".into(),
            entry_type: "block".into(),
            version: "0.1.0".into(),
            name: "Macro remux + vérifier (exemple)".into(),
            manifest_path: "blocks/macro-remux-verify.example/manifest.json".into(),
            capability: None,
            current_choice: None,
            distribution: None,
        },
    ]
}
