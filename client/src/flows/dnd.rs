//! Glisser-déposer et opérations de câblage sur le graphe, validées par `FlowGraph`.

use super::presentation::node_output_ports;
use flux_shared::flows::{
    FlowBlockKind, FlowGraph, FlowNode, GraphError, OwnedBlockSpec, find_owned,
};
use serde_json::{Map, Value};
use std::collections::{HashSet, VecDeque};

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

/// Cible d'insertion sur un port de sortie ou sur une liaison existante.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BranchTarget {
    pub source_id: String,
    pub port: Option<String>,
    pub replace_to: Option<String>,
}

fn next_node_id(graph: &FlowGraph) -> String {
    let used: HashSet<&str> = graph.nodes.iter().map(|node| node.id.as_str()).collect();
    (graph.nodes.len() + 1..)
        .map(|index| format!("node-{index}"))
        .find(|id| !used.contains(id.as_str()))
        .unwrap_or_else(|| format!("node-{}", graph.nodes.len() + 1))
}

fn reaches(graph: &FlowGraph, from: &str, target: &str) -> bool {
    let mut seen = HashSet::new();
    let mut queue = VecDeque::from([from]);
    while let Some(current) = queue.pop_front() {
        if current == target {
            return true;
        }
        if seen.insert(current) {
            queue.extend(graph.outgoing(current).map(|edge| edge.to.as_str()));
        }
    }
    false
}

/// Indique si une liaison `(from, port) -> to` est autorisée (sans boucle ni port invalide).
pub fn can_connect(
    graph: &FlowGraph,
    from: &str,
    port: &Option<String>,
    to: &str,
    catalog: &[OwnedBlockSpec],
) -> bool {
    if from == to {
        return false;
    }
    let (Some(source), Some(target)) = (graph.node(from), graph.node(to)) else {
        return false;
    };
    let target_has_input = target.kind != FlowBlockKind::FileInput
        && find_owned(catalog, &target.kind).is_none_or(|spec| spec.has_input);
    if !target_has_input {
        return false;
    }
    if !node_output_ports(graph, source, catalog).contains(port) {
        return false;
    }
    if graph.outgoing(from).any(|edge| &edge.port == port) {
        return false;
    }
    !reaches(graph, to, from)
}

/// Crée une liaison `(from, port) -> to` en tenant compte du catalogue courant.
pub fn connect_checked(
    graph: &mut FlowGraph,
    from: &str,
    port: Option<String>,
    to: &str,
    catalog: &[OwnedBlockSpec],
) -> Result<(), GraphError> {
    if graph.node(from).is_none() || graph.node(to).is_none() {
        return Err(GraphError::UnknownNode);
    }
    if from == to {
        return Err(GraphError::SelfLink);
    }
    if graph.outgoing(from).any(|edge| edge.port == port) {
        return Err(GraphError::PortTaken);
    }
    if !can_connect(graph, from, &port, to, catalog) {
        return Err(if reaches(graph, to, from) {
            GraphError::Cycle
        } else {
            GraphError::InvalidPort
        });
    }
    graph.connections.push(flux_shared::flows::FlowConnection {
        from: from.to_owned(),
        to: to.to_owned(),
        port,
    });
    Ok(())
}

/// Supprime précisément une liaison orientée du graphe.
pub fn disconnect_edge(graph: &mut FlowGraph, from: &str, port: &Option<String>, to: &str) {
    graph
        .connections
        .retain(|edge| !(edge.from == from && &edge.port == port && edge.to == to));
}

/// Insère un nouveau bloc sur une sortie donnée et raccorde l'éventuel successeur existant.
pub fn insert_on_branch(
    graph: &mut FlowGraph,
    target: &BranchTarget,
    spec: &OwnedBlockSpec,
    defaults: Value,
) -> Result<String, GraphError> {
    let Some(source) = graph.node(&target.source_id) else {
        return Err(GraphError::UnknownNode);
    };
    let kind = spec.kind_enum();
    if !spec.has_input || kind == FlowBlockKind::FileInput {
        return Err(GraphError::InvalidPort);
    }
    let source_has_outputs = source.kind != FlowBlockKind::FileOutput;
    if !source_has_outputs {
        return Err(GraphError::InvalidPort);
    }

    let previous_next = target.replace_to.clone().or_else(|| {
        graph
            .outgoing(&target.source_id)
            .find(|edge| edge.port == target.port)
            .map(|edge| edge.to.clone())
    });

    let new_id = next_node_id(graph);
    graph.nodes.push(FlowNode {
        id: new_id.clone(),
        kind,
        name: spec.label.clone(),
        config: defaults,
        extra: Map::new(),
    });

    graph.disconnect(&target.source_id, &target.port);
    graph.connections.push(flux_shared::flows::FlowConnection {
        from: target.source_id.clone(),
        to: new_id.clone(),
        port: target.port.clone(),
    });

    if let Some(next_id) = previous_next
        && !spec.terminal
        && graph.node(&next_id).is_some()
    {
        let bridge_port = spec.ports.first().map(|(port, _)| port.clone());
        graph.connections.push(flux_shared::flows::FlowConnection {
            from: new_id.clone(),
            to: next_id,
            port: bridge_port,
        });
    }

    Ok(new_id)
}

/// Duplique un bloc existant (nom suffixé « (copie) ») sans copier ses liaisons.
pub fn duplicate_node(graph: &mut FlowGraph, node_id: &str) -> Option<String> {
    let source = graph.node(node_id)?.clone();
    let new_id = next_node_id(graph);
    let duplicated = FlowNode {
        id: new_id.clone(),
        kind: source.kind,
        name: format!("{} (copie)", source.name),
        config: source.config,
        extra: Map::new(),
    };
    if let Some(pos) = graph.nodes.iter().position(|node| node.id == node_id) {
        graph.nodes.insert(pos + 1, duplicated);
    } else {
        graph.nodes.push(duplicated);
    }
    Some(new_id)
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
        let (mut graph, input, remux) = chain();
        let hook = graph.append(FlowBlockKind::Webhook, "Hook", json!({}));
        let end = graph.nodes[2].id.clone();
        // Fin est terminal : Hook reste non relié, puis s'insère derrière l'entrée.
        let Ok(moved) = drop_outcome(&graph, &DragPayload::Card(remux.clone()), &input) else {
            panic!("déplacement refusé");
        };
        assert_eq!(moved.graph.connections.len(), graph.connections.len());
        let Ok(moved) = drop_outcome(&graph, &DragPayload::Card(hook.clone()), &input) else {
            panic!("déplacement refusé");
        };
        assert_eq!(
            moved.graph.incoming(&hook).next().map(|e| &e.from),
            Some(&input)
        );
        assert_eq!(
            moved.graph.incoming(&remux).next().map(|e| &e.from),
            Some(&hook)
        );
        assert_eq!(moved.graph.nodes.len(), graph.nodes.len());
        assert!(moved.graph.node(&end).is_some());
    }

    #[test]
    fn the_entry_block_cannot_move_behind_another() {
        let (graph, input, remux) = chain();
        assert!(drop_outcome(&graph, &DragPayload::Card(input), &remux).is_err());
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

    #[test]
    fn insert_on_branch_bridges_existing_edge_and_rejects_file_input() {
        let catalog = flux_shared::flows::native_owned_catalog();
        let (mut graph, input, remux) = chain();
        let webhook_spec = catalog.iter().find(|s| s.kind == "webhook").cloned();
        let Some(webhook_spec) = webhook_spec else {
            panic!("webhook spec missing");
        };
        let target = BranchTarget {
            source_id: input.clone(),
            port: None,
            replace_to: Some(remux.clone()),
        };
        let inserted = insert_on_branch(
            &mut graph,
            &target,
            &webhook_spec,
            json!({"url": "https://a"}),
        );
        let Ok(new_id) = inserted else {
            panic!("insertion refusée");
        };
        assert_eq!(
            graph.outgoing(&input).next().map(|e| e.to.as_str()),
            Some(new_id.as_str())
        );
        assert_eq!(
            graph.outgoing(&new_id).next().map(|e| e.to.as_str()),
            Some(remux.as_str())
        );

        let input_spec = catalog.iter().find(|s| s.kind == "file-input").cloned();
        let Some(input_spec) = input_spec else {
            panic!("file-input spec missing");
        };
        assert_eq!(
            insert_on_branch(&mut graph, &target, &input_spec, json!({})),
            Err(GraphError::InvalidPort)
        );
    }

    #[test]
    fn duplicate_node_clones_config_and_connect_checked_guards_cycles() {
        let catalog = flux_shared::flows::native_owned_catalog();
        let (mut graph, input, remux) = chain();
        let copy_id = duplicate_node(&mut graph, &remux);
        let Some(copy_id) = copy_id else {
            panic!("duplicate failed");
        };
        assert_eq!(
            graph.node(&copy_id).map(|n| n.name.as_str()),
            Some("Remux (copie)")
        );
        assert!(!can_connect(&graph, &remux, &None, &copy_id, &catalog));
        graph.disconnect(&remux, &None);
        assert!(can_connect(&graph, &remux, &None, &copy_id, &catalog));
        assert_eq!(
            connect_checked(&mut graph, &copy_id, None, &input, &catalog),
            Err(GraphError::InvalidPort)
        );
        assert_eq!(
            connect_checked(&mut graph, &remux, None, &copy_id, &catalog),
            Ok(())
        );
        disconnect_edge(&mut graph, &remux, &None, &copy_id);
        assert_eq!(graph.outgoing(&remux).count(), 0);
    }
}
