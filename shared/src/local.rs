//! Contrats de la librairie locale (scripts, fichiers ressources, modèles de blocs/flows, packs) sans dépôt GitHub.

use crate::plugins::vocab::{CatalogItemType, PluginCategory};
use crate::plugins::{CatalogEntry, PackComponent};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Interpréteur système supporté pour l'exécution agnostique de scripts (sans shell libre).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ScriptInterpreter {
    Bash,
    #[default]
    Sh,
    Python3,
    Node,
    Deno,
    Bun,
    Pwsh,
}

impl ScriptInterpreter {
    pub const ALL: &'static [Self] = &[
        Self::Sh,
        Self::Bash,
        Self::Python3,
        Self::Node,
        Self::Deno,
        Self::Bun,
        Self::Pwsh,
    ];

    pub const fn executable(self) -> &'static str {
        match self {
            Self::Bash => "bash",
            Self::Sh => "sh",
            Self::Python3 => "python3",
            Self::Node => "node",
            Self::Deno => "deno",
            Self::Bun => "bun",
            Self::Pwsh => "pwsh",
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Sh => "Shell (sh)",
            Self::Bash => "Bash",
            Self::Python3 => "Python 3",
            Self::Node => "Node.js",
            Self::Deno => "Deno",
            Self::Bun => "Bun",
            Self::Pwsh => "PowerShell",
        }
    }

    pub const fn default_extension(self) -> &'static str {
        match self {
            Self::Bash | Self::Sh => "sh",
            Self::Python3 => "py",
            Self::Node | Self::Deno | Self::Bun => "js",
            Self::Pwsh => "ps1",
        }
    }

    pub fn from_executable(raw: &str) -> Option<Self> {
        let normalized = raw.trim().to_ascii_lowercase();
        Self::ALL
            .iter()
            .copied()
            .find(|interp| interp.executable() == normalized)
    }
}

/// Script local écrit ou importé par l'administrateur.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalScriptSummary {
    pub id: String,
    pub slug: String,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    pub interpreter: ScriptInterpreter,
    pub entrypoint: String,
    pub code: String,
    #[serde(default)]
    pub category: Option<PluginCategory>,
    #[serde(default)]
    pub tags: Vec<String>,
}

/// Création ou mise à jour d'un script local (`POST /api/v1/local/scripts`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpsertLocalScriptRequest {
    #[serde(default)]
    pub id: Option<String>,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    pub interpreter: ScriptInterpreter,
    pub code: String,
    #[serde(default)]
    pub filename: Option<String>,
    #[serde(default)]
    pub category: Option<PluginCategory>,
    #[serde(default)]
    pub tags: Vec<String>,
}

/// Fichier ressource local (image, sous-titre, son, police, modèle…).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalResourceSummary {
    pub id: String,
    pub slug: String,
    pub name: String,
    pub entry_type: CatalogItemType,
    pub filename: String,
    pub size_bytes: u64,
    #[serde(default)]
    pub description: Option<String>,
}

/// Import d'un fichier ressource local (`POST /api/v1/local/resources`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UploadLocalResourceRequest {
    #[serde(default)]
    pub id: Option<String>,
    pub name: String,
    pub entry_type: CatalogItemType,
    pub filename: String,
    pub content_base64: String,
    #[serde(default)]
    pub description: Option<String>,
}

/// Modèle de blocs (sous-graphe réutilisable dans l'éditeur de flux).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalBlockTemplate {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    pub definition: Value,
    #[serde(default)]
    pub tags: Vec<String>,
    pub is_local: bool,
}

/// Création d'un modèle de blocs (`POST /api/v1/local/block-templates`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateBlockTemplateRequest {
    #[serde(default)]
    pub id: Option<String>,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    pub definition: Value,
    #[serde(default)]
    pub tags: Vec<String>,
}

/// Modèle de flow réutilisable (local ou issu d'un dépôt).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalFlowTemplate {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    pub file_mask: String,
    pub version: String,
    pub definition: Value,
    pub is_local: bool,
}

/// Création d'un modèle de flow (`POST /api/v1/local/flow-templates`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateFlowTemplateRequest {
    #[serde(default)]
    pub id: Option<String>,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub file_mask: Option<String>,
    pub definition: Value,
}

/// Assemblage d'un pack local (`POST /api/v1/local/packs`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateLocalPackRequest {
    #[serde(default)]
    pub id: Option<String>,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    pub components: Vec<PackComponent>,
    #[serde(default)]
    pub entrypoint: Option<PackComponent>,
}

/// Vue consolidée de la librairie locale et des modèles disponibles (`GET /api/v1/local`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalLibrarySnapshot {
    pub scripts: Vec<LocalScriptSummary>,
    pub resources: Vec<LocalResourceSummary>,
    pub block_templates: Vec<LocalBlockTemplate>,
    pub flow_templates: Vec<LocalFlowTemplate>,
    pub packs: Vec<CatalogEntry>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn script_interpreter_covers_all_variants_and_roundtrips() -> Result<(), serde_json::Error> {
        for &interp in ScriptInterpreter::ALL {
            assert_eq!(
                ScriptInterpreter::from_executable(interp.executable()),
                Some(interp)
            );
            assert_eq!(
                ScriptInterpreter::from_executable(&interp.executable().to_ascii_uppercase()),
                Some(interp)
            );
            assert!(!interp.label().is_empty());
            assert!(!interp.default_extension().is_empty());
            let json = serde_json::to_string(&interp)?;
            let parsed: ScriptInterpreter = serde_json::from_str(&json)?;
            assert_eq!(parsed, interp);
        }
        assert_eq!(ScriptInterpreter::from_executable("ruby"), None);
        Ok(())
    }

    #[test]
    fn upload_resource_request_defaults_optional_id_to_none() -> Result<(), serde_json::Error> {
        let parsed: UploadLocalResourceRequest = serde_json::from_str(
            r#"{
                "name": "FR",
                "entryType": "resource.subtitle",
                "filename": "fr.srt",
                "contentBase64": "QUI="
            }"#,
        )?;
        assert_eq!(parsed.id, None);
        Ok(())
    }
}
