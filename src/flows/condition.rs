//! Conditions d'aiguillage côté moteur : contrat partagé et validation à l'enregistrement.

pub use forgearr_shared::flows::{
    ConditionConfig, evaluate_condition, evaluate_condition_with_variables, validate_condition,
};
use forgearr_shared::flows::{FlowGraph, FlowNode};
use serde_json::Value;

/// Clés qui signalent une configuration de condition dans `node.config`.
const GROUP_KEY: &str = "group";
const PROPERTY_KEY: &str = "property";
const OPERATOR_KEY: &str = "operator";

fn looks_like_condition(config: &Value) -> bool {
    config.get(GROUP_KEY).is_some()
        || (config.get(PROPERTY_KEY).is_some() && config.get(OPERATOR_KEY).is_some())
}

/// Lit la condition d'un nœud ; une configuration illisible vaut la condition par défaut.
pub fn read_condition(node_id: &str, config: &Value) -> ConditionConfig {
    serde_json::from_value(config.clone()).unwrap_or_else(|error| {
        tracing::warn!(node_id, %error, "condition illisible, repli sur la condition par défaut");
        ConditionConfig::default()
    })
}

fn node_condition_error(node: &FlowNode) -> Option<String> {
    if !looks_like_condition(&node.config) {
        return None;
    }
    let config: ConditionConfig = match serde_json::from_value(node.config.clone()) {
        Ok(config) => config,
        Err(error) => return Some(format!("Nœud {} : condition illisible ({error})", node.id)),
    };
    validate_condition(&config)
        .err()
        .map(|error| format!("Nœud {} : {error}", node.id))
}

/// Erreurs de condition d'un graphe (profondeur, nombre de règles, expressions régulières).
pub fn graph_condition_errors(graph: &FlowGraph) -> Vec<String> {
    graph
        .nodes
        .iter()
        .filter_map(node_condition_error)
        .collect()
}

/// Erreurs de condition d'une définition JSON (une définition illisible relève d'autres contrôles).
pub fn definition_condition_errors(definition: &str) -> Vec<String> {
    serde_json::from_str::<FlowGraph>(definition)
        .map(|graph| graph_condition_errors(&graph))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use forgearr_shared::flows::{ConditionOperator, MAX_CONDITION_DEPTH};
    use serde_json::json;

    fn nested_group(levels: usize) -> Value {
        let mut group = json!({"rules": [{"property": "format", "operator": "exists"}]});
        for _ in 1..levels {
            group = json!({"combinator": "all", "rules": [group]});
        }
        group
    }

    fn definition(config: Value) -> String {
        let node = json!({
            "id": "c1", "type": "essentiel.condition", "name": "C", "config": config
        });
        json!({ "nodes": [node] }).to_string()
    }

    #[test]
    fn accepts_legacy_and_bounded_conditions() {
        let legacy = definition(json!({"property": "codec", "operator": "eq", "target": "hevc"}));
        assert!(definition_condition_errors(&legacy).is_empty());
        let bounded = definition(json!({"group": nested_group(MAX_CONDITION_DEPTH)}));
        assert!(definition_condition_errors(&bounded).is_empty());
        let other_block = definition(json!({"url": "https://x"}));
        assert!(definition_condition_errors(&other_block).is_empty());
        assert!(definition_condition_errors("pas du json").is_empty());
    }

    #[test]
    fn rejects_too_deep_groups_and_invalid_regex() {
        let deep = definition(json!({"group": nested_group(MAX_CONDITION_DEPTH + 1)}));
        let errors = definition_condition_errors(&deep);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].starts_with("Nœud c1"));
        let regex = definition(json!({"property": "codec", "operator": "regex", "target": "("}));
        assert_eq!(definition_condition_errors(&regex).len(), 1);
    }

    #[test]
    fn read_condition_falls_back_to_default() {
        let invalid = read_condition("n", &json!("pas un objet"));
        assert_eq!(invalid, ConditionConfig::default());
        let flat = json!({"property": "a", "operator": "exact", "target": "b"});
        assert_eq!(
            read_condition("n", &flat),
            ConditionConfig::single("a", ConditionOperator::Exact, "b")
        );
    }
}
