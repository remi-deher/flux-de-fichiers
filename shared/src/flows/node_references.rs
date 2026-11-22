//! Variables d'un nœud script dans l'éditeur de flux : références proposées et configuration résolue (ADR 0025, phase D ; modèles `{$…}` de l'ADR 0026).

use crate::{
    flows::{
        CUSTOM_PREFIX, FlowGraph, IntrinsicVariable, is_known_expression, is_set_variable_kind,
    },
    script_variables::ScriptVariable,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// D'où vient une référence proposée dans le sélecteur.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReferenceSource {
    /// Variables intrinsèques et personnalisées (`file.path`, `job.id`, `vars.<nom>`…).
    Variables,
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
            Self::Variables => "Variables",
            Self::Flow => "Flux",
            Self::MediaInfo => "Métadonnées du fichier",
            Self::Node => "Nœuds précédents",
            Self::Plugin => "Plugins",
        }
    }
}

/// Expression `{$…}` utilisable dans la valeur d'une variable du nœud (`reference` sans délimiteurs).
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

/// Clé de configuration du nom écrit par un bloc « Définir une variable ».
const SET_VARIABLE_NAME_KEY: &str = "name";

/// Famille d'une variable intrinsèque dans le sélecteur : emplacements de fichiers, ou variables.
fn intrinsic_source(var: IntrinsicVariable) -> ReferenceSource {
    match var {
        IntrinsicVariable::FilePath
        | IntrinsicVariable::SourcePath
        | IntrinsicVariable::WorkDir => ReferenceSource::Flow,
        _ => ReferenceSource::Variables,
    }
}

/// Variables du flux proposées partout : intrinsèques, puis `vars.<nom>` des blocs « Définir une
/// variable » du graphe (sans doublon), sans valeur d'exemple.
pub fn flow_variable_suggestions(graph: &FlowGraph) -> Vec<ReferenceSuggestion> {
    let mut suggestions: Vec<ReferenceSuggestion> = IntrinsicVariable::ALL
        .into_iter()
        .map(|var| ReferenceSuggestion {
            reference: var.key().to_owned(),
            label: var.label().to_owned(),
            source: intrinsic_source(var),
            sample: None,
        })
        .collect();
    for node in graph
        .nodes
        .iter()
        .filter(|node| is_set_variable_kind(node.kind.as_str()))
    {
        let name = node
            .config
            .get(SET_VARIABLE_NAME_KEY)
            .and_then(Value::as_str)
            .unwrap_or_default();
        let reference = format!("{CUSTOM_PREFIX}{}", name.trim());
        if is_known_expression(&reference)
            && !suggestions.iter().any(|known| known.reference == reference)
        {
            suggestions.push(ReferenceSuggestion {
                reference,
                label: format!("Variable définie par « {} »", node.name),
                source: ReferenceSource::Variables,
                sample: None,
            });
        }
    }
    suggestions
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
    /// Avertissements sans effet sur l'exécution, par exemple une référence à une variable exposée dépréciée.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sources_have_distinct_labels_and_serialize_to_snake_case() -> Result<(), serde_json::Error> {
        let sources = [
            ReferenceSource::Variables,
            ReferenceSource::Flow,
            ReferenceSource::MediaInfo,
            ReferenceSource::Node,
            ReferenceSource::Plugin,
        ];
        let mut labels: Vec<&str> = sources.iter().map(|source| source.label()).collect();
        labels.sort_unstable();
        labels.dedup();
        assert_eq!(labels.len(), sources.len());
        assert_eq!(
            serde_json::to_string(&ReferenceSource::MediaInfo)?,
            "\"media_info\""
        );
        Ok(())
    }

    #[test]
    fn flow_suggestions_list_intrinsics_then_each_defined_variable_once()
    -> Result<(), serde_json::Error> {
        let graph: FlowGraph = serde_json::from_value(serde_json::json!({
            "nodes": [
                {"id": "a", "type": "set-variable", "name": "A", "config": {"name": "codec"}},
                {"id": "b", "type": "essentiel.set-variable", "name": "B", "config": {"name": "codec"}},
                {"id": "c", "type": "set-variable", "name": "C", "config": {"name": "nom invalide"}}
            ]
        }))?;
        let suggestions = flow_variable_suggestions(&graph);
        assert_eq!(suggestions.len(), IntrinsicVariable::ALL.len() + 1);
        let find = |reference: &str| suggestions.iter().find(|s| s.reference == reference);
        assert_eq!(
            find("file.path").map(|s| s.source),
            Some(ReferenceSource::Flow)
        );
        assert_eq!(
            find("job.id").map(|s| s.source),
            Some(ReferenceSource::Variables)
        );
        assert_eq!(
            find("vars.codec").map(|s| s.label.as_str()),
            Some("Variable définie par « A »")
        );
        Ok(())
    }

    #[test]
    fn a_request_without_media_info_deserializes_and_a_default_view_is_empty()
    -> Result<(), serde_json::Error> {
        let request: NodeVariablesRequest = serde_json::from_str(r#"{"graph":{},"nodeId":"n1"}"#)?;
        assert!(request.media_info.is_null());
        let view = NodeVariablesView::default();
        let json = serde_json::to_string(&view)?;
        assert!(
            !json.contains("resolved") && !json.contains("error") && !json.contains("warnings")
        );
        Ok(())
    }
}
