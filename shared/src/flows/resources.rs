//! Classe de ressource d'un flux figé : CPU, ou GPU si un bloc exige l'accélération matérielle.
//!
//! Règle (V1, sans champ dédié dans le flux) : la tâche est GPU dès qu'un bloc du graphe porte
//! une config `acceleration` valant `nvenc` ou `vaapi` — le champ que lisent déjà le placement et
//! les relais. Le classement est pessimiste : un bloc GPU sur une branche conditionnelle classe
//! toute la tâche en GPU, pour réserver la place au pire cas. Un bloc script n'est GPU que s'il
//! déclare lui-même `acceleration`. Seule une valeur littérale compte : une variable non résolue
//! (`{{…}}`) ou une valeur inconnue donne CPU.

use super::FlowGraph;
use crate::capabilities::EncodeAcceleration;
use crate::jobs::JobResourceClass;

/// Clé de config d'un bloc qui choisit l'accélération d'encodage.
const ACCELERATION_CONFIG_KEY: &str = "acceleration";

/// Classe de ressource exigée par le graphe (voir l'en-tête du module).
pub fn flow_resource_class(graph: &FlowGraph) -> JobResourceClass {
    let hardware = graph.nodes.iter().any(|node| {
        node.config
            .get(ACCELERATION_CONFIG_KEY)
            .cloned()
            .and_then(|value| serde_json::from_value::<EncodeAcceleration>(value).ok())
            .is_some_and(|acceleration| acceleration != EncodeAcceleration::Cpu)
    });
    if hardware {
        JobResourceClass::Gpu
    } else {
        JobResourceClass::Cpu
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn graph_with(acceleration: serde_json::Value) -> Result<FlowGraph, serde_json::Error> {
        serde_json::from_value(json!({
            "nodes": [
                {"id": "in", "type": "file-input", "name": "Entrée"},
                {"id": "enc", "type": "video.encode", "name": "Encoder",
                 "config": {"codec": "hevc", "acceleration": acceleration}}
            ],
            "connections": [{"from": "in", "to": "enc"}]
        }))
    }

    #[test]
    fn empty_graph_is_cpu() {
        assert_eq!(
            flow_resource_class(&FlowGraph::default()),
            JobResourceClass::Cpu
        );
    }

    #[test]
    fn hardware_acceleration_is_gpu() -> Result<(), serde_json::Error> {
        assert_eq!(
            flow_resource_class(&graph_with(json!("cpu"))?),
            JobResourceClass::Cpu
        );
        assert_eq!(
            flow_resource_class(&graph_with(json!("nvenc"))?),
            JobResourceClass::Gpu
        );
        assert_eq!(
            flow_resource_class(&graph_with(json!("vaapi"))?),
            JobResourceClass::Gpu
        );
        Ok(())
    }

    #[test]
    fn unknown_or_templated_values_are_cpu() -> Result<(), serde_json::Error> {
        for value in [
            json!("quantum"),
            json!("{{custom.accel}}"),
            json!(42),
            json!(null),
        ] {
            assert_eq!(
                flow_resource_class(&graph_with(value)?),
                JobResourceClass::Cpu
            );
        }
        Ok(())
    }
}
