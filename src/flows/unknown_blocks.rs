//! Blocs d'un flux que rien d'installé ne porte (ADR 0029).

use super::OperationRegistry;
use forgearr_shared::flows::{FlowGraph, FlowNode, OwnedBlockSpec, find_owned};
use std::collections::BTreeSet;

/// Rien dans le moteur ne décrit ni n'exécute-t-il ce nœud ?
pub fn is_unknown_block(
    node: &FlowNode,
    blocks: &[OwnedBlockSpec],
    registry: &OperationRegistry,
) -> bool {
    find_owned(blocks, &node.kind).is_none() && registry.resolve_node_operation(node).is_none()
}

pub fn unknown_block_message(kind: &str) -> String {
    format!("type de bloc inconnu « {kind} » : installez-le depuis le Store")
}

/// Types inconnus que `saved`, le graphe enregistré, n'a pas déjà : un flux dont un bloc a disparu reste
/// modifiable.
pub fn added_unknown_blocks(
    graph: &FlowGraph,
    saved: Option<&FlowGraph>,
    blocks: &[OwnedBlockSpec],
    registry: &OperationRegistry,
) -> BTreeSet<String> {
    let is_saved = |node: &FlowNode| {
        saved.is_some_and(|saved| saved.nodes.iter().any(|known| known.kind == node.kind))
    };
    graph
        .nodes
        .iter()
        .filter(|node| !is_saved(node) && is_unknown_block(node, blocks, registry))
        .map(|node| node.kind.as_str().to_owned())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    const OPERATION: &str = "operation.demo.run";

    fn graph(nodes: Value) -> FlowGraph {
        FlowGraph::parse(&json!({"nodes": nodes, "connections": []}).to_string())
    }

    #[test]
    fn only_a_kind_the_saved_flow_does_not_have_is_added() -> anyhow::Result<()> {
        let mut registry = OperationRegistry::new();
        registry.register_json(json!({
            "schemaVersion": "1.1.0", "id": OPERATION, "version": "1.0.0", "name": "Demo",
            "providerId": "demo", "providerKind": "plugin", "capability": "x", "status": "validated",
            "runtime": {"kind": "primitive", "primitive": "entry"}
        }))?;
        registry.register_block_mapping("demo.run", OPERATION);
        let edited = graph(json!([
            {"id": "a", "type": "demo.run", "name": "A"},
            {"id": "b", "type": "perdu.run", "name": "B"},
            {"id": "c", "type": "absent.run", "name": "C"},
            {"id": "d", "type": "absent.run", "name": "D"},
            // L'opération nommée par le nœud suffit à l'exécuter.
            {"id": "e", "type": "libre", "name": "E", "operationId": OPERATION}
        ]));
        let saved = graph(json!([{"id": "b", "type": "perdu.run", "name": "B"}]));
        let added = |saved: Option<&FlowGraph>| {
            added_unknown_blocks(&edited, saved, &[], &registry)
                .into_iter()
                .collect::<Vec<_>>()
        };
        assert_eq!(added(None), ["absent.run", "perdu.run"]);
        assert_eq!(added(Some(&saved)), ["absent.run"]);
        assert_eq!(added(Some(&edited)), Vec::<String>::new());
        let message = unknown_block_message("absent.run");
        assert!(message.contains("absent.run") && message.contains("depuis le Store"));
        Ok(())
    }
}
