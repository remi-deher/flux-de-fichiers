//! État partagé de l'éditeur : graphe édité, bloc sélectionné, modifications non enregistrées.

use flux_shared::flows::{FlowGraph, OwnedBlockSpec, find_owned, native_owned_catalog};
use leptos::prelude::*;

#[derive(Clone, Copy)]
pub struct Editor {
    pub graph: RwSignal<FlowGraph>,
    pub selected: RwSignal<Option<String>>,
    pub dirty: RwSignal<bool>,
    /// Catalogue natif + blocs des dépôts (chargé via l'API).
    pub catalog: RwSignal<Vec<OwnedBlockSpec>>,
}

impl Editor {
    pub fn new(graph: FlowGraph) -> Self {
        let selected = graph.nodes.first().map(|node| node.id.clone());
        Self {
            graph: RwSignal::new(graph),
            selected: RwSignal::new(selected),
            dirty: RwSignal::new(false),
            catalog: RwSignal::new(native_owned_catalog()),
        }
    }

    /// Modifie le graphe et marque l'éditeur comme modifié.
    pub fn edit(&self, change: impl FnOnce(&mut FlowGraph)) {
        self.graph.update(change);
        self.dirty.set(true);
    }
}

/// Blocs dont un réglage obligatoire est vide : `(nom du bloc, champs manquants)`.
pub fn incomplete_blocks(
    graph: &FlowGraph,
    catalog: &[OwnedBlockSpec],
) -> Vec<(String, Vec<String>)> {
    graph
        .nodes
        .iter()
        .filter_map(|node| {
            let missing = find_owned(catalog, &node.kind)?.missing_fields(&node.config);
            (!missing.is_empty()).then(|| (node.name.clone(), missing))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use flux_shared::flows::FlowBlockKind;
    use serde_json::json;

    #[test]
    fn only_blocks_with_empty_required_fields_are_reported() {
        let catalog = native_owned_catalog();
        let mut graph = FlowGraph::default();
        graph.append(FlowBlockKind::FileInput, "Entrée", json!({}));
        graph.append(FlowBlockKind::Webhook, "Hook", json!({"url": ""}));
        graph.append(FlowBlockKind::parse("macro.unknown"), "Inconnu", json!({}));
        assert_eq!(
            incomplete_blocks(&graph, &catalog),
            vec![("Hook".to_owned(), vec!["URL".to_owned()])]
        );
    }
}
