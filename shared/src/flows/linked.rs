//! Flux liés à un flux : flux d'échec et sous-flux (ADR 0022).
//!
//! Le flux d'échec est désigné par la clé `failureFlowId` du graphe (clé optionnelle, absente
//! des définitions déjà stockées). Un sous-flux est appelé par le bloc primitif « Aller au
//! sous-flux », dont la config `flowId` désigne le flux appelé. Les versions de ces flux sont
//! figées à la création de la tâche, comme celle du flux principal (ADR 0012).

use super::{FlowGraph, FlowNode, FlowRecord};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fmt;

/// Clé du graphe qui désigne le flux d'échec.
pub const FAILURE_FLOW_KEY: &str = "failureFlowId";
/// Identifiant du bloc primitif « Aller au sous-flux » (suffixe des types de bloc associés).
pub const SUB_FLOW_BLOCK: &str = "sub-flow";
/// Clé de configuration du bloc qui désigne le flux appelé.
pub const SUB_FLOW_CONFIG_KEY: &str = "flowId";
/// Profondeur maximale d'appels imbriqués de sous-flux (le flux principal est au niveau 0).
pub const MAX_SUB_FLOW_DEPTH: usize = 4;
/// Séparateur entre le type de bloc et son espace de noms (`essentiel.sub-flow`).
const KIND_NAMESPACE_SEPARATOR: char = '.';

/// Rôle d'un flux lié figé avec une tâche.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LinkedFlowRole {
    /// Exécuté sur la tâche quand elle échoue.
    Failure,
    /// Appelé par un bloc « Aller au sous-flux ».
    SubFlow,
}

impl LinkedFlowRole {
    pub const ALL: [Self; 2] = [Self::Failure, Self::SubFlow];

    /// Valeur stockée en base (`job_linked_flows.role`).
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Failure => "failure",
            Self::SubFlow => "sub_flow",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|role| role.as_str() == value)
    }
}

/// Version figée d'un flux lié, transmise au worker avec le bail.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LinkedFlow {
    pub role: LinkedFlowRole,
    pub flow: FlowRecord,
}

/// Flux d'échec global (`GET` / `PUT /api/v1/flows/failure-default`) ; `None` : aucun.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DefaultFailureFlow {
    #[serde(default)]
    pub flow_id: Option<String>,
}

/// Cherche le flux lié `flow_id` de rôle `role`.
pub fn find_linked<'a>(
    linked: &'a [LinkedFlow],
    role: LinkedFlowRole,
    flow_id: &str,
) -> Option<&'a FlowRecord> {
    linked
        .iter()
        .find(|entry| entry.role == role && entry.flow.id == flow_id)
        .map(|entry| &entry.flow)
}

/// Premier flux lié de rôle `role` (le flux d'échec est unique).
pub fn first_linked(linked: &[LinkedFlow], role: LinkedFlowRole) -> Option<&FlowRecord> {
    linked
        .iter()
        .find(|entry| entry.role == role)
        .map(|entry| &entry.flow)
}

fn non_empty_text(value: Option<&Value>) -> Option<&str> {
    value
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
}

/// Vrai si `kind` désigne le bloc « Aller au sous-flux » (`sub-flow` ou `<espace>.sub-flow`).
pub fn is_sub_flow_kind(kind: &str) -> bool {
    match kind.strip_suffix(SUB_FLOW_BLOCK) {
        Some(prefix) => prefix.is_empty() || prefix.ends_with(KIND_NAMESPACE_SEPARATOR),
        None => false,
    }
}

/// Flux appelé par un bloc « Aller au sous-flux » ; `None` pour un autre bloc ou sans cible.
pub fn sub_flow_target(node: &FlowNode) -> Option<&str> {
    if !is_sub_flow_kind(node.kind.as_str()) {
        return None;
    }
    non_empty_text(node.config.get(SUB_FLOW_CONFIG_KEY))
}

impl FlowGraph {
    /// Flux d'échec désigné par ce flux, s'il y en a un.
    pub fn failure_flow_id(&self) -> Option<&str> {
        non_empty_text(self.extra.get(FAILURE_FLOW_KEY))
    }

    /// Désigne (ou retire, avec `None` ou un texte vide) le flux d'échec.
    pub fn set_failure_flow_id(&mut self, flow_id: Option<&str>) {
        match flow_id.map(str::trim).filter(|id| !id.is_empty()) {
            Some(id) => {
                self.extra
                    .insert(FAILURE_FLOW_KEY.to_owned(), Value::String(id.to_owned()));
            }
            None => {
                self.extra.remove(FAILURE_FLOW_KEY);
            }
        }
    }

    /// Flux appelés par les blocs « Aller au sous-flux », sans doublon, dans l'ordre des blocs.
    pub fn sub_flow_ids(&self) -> Vec<String> {
        let mut ids: Vec<String> = Vec::new();
        for target in self.nodes.iter().filter_map(sub_flow_target) {
            if !ids.iter().any(|known| known == target) {
                ids.push(target.to_owned());
            }
        }
        ids
    }

    /// Blocs « Aller au sous-flux » sans flux désigné.
    pub fn sub_flow_nodes_without_target(&self) -> Vec<&str> {
        self.nodes
            .iter()
            .filter(|node| is_sub_flow_kind(node.kind.as_str()) && sub_flow_target(node).is_none())
            .map(|node| node.id.as_str())
            .collect()
    }
}

/// Refus d'un enchaînement de sous-flux.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LinkError {
    /// Chaîne d'appels qui revient sur un flux déjà en cours.
    Cycle(Vec<String>),
    /// Chaîne d'appels plus profonde que `MAX_SUB_FLOW_DEPTH`.
    TooDeep(Vec<String>),
    /// Flux appelé introuvable.
    Missing(String),
    /// Bloc « Aller au sous-flux » sans flux désigné.
    MissingTarget(String),
}

impl fmt::Display for LinkError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Cycle(path) => write!(f, "appel circulaire de sous-flux : {}", path.join(" → ")),
            Self::TooDeep(path) => write!(
                f,
                "sous-flux imbriqués au-delà de {MAX_SUB_FLOW_DEPTH} niveaux : {}",
                path.join(" → ")
            ),
            Self::Missing(id) => write!(f, "sous-flux introuvable : {id}"),
            Self::MissingTarget(node) => write!(f, "le bloc {node} n'indique aucun sous-flux"),
        }
    }
}

impl std::error::Error for LinkError {}

/// Vérifie les appels de sous-flux atteignables depuis `root` : flux présents, sans cycle et
/// sans dépasser `MAX_SUB_FLOW_DEPTH`. `load` renvoie le graphe actuel d'un autre flux.
pub fn check_sub_flow_links(
    root_id: &str,
    root: &FlowGraph,
    load: &dyn Fn(&str) -> Option<FlowGraph>,
) -> Result<(), LinkError> {
    if let Some(node) = root.sub_flow_nodes_without_target().first() {
        return Err(LinkError::MissingTarget((*node).to_owned()));
    }
    let mut path = vec![root_id.to_owned()];
    visit(root, &mut path, load)
}

fn visit(
    graph: &FlowGraph,
    path: &mut Vec<String>,
    load: &dyn Fn(&str) -> Option<FlowGraph>,
) -> Result<(), LinkError> {
    for target in graph.sub_flow_ids() {
        let mut chain = path.clone();
        chain.push(target.clone());
        if path.contains(&target) {
            return Err(LinkError::Cycle(chain));
        }
        if path.len() > MAX_SUB_FLOW_DEPTH {
            return Err(LinkError::TooDeep(chain));
        }
        let child = load(&target).ok_or_else(|| LinkError::Missing(target.clone()))?;
        path.push(target);
        visit(&child, path, load)?;
        path.pop();
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::collections::HashMap;

    fn graph(value: Value) -> FlowGraph {
        serde_json::from_value(value).unwrap_or_default()
    }

    fn calling(targets: &[&str]) -> FlowGraph {
        let nodes: Vec<Value> = targets
            .iter()
            .enumerate()
            .map(|(index, target)| {
                json!({"id": format!("s{index}"), "type": "essentiel.sub-flow", "name": "s",
                       "config": {"flowId": target}})
            })
            .collect();
        graph(json!({"nodes": nodes}))
    }

    fn library(entries: &[(&str, &[&str])]) -> HashMap<String, FlowGraph> {
        entries
            .iter()
            .map(|(id, targets)| ((*id).to_owned(), calling(targets)))
            .collect()
    }

    fn check(root: &str, flows: &HashMap<String, FlowGraph>) -> Result<(), LinkError> {
        let root_graph = flows.get(root).cloned().unwrap_or_default();
        check_sub_flow_links(root, &root_graph, &|id| flows.get(id).cloned())
    }

    #[test]
    fn recognises_sub_flow_kinds() {
        assert!(is_sub_flow_kind("sub-flow"));
        assert!(is_sub_flow_kind("essentiel.sub-flow"));
        assert!(!is_sub_flow_kind("my-sub-flow"));
        assert!(!is_sub_flow_kind("condition"));
    }

    #[test]
    fn failure_flow_key_is_optional_and_roundtrips() -> Result<(), serde_json::Error> {
        let mut stored = graph(json!({"nodes": []}));
        assert_eq!(stored.failure_flow_id(), None);
        stored.set_failure_flow_id(Some(" flow.echec "));
        assert_eq!(stored.failure_flow_id(), Some("flow.echec"));
        let encoded = serde_json::to_value(&stored)?;
        assert_eq!(encoded[FAILURE_FLOW_KEY], "flow.echec");
        stored.set_failure_flow_id(Some(""));
        assert_eq!(stored.failure_flow_id(), None);
        assert!(serde_json::to_value(&stored)?.get(FAILURE_FLOW_KEY).is_none());
        Ok(())
    }

    #[test]
    fn lists_sub_flow_targets_without_duplicates() {
        let flow = calling(&["a", "b", "a"]);
        assert_eq!(flow.sub_flow_ids(), vec!["a".to_owned(), "b".to_owned()]);
        let empty = graph(json!({"nodes": [{"id": "x", "type": "sub-flow", "name": "x"}]}));
        assert!(empty.sub_flow_ids().is_empty());
        assert_eq!(empty.sub_flow_nodes_without_target(), vec!["x"]);
    }

    #[test]
    fn accepts_a_shared_sub_flow_and_refuses_cycles() {
        let ok = library(&[
            ("root", &["a", "b"]),
            ("a", &["c"]),
            ("b", &["c"]),
            ("c", &[]),
        ]);
        assert_eq!(check("root", &ok), Ok(()));

        let direct = library(&[("root", &["root"])]);
        assert!(matches!(check("root", &direct), Err(LinkError::Cycle(_))));

        let indirect = library(&[("root", &["a"]), ("a", &["b"]), ("b", &["root"])]);
        let error = check("root", &indirect);
        assert!(matches!(error, Err(LinkError::Cycle(path)) if path.join(">") == "root>a>b>root"));
    }

    #[test]
    fn refuses_missing_flows_targets_and_excessive_depth() {
        let missing = library(&[("root", &["absent"])]);
        assert_eq!(
            check("root", &missing),
            Err(LinkError::Missing("absent".into()))
        );

        let no_target = graph(json!({"nodes": [{"id": "n", "type": "sub-flow", "name": "n"}]}));
        assert_eq!(
            check_sub_flow_links("root", &no_target, &|_| None),
            Err(LinkError::MissingTarget("n".into()))
        );

        let at_limit = library(&[
            ("root", &["f1"]),
            ("f1", &["f2"]),
            ("f2", &["f3"]),
            ("f3", &["f4"]),
            ("f4", &[]),
        ]);
        assert_eq!(check("root", &at_limit), Ok(()));
        let too_deep = library(&[
            ("root", &["f1"]),
            ("f1", &["f2"]),
            ("f2", &["f3"]),
            ("f3", &["f4"]),
            ("f4", &["f5"]),
            ("f5", &[]),
        ]);
        assert!(matches!(check("root", &too_deep), Err(LinkError::TooDeep(_))));
    }

    #[test]
    fn roles_roundtrip_and_lookup() {
        for role in LinkedFlowRole::ALL {
            assert_eq!(LinkedFlowRole::parse(role.as_str()), Some(role));
        }
        assert_eq!(LinkedFlowRole::parse("autre"), None);
        let record = FlowRecord {
            id: "flow.a".into(),
            name: "A".into(),
            description: None,
            file_mask: "*".into(),
            is_default: false,
            definition: "{}".into(),
            version: 2,
            created_at: chrono::DateTime::UNIX_EPOCH,
            updated_at: chrono::DateTime::UNIX_EPOCH,
        };
        let linked = vec![LinkedFlow {
            role: LinkedFlowRole::SubFlow,
            flow: record,
        }];
        assert!(find_linked(&linked, LinkedFlowRole::SubFlow, "flow.a").is_some());
        assert!(find_linked(&linked, LinkedFlowRole::Failure, "flow.a").is_none());
        assert!(first_linked(&linked, LinkedFlowRole::Failure).is_none());
    }
}
