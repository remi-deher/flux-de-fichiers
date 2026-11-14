//! Glisser-déposer : règles pures d'un dépôt sur une carte, validées par `FlowGraph::connect`.

use flux_shared::flows::{FlowBlockKind, FlowGraph, GraphError};
use serde_json::Value;

const NO_FREE_OUTPUT: &str = "Toutes les sorties de ce bloc sont déjà reliées.";
const BRANCHING_BLOCK: &str = "Ce bloc a plusieurs liaisons : déliez-le avant de le déplacer.";

/// Ce que l'on tient pendant un glisser.
#[derive(Clone, Debug, PartialEq)]
pub enum DragPayload {
    /// Un bloc de la palette, à créer au dépôt.
    Palette {
        kind: String,
        label: String,
        defaults: Value,
    },
    /// Une carte du plan, à déplacer.
    Card(String),
}

/// Résultat d'un dépôt accepté : le nouveau graphe et le bloc concerné.
#[derive(Clone, Debug, PartialEq)]
pub struct Dropped {
    pub graph: FlowGraph,
    pub node_id: String,
}

fn refusal(error: GraphError) -> &'static str {
    error.message()
}

/// Relie `node` à la suite de `target` : première sortie libre, sinon insertion dans la
/// liaison simple existante (l'ancienne suite passe derrière `node`).
fn attach_after(graph: &mut FlowGraph, target: &str, node: &str) -> Result<(), &'static str> {
    let ports = graph
        .node(target)
        .map(|source| graph.output_ports(source))
        .ok_or(refusal(GraphError::UnknownNode))?;
    if let Some(free) = ports
        .iter()
        .find(|port| graph.outgoing(target).all(|edge| &edge.port != *port))
    {
        return graph.connect(target, free.clone(), node).map_err(refusal);
    }
    let Some(next) = graph
        .outgoing(target)
        .find(|edge| edge.port.is_none())
        .map(|edge| edge.to.clone())
    else {
        return Err(NO_FREE_OUTPUT);
    };
    graph.disconnect(target, &None);
    graph.connect(target, None, node).map_err(refusal)?;
    graph.connect(node, None, &next).map_err(refusal)
}

/// Dépôt d'un bloc de la palette sur la carte `target`.
fn drop_new(
    graph: &FlowGraph,
    target: &str,
    kind: &str,
    label: &str,
    defaults: &Value,
) -> Result<Dropped, &'static str> {
    let mut next = graph.clone();
    let node_id = next.append(FlowBlockKind::parse(kind), label, defaults.clone());
    // `append` relie d'office au dernier bloc libre ; ici c'est la cible qui décide.
    let automatic: Vec<_> = next
        .incoming(&node_id)
        .map(|edge| (edge.from.clone(), edge.port.clone()))
        .collect();
    for (from, port) in automatic {
        next.disconnect(&from, &port);
    }
    attach_after(&mut next, target, &node_id)?;
    Ok(Dropped {
        graph: next,
        node_id,
    })
}

/// Dépôt de la carte `moved` derrière la carte `target` ; seul un maillon simple se déplace.
fn drop_move(graph: &FlowGraph, target: &str, moved: &str) -> Result<Dropped, &'static str> {
    if target == moved {
        return Err(refusal(GraphError::SelfLink));
    }
    let index = graph
        .nodes
        .iter()
        .position(|node| node.id == moved)
        .ok_or(refusal(GraphError::UnknownNode))?;
    if graph.node(target).is_none() {
        return Err(refusal(GraphError::UnknownNode));
    }
    let simple_outputs = graph.outgoing(moved).all(|edge| edge.port.is_none());
    if graph.incoming(moved).count() > 1 || graph.outgoing(moved).count() > 1 || !simple_outputs {
        return Err(BRANCHING_BLOCK);
    }
    let node = graph.nodes[index].clone();
    let mut next = graph.clone();
    next.remove(moved);
    next.nodes.insert(index, node);
    attach_after(&mut next, target, moved)?;
    Ok(Dropped {
        graph: next,
        node_id: moved.to_owned(),
    })
}

/// Résultat d'un dépôt de `payload` sur `target`, ou la raison du refus (graphe inchangé).
pub fn drop_outcome(
    graph: &FlowGraph,
    payload: &DragPayload,
    target: &str,
) -> Result<Dropped, &'static str> {
    match payload {
        DragPayload::Palette {
            kind,
            label,
            defaults,
        } => drop_new(graph, target, kind, label, defaults),
        DragPayload::Card(moved) => drop_move(graph, target, moved),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn chain() -> (FlowGraph, String, String) {
        let mut graph = FlowGraph::default();
        let input = graph.append(FlowBlockKind::FileInput, "Entrée", json!({}));
        let remux = graph.append(FlowBlockKind::RemuxMkv, "Remux", json!({}));
        graph.append(FlowBlockKind::FileOutput, "Fin", json!({}));
        (graph, input, remux)
    }

    fn palette(kind: &FlowBlockKind) -> DragPayload {
        DragPayload::Palette {
            kind: kind.as_str().to_owned(),
            label: "Nouveau".into(),
            defaults: json!({}),
        }
    }

    #[test]
    fn a_palette_block_is_inserted_into_the_simple_link() {
        let (graph, input, remux) = chain();
        let dropped = drop_outcome(&graph, &palette(&FlowBlockKind::Webhook), &input);
        let Ok(dropped) = dropped else {
            panic!("dépôt refusé");
        };
        let new = dropped.node_id;
        assert_eq!(
            dropped.graph.outgoing(&input).next().map(|e| &e.to),
            Some(&new)
        );
        assert_eq!(
            dropped.graph.outgoing(&new).next().map(|e| &e.to),
            Some(&remux)
        );
        assert_eq!(dropped.graph.connections.len(), 3);
    }

    #[test]
    fn a_free_output_is_used_before_inserting() {
        let mut graph = FlowGraph::default();
        let input = graph.append(FlowBlockKind::FileInput, "Entrée", json!({}));
        let dropped = drop_outcome(&graph, &palette(&FlowBlockKind::RemuxMkv), &input);
        assert_eq!(dropped.map(|d| d.graph.connections.len()), Ok(1));
    }

    #[test]
    fn a_block_without_input_is_refused_and_leaves_the_graph_alone() {
        let (graph, input, _) = chain();
        let refused = drop_outcome(&graph, &palette(&FlowBlockKind::FileInput), &input);
        assert!(refused.is_err());
    }

    #[test]
    fn a_terminal_target_has_no_free_output() {
        let (graph, _, _) = chain();
        let end = graph.nodes[2].id.clone();
        assert_eq!(
            drop_outcome(&graph, &palette(&FlowBlockKind::Webhook), &end),
            Err(NO_FREE_OUTPUT)
        );
    }

    #[test]
    fn a_card_moves_behind_another_and_the_chain_closes_up() {
        let (graph, input, first) = chain();
        let end = graph.nodes[2].id.clone();
        let mut longer = graph.clone();
        longer.remove(&end);
        let second = longer.append(FlowBlockKind::RemuxMkv, "Remux 2", json!({}));
        let end = longer.append(FlowBlockKind::FileOutput, "Fin", json!({}));
        // Déjà derrière l'entrée : l'ordre ne change pas.
        let Ok(unchanged) = drop_outcome(&longer, &DragPayload::Card(first.clone()), &input) else {
            panic!("déplacement refusé");
        };
        assert_eq!(unchanged.graph.connections.len(), 3);
        // Le premier remux passe derrière le second : entrée → second → premier → fin.
        let Ok(moved) = drop_outcome(&longer, &DragPayload::Card(first.clone()), &second) else {
            panic!("déplacement refusé");
        };
        let chain_ids: Vec<_> = [&input, &second, &first, &end]
            .windows(2)
            .map(|pair| {
                moved
                    .graph
                    .outgoing(pair[0])
                    .any(|edge| &edge.to == pair[1])
            })
            .collect();
        assert_eq!(chain_ids, [true, true, true]);
        assert_eq!(moved.graph.nodes.len(), 4);
        assert_eq!(moved.graph.connections.len(), 3);
    }

    #[test]
    fn a_block_without_input_cannot_move_behind_another() {
        let (graph, input, remux) = chain();
        assert_eq!(
            drop_outcome(&graph, &DragPayload::Card(input), &remux),
            Err(refusal(GraphError::InvalidPort))
        );
    }

    #[test]
    fn a_card_cannot_be_dropped_on_itself_or_move_while_branching() {
        let (graph, input, _) = chain();
        assert!(drop_outcome(&graph, &DragPayload::Card(input.clone()), &input).is_err());
        let mut branching = FlowGraph::default();
        branching.append(FlowBlockKind::FileInput, "Entrée", json!({}));
        let cond = branching.append(FlowBlockKind::Condition, "Si", json!({}));
        let yes = branching.append(FlowBlockKind::RemuxMkv, "Oui", json!({}));
        let no = branching.append(FlowBlockKind::RemuxMkv, "Non", json!({}));
        let _ = branching.connect(&cond, Some("true".into()), &yes);
        let _ = branching.connect(&cond, Some("false".into()), &no);
        let other = yes;
        assert_eq!(
            drop_outcome(&branching, &DragPayload::Card(cond), &other),
            Err(BRANCHING_BLOCK)
        );
    }
}
