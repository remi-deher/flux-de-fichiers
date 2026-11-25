//! Store et éléments installés (ADR 0029) : origine, état d'installation, fiches.

pub mod plan;
pub mod usage;

use super::{
    CatalogEntry, PackDetailView, PluginDetailView, ToolDetailView, ToolStatus, ToolStatusSummary,
};
use crate::highlight::ScriptPreviewView;
use serde::{Deserialize, Serialize};

/// Dossier de plugins détecté à côté du projet. `@` n'existe pas dans un identifiant de dépôt.
pub const DEV_SOURCE_ID: &str = "@dev";
/// Collection locale de l'utilisateur.
pub const LOCAL_SOURCE_ID: &str = "@local";

/// Un élément d'une source du Store (identifiant de dépôt, `@dev` ou `@local`).
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemRef {
    pub repository_id: String,
    pub id: String,
}

/// Dépôt d'origine d'un élément ou de ses réglages.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemOrigin {
    pub repository_id: String,
    pub repository_url: String,
    /// Pour l'affichage : reste lisible après le retrait du dépôt.
    pub repository_name: String,
}

impl ItemOrigin {
    /// Même identifiant et même adresse : un dépôt rajouté ailleurs sous le même nom est un autre dépôt.
    pub fn is_same_as(&self, other: &Self) -> bool {
        self.repository_id == other.repository_id && self.repository_url == other.repository_url
    }
}

/// Mise à jour publiée par le dépôt d'origine.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AvailableUpdate {
    pub version: String,
    /// Même version, contenu différent.
    #[serde(default)]
    pub content_only: bool,
}

/// Ce que le Store peut faire d'un élément.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "state",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum InstallState {
    #[default]
    Available,
    Installed {
        version: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        update: Option<AvailableUpdate>,
        /// « À réparer » : son dépôt peut redonner la copie que le moteur ne lit plus.
        #[serde(default)]
        damaged: bool,
    },
    /// Déjà lu par le moteur depuis `PLUGINS_DIR` ou la collection locale.
    Provided,
    /// Le même identifiant est installé depuis un autre dépôt.
    HeldByOther { origin: ItemOrigin },
    /// La copie de vitrine n'a pas tous les fichiers que le moteur lit.
    Incomplete { missing: Vec<String> },
    /// Pack dont une partie des composants reste à installer.
    PartlyInstalled { remaining: usize },
}

impl InstallState {
    /// Un plan d'installation ou de mise à jour a-t-il un effet ?
    pub fn is_actionable(&self) -> bool {
        match self {
            Self::Available | Self::PartlyInstalled { .. } => true,
            Self::Installed {
                update, damaged, ..
            } => update.is_some() || *damaged,
            Self::Provided | Self::HeldByOther { .. } | Self::Incomplete { .. } => false,
        }
    }
}

/// Élément du Store, adressé par (dépôt, identifiant) : les homonymes sont tous montrés.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoreEntry {
    pub origin: ItemOrigin,
    #[serde(default)]
    pub install: InstallState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_status: Option<ToolStatus>,
    #[serde(flatten)]
    pub entry: CatalogEntry,
}

/// Élément installé.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledItem {
    pub origin: ItemOrigin,
    /// Le dépôt d'origine est-il encore enregistré, activé et synchronisé ?
    #[serde(default)]
    pub origin_available: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub update: Option<AvailableUpdate>,
    /// Copie figée absente ou altérée : « à réparer ».
    #[serde(default)]
    pub damaged: bool,
    /// Fichiers que le moteur lit et que le dépôt n'a jamais fournis.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub missing: Vec<String>,
    pub installed_at: String,
    /// Flux créé, ou script de la collection.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub copy_id: Option<String>,
    /// Script : la copie diffère-t-elle de sa copie figée ?
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub copy_modified: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool: Option<ToolStatusSummary>,
    #[serde(flatten)]
    pub entry: CatalogEntry,
}

/// Fiche d'un élément, dans le Store comme dans les éléments installés.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemDetail {
    pub entry: StoreEntry,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plugin: Option<PluginDetailView>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pack: Option<PackDetailView>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool: Option<ToolDetailView>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub script: Option<ScriptPreviewView>,
    /// Renseigné pour un élément installé.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<usage::ItemUsage>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn origin(id: &str, url: &str) -> ItemOrigin {
        ItemOrigin {
            repository_id: id.to_owned(),
            repository_url: url.to_owned(),
            repository_name: format!("Dépôt {id}"),
        }
    }

    #[test]
    fn an_origin_is_an_identifier_and_an_address() {
        let known = origin("repo.a", "https://a.example/index.json");
        let renamed = ItemOrigin {
            repository_name: "Autre nom".to_owned(),
            ..known.clone()
        };
        assert!(known.is_same_as(&renamed));
        assert!(!known.is_same_as(&origin("repo.a", "https://b.example/index.json")));
        assert!(!known.is_same_as(&origin("repo.b", "https://a.example/index.json")));
    }

    #[test]
    fn only_useful_plans_are_offered() {
        let installed = |update, damaged| InstallState::Installed {
            version: "1.0.0".to_owned(),
            update,
            damaged,
        };
        let update = AvailableUpdate {
            version: "1.0.0".to_owned(),
            content_only: true,
        };
        assert!(InstallState::Available.is_actionable());
        assert!(InstallState::PartlyInstalled { remaining: 3 }.is_actionable());
        assert!(installed(Some(update), false).is_actionable());
        assert!(installed(None, true).is_actionable());
        assert!(!installed(None, false).is_actionable());
        assert!(!InstallState::Provided.is_actionable());
    }

    #[test]
    fn a_store_entry_stays_readable_as_a_catalog_entry() -> Result<(), serde_json::Error> {
        let raw = r#"{"id":"video","type":"plugin","version":"1.2.0","name":"Vidéo","manifestPath":"plugins/video/manifest.json"}"#;
        let entry = StoreEntry {
            origin: origin("repo.official", "https://a.example/index.json"),
            install: InstallState::HeldByOther {
                origin: origin("repo.b", "https://b.example/index.json"),
            },
            tool_status: None,
            entry: serde_json::from_str(raw)?,
        };
        let json = serde_json::to_string(&entry)?;
        assert!(json.contains(r#""state":"held_by_other""#), "{json}");
        assert!(json.contains(r#""repositoryUrl""#), "{json}");
        assert_eq!(serde_json::from_str::<StoreEntry>(&json)?, entry);
        assert_eq!(
            serde_json::from_str::<CatalogEntry>(&json)?.id,
            entry.entry.id
        );
        Ok(())
    }
}
