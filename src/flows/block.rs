//! Utilitaires déclaratifs de résolution de nœuds d'entrée dans un graphe de flux.

use crate::flows::contract::{
    OperationContract, OperationRegistry, OperationRuntime, PrimitiveKind,
};
use forgearr_shared::flows::{FlowGraph, FlowNode};

/// Indique si un nœud correspond à une primitive d'entrée (`runtime.primitive == "entry"`).
pub fn is_entry_node(registry: &OperationRegistry, node: &FlowNode) -> bool {
    matches!(
        registry.resolve_node_operation(node),
        Some(OperationContract {
            runtime: OperationRuntime::Primitive(prim),
            ..
        }) if prim.primitive == PrimitiveKind::Entry
    )
}

/// Bloc d'entrée d'un graphe : la primitive d'entrée, sinon le premier bloc.
pub fn entry_node_id(registry: &OperationRegistry, graph: &FlowGraph) -> Option<String> {
    graph
        .nodes
        .iter()
        .find(|node| is_entry_node(registry, node))
        .or_else(|| graph.nodes.first())
        .map(|node| node.id.clone())
}
