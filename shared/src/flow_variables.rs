//! Variables d'un nœud script dans l'éditeur de flux : références proposées et configuration résolue (ADR 0025, phase D).

use crate::{flows::FlowGraph, script_variables::ScriptVariable};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// D'où vient une référence proposée dans le sélecteur.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReferenceSource {
    /// Fichier courant, fichier source, dossier de travail.
    Flow,
    /// Champ des métadonnées média du fichier.
    MediaInfo,
    /// Sortie d'un nœud en amont.
    Node,
    /// Variable exposée par un plugin.
    Plugin,
}

impl ReferenceSource {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Flow => "Flux",
            Self::MediaInfo => "Métadonnées du fichier",
            Self::Node => "Nœuds précédents",
            Self::Plugin => "Plugins",
        }
    }
}

/// Référence `{{…}}` utilisable dans la valeur d'une variable du nœud.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReferenceSuggestion {
    pub reference: String,
    pub label: String,
    pub source: ReferenceSource,
    /// Valeur d'exemple, quand elle est connue avant l'exécution.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sample: Option<String>,
}

/// Demande des variables d'un nœud : le graphe courant (même non enregistré) et un exemple de métadonnées.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeVariablesRequest {
    pub graph: FlowGraph,
    pub node_id: String,
    #[serde(default)]
    pub media_info: Value,
}

/// Schéma du script d'un nœud, références disponibles à cet endroit et configuration résolue d'exemple.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeVariablesView {
    #[serde(default)]
    pub variables: Vec<ScriptVariable>,
    #[serde(default)]
    pub references: Vec<ReferenceSuggestion>,
    /// `config` tel que le script le recevrait avec les valeurs d'exemple ; absent si la résolution échoue.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolved: Option<Value>,
    /// Pourquoi la résolution échoue (référence sans valeur, variable requise, valeur hors bornes…).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sources_have_distinct_labels_and_serialize_to_snake_case() -> Result<(), serde_json::Error> {
        let sources = [
            ReferenceSource::Flow,
            ReferenceSource::MediaInfo,
            ReferenceSource::Node,
            ReferenceSource::Plugin,
        ];
        let mut labels: Vec<&str> = sources.iter().map(|source| source.label()).collect();
        labels.sort_unstable();
        labels.dedup();
        assert_eq!(labels.len(), sources.len());
        assert_eq!(serde_json::to_string(&ReferenceSource::MediaInfo)?, "\"media_info\"");
        Ok(())
    }

    #[test]
    fn a_request_without_media_info_deserializes_and_a_default_view_is_empty() -> Result<(), serde_json::Error> {
        let request: NodeVariablesRequest = serde_json::from_str(r#"{"graph":{},"nodeId":"n1"}"#)?;
        assert!(request.media_info.is_null());
        let view = NodeVariablesView::default();
        let json = serde_json::to_string(&view)?;
        assert!(!json.contains("resolved") && !json.contains("error"));
        Ok(())
    }
}
