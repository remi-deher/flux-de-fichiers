//! Utilitaires déclaratifs de résolution de nœuds d'entrée dans un graphe de flux.

use crate::flows::contract::{
    OperationContract, OperationRegistry, OperationRuntime, PrimitiveKind,
};
use flux_shared::flows::FlowNode;

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
