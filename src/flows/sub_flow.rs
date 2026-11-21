//! Bloc primitif « Aller au sous-flux » : appel d'un flux lié figé, avec retour (ADR 0026).

use super::{block, contract::OperationRegistry};
use forgearr_shared::flows::{
    FlowGraph, FlowNode, LinkedFlow, LinkedFlowRole, MAX_SUB_FLOW_DEPTH, find_linked,
    sub_flow_target,
};

/// Appelant mis de côté pendant l'exécution d'un sous-flux.
#[derive(Debug)]
pub(super) struct SubFlowFrame {
    /// Graphe de l'appelant.
    pub graph: FlowGraph,
    /// Flux de l'appelant.
    pub flow_id: String,
    /// Bloc « Aller au sous-flux » dont la sortie reprend après le retour.
    pub call_node_id: String,
}

/// Sous-flux prêt à être exécuté.
#[derive(Debug)]
pub(super) struct SubFlowEntry {
    pub graph: FlowGraph,
    pub flow_id: String,
    pub version: i64,
    pub entry_node_id: String,
}

impl SubFlowEntry {
    pub fn step_message(&self) -> String {
        format!("sous-flux « {} » (v{}) exécuté", self.flow_id, self.version)
    }
}

/// Prépare l'appel du sous-flux désigné par `node`, à partir des versions figées de la tâche.
/// Refuse une cible absente, non figée, illisible, un appel récursif ou trop profond.
pub(super) fn enter_sub_flow(
    registry: &OperationRegistry,
    linked: &[LinkedFlow],
    node: &FlowNode,
    active_flow_id: &str,
    frames: &[SubFlowFrame],
) -> Result<SubFlowEntry, String> {
    let target = sub_flow_target(node)
        .ok_or_else(|| format!("le bloc {} n'indique aucun sous-flux à exécuter", node.id))?;
    let recursive = target == active_flow_id || frames.iter().any(|frame| frame.flow_id == target);
    if recursive {
        return Err(format!("appel récursif du sous-flux « {target} » refusé"));
    }
    if frames.len() >= MAX_SUB_FLOW_DEPTH {
        return Err(format!(
            "profondeur maximale de sous-flux atteinte ({MAX_SUB_FLOW_DEPTH})"
        ));
    }
    let Some(record) = find_linked(linked, LinkedFlowRole::SubFlow, target) else {
        return Err(format!(
            "le sous-flux « {target} » n'a pas été figé avec la tâche"
        ));
    };
    let graph: FlowGraph = serde_json::from_str(&record.definition)
        .map_err(|err| format!("définition du sous-flux « {target} » invalide : {err}"))?;
    let entry_node_id = block::entry_node_id(registry, &graph)
        .ok_or_else(|| format!("le sous-flux « {target} » ne contient aucun bloc"))?;
    Ok(SubFlowEntry {
        graph,
        flow_id: record.id.clone(),
        version: record.version,
        entry_node_id,
    })
}
