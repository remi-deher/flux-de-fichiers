//! Plan d'installation ou de mise à jour, puis son application (ADR 0029).

use super::{ItemOrigin, ItemRef, usage::name_some};
use crate::{
    Permission, exposed_variables::RepositoryPointer, plugins::ToolStatus,
    plugins::vocab::CatalogItemType,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanAction {
    Install,
    Update,
    AlreadyInstalled,
}

/// Pourquoi un élément est dans le plan.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanReason {
    Target,
    /// Composant d'un pack visé.
    Component,
    Required,
    /// Sonde primaire ou opération par défaut, dont toute tâche a besoin.
    Baseline,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanItem {
    pub origin: ItemOrigin,
    pub id: String,
    pub kind: CatalogItemType,
    pub name: String,
    pub version: String,
    pub action: PlanAction,
    pub reason: PlanReason,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub required_by: Vec<String>,
    #[serde(default)]
    pub has_settings: bool,
    #[serde(default)]
    pub configured: bool,
}

/// Outil recommandé. Son contrat est toujours figé ; la case ne décide que du téléchargement.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanTool {
    pub origin: ItemOrigin,
    pub id: String,
    pub name: String,
    pub version: String,
    pub status: ToolStatus,
    pub installable: bool,
    pub preselected: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub required_by: Vec<String>,
}

/// Nom unique dans le moteur.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClaimKind {
    EntryId,
    BlockKind,
    Operation,
}

impl ClaimKind {
    pub const fn label(self) -> &'static str {
        match self {
            Self::EntryId => "l'identifiant",
            Self::BlockKind => "le type de bloc",
            Self::Operation => "l'opération",
        }
    }
}

/// Ce qui interdit d'appliquer le plan.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "code",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum PlanBlocker {
    HeldByOther {
        id: String,
        origin: ItemOrigin,
    },
    ClaimConflict {
        id: String,
        claim: ClaimKind,
        value: String,
        holder: String,
    },
    /// Opération déclarée au nom d'un autre plugin.
    ForeignProvider {
        id: String,
        operation: String,
        provider: String,
    },
    Missing {
        id: String,
        required_by: String,
    },
    VersionTooOld {
        id: String,
        available: String,
        required: String,
    },
    /// La mise à jour rompt des éléments installés.
    BreaksDependents {
        id: String,
        dependents: Vec<String>,
    },
    /// La mise à jour retire un bloc ou une opération dont se servent des tâches non terminées.
    InUseByJobs {
        id: String,
        removed: String,
        jobs: Vec<String>,
    },
    /// Le dépôt ne fournit pas un fichier que le moteur lit.
    Incomplete {
        id: String,
        files: Vec<String>,
    },
    InvalidItem {
        id: String,
        message: String,
    },
    /// Script modifié depuis son installation : il n'est pas remplacé.
    ModifiedCopy {
        id: String,
    },
    MissingPermission {
        permission: Permission,
    },
}

impl PlanBlocker {
    pub fn message(&self) -> String {
        match self {
            Self::HeldByOther { id, origin } => format!(
                "« {id} » est déjà installé depuis {}",
                origin.repository_name
            ),
            Self::ClaimConflict {
                id,
                claim,
                value,
                holder,
            } => format!(
                "« {id} » déclare {} « {value} », déjà fourni par « {holder} »",
                claim.label()
            ),
            Self::ForeignProvider {
                id,
                operation,
                provider,
            } => format!("« {id} » déclare l'opération « {operation} » au nom de « {provider} »"),
            Self::Missing { id, required_by } => {
                format!("« {id} », requis par « {required_by} », n'est dans aucun dépôt")
            }
            Self::VersionTooOld {
                id,
                available,
                required,
            } => format!("« {id} » est en version {available}, {required} est exigée"),
            Self::BreaksDependents { id, dependents } => format!(
                "la mise à jour de « {id} » ne convient plus à {}",
                name_some(dependents)
            ),
            Self::InUseByJobs { id, removed, jobs } => format!(
                "la mise à jour de « {id} » retire « {removed} », dont se servent des tâches : {}",
                name_some(jobs)
            ),
            Self::Incomplete { id, files } => format!(
                "« {id} » est incomplet dans son dépôt : {}",
                name_some(files)
            ),
            Self::InvalidItem { id, message } => format!("« {id} » est invalide : {message}"),
            Self::ModifiedCopy { id } => {
                format!("le script « {id} » a été modifié : dupliquez-le puis réinstallez-le")
            }
            Self::MissingPermission { permission } => {
                format!("droit manquant : {}", permission.label())
            }
        }
    }
}

/// Ce que l'utilisateur doit accepter explicitement pour appliquer le plan.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "code",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum PlanConsent {
    /// Les réglages conservés viennent d'un autre dépôt : ils seront effacés.
    DiscardSettings {
        id: String,
    },
    /// Le plugin lit les réglages, secrets compris, d'un autre.
    ReadsSettings {
        id: String,
        plugin: String,
    },
    ReplacesPrimaryProbe {
        id: String,
        current: String,
    },
    ReplacesDefaultOperation {
        id: String,
        current: String,
    },
    /// La mise à jour retire un bloc ou une opération dont se servent des flux.
    Removes {
        id: String,
        removed: String,
        flows: Vec<String>,
    },
    /// Le dépôt a écarté des opérations : l'élément est installé sans elles.
    PartialInstall {
        id: String,
        files: Vec<String>,
    },
}

impl PlanConsent {
    pub fn message(&self) -> String {
        match self {
            Self::DiscardSettings { id } => format!(
                "les réglages conservés de « {id} » viennent d'un autre dépôt et seront effacés"
            ),
            Self::ReadsSettings { id, plugin } => {
                format!("« {id} » lit les réglages et les secrets de « {plugin} »")
            }
            Self::ReplacesPrimaryProbe { id, current } => {
                format!("« {id} » prend la place de « {current} » comme sonde de toutes les tâches")
            }
            Self::ReplacesDefaultOperation { id, current } => {
                format!("« {id} » prend la place de « {current} » comme opération par défaut")
            }
            Self::Removes { id, removed, flows } => format!(
                "la mise à jour de « {id} » retire « {removed} », dont se servent : {}",
                name_some(flows)
            ),
            Self::PartialInstall { id, files } => {
                format!("« {id} » sera installé sans : {}", name_some(files))
            }
        }
    }
}

/// Information sans effet sur l'application.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "code",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum PlanWarning {
    /// Dépendance satisfaite par l'élément installé depuis un autre dépôt.
    OtherOrigin {
        id: String,
        origin: ItemOrigin,
    },
    OptionalMissing {
        id: String,
        required_by: String,
    },
    /// Espace de variables déjà exposé par un autre plugin (permis, ADR 0025).
    SharedNamespace {
        id: String,
        namespace: String,
        holder: String,
    },
    /// Fichiers annexes absents du dépôt.
    SkippedFiles {
        id: String,
        files: Vec<String>,
    },
}

impl PlanWarning {
    pub fn message(&self) -> String {
        match self {
            Self::OtherOrigin { id, origin } => format!(
                "« {id} » est déjà installé depuis {}",
                origin.repository_name
            ),
            Self::OptionalMissing { id, required_by } => {
                format!("« {id} », facultatif pour « {required_by} », n'est dans aucun dépôt")
            }
            Self::SharedNamespace {
                id,
                namespace,
                holder,
            } => format!("« {id} » expose « {namespace} », comme « {holder} »"),
            Self::SkippedFiles { id, files } => {
                format!("« {id} » : fichiers non copiés, {}", name_some(files))
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallPlan {
    /// Empreinte du plan : l'application est refusée s'il a changé.
    pub plan_id: String,
    #[serde(default)]
    pub items: Vec<PlanItem>,
    #[serde(default)]
    pub tools: Vec<PlanTool>,
    #[serde(default)]
    pub blockers: Vec<PlanBlocker>,
    #[serde(default)]
    pub consents: Vec<PlanConsent>,
    #[serde(default)]
    pub warnings: Vec<PlanWarning>,
    #[serde(default)]
    pub suggested_repositories: Vec<RepositoryPointer>,
}

/// Plusieurs cibles : composants choisis d'un pack, « tout mettre à jour ».
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanRequest {
    pub targets: Vec<ItemRef>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallRequest {
    pub targets: Vec<ItemRef>,
    pub plan_id: String,
    /// Accords donnés ; tous ceux du plan recalculé sont exigés.
    #[serde(default)]
    pub consents: Vec<PlanConsent>,
    /// Outils à télécharger une fois les éléments installés.
    #[serde(default)]
    pub tools: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledRef {
    pub id: String,
    pub kind: CatalogItemType,
    pub name: String,
    pub version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub copy_id: Option<String>,
}

/// Où mène « Utiliser » après l'installation.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum UseTarget {
    Flow {
        id: String,
    },
    Script {
        id: String,
    },
    Plugin {
        id: String,
    },
    #[default]
    None,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallOutcome {
    #[serde(default)]
    pub installed: Vec<InstalledRef>,
    #[serde(default)]
    pub use_target: UseTarget,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blockers_and_consents_say_what_they_are_about() -> Result<(), serde_json::Error> {
        let blocker = PlanBlocker::ClaimConflict {
            id: "tiers".to_owned(),
            claim: ClaimKind::Operation,
            value: "operation.core.remux".to_owned(),
            holder: "video".to_owned(),
        };
        assert_eq!(
            blocker.message(),
            "« tiers » déclare l'opération « operation.core.remux », déjà fourni par « video »"
        );
        let json = serde_json::to_string(&blocker)?;
        assert!(json.contains(r#""code":"claim_conflict""#), "{json}");
        assert_eq!(serde_json::from_str::<PlanBlocker>(&json)?, blocker);

        let consent = PlanConsent::ReadsSettings {
            id: "tiers".to_owned(),
            plugin: "sonarr".to_owned(),
        };
        assert_eq!(
            consent.message(),
            "« tiers » lit les réglages et les secrets de « sonarr »"
        );
        Ok(())
    }

    #[test]
    fn a_request_without_options_accepts_nothing_and_downloads_nothing()
    -> Result<(), serde_json::Error> {
        let raw = r#"{"targets":[{"repositoryId":"repo.official","id":"video"}],"planId":"abc"}"#;
        let request: InstallRequest = serde_json::from_str(raw)?;
        assert_eq!(request.targets[0].id, "video");
        assert!(request.consents.is_empty() && request.tools.is_empty());
        let outcome: InstallOutcome = serde_json::from_str(r#"{"installed":[]}"#)?;
        assert_eq!(outcome.use_target, UseTarget::None);
        Ok(())
    }
}
