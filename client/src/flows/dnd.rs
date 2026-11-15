//! Glisser-déposer et opérations de câblage sur le graphe, validées par `FlowGraph`.

use super::presentation::node_output_ports;
use flux_shared::flows::{
    FlowBlockKind, FlowConnection, FlowGraph, FlowNode, GraphError, OwnedBlockSpec, find_owned,
};
use serde_json::{Map, Value};
use std::collections::{HashMap, HashSet, VecDeque};

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
fn attach_after(
    graph: &mut FlowGraph,
    target: &str,
    node: &str,
    catalog: &[OwnedBlockSpec],
) -> Result<(), &'static str> {
    let ports = graph
        .node(target)
        .map(|source| graph.output_ports(source, catalog))
        .ok_or(refusal(GraphError::UnknownNode))?;
    if let Some(free) = ports
        .iter()
        .find(|port| graph.outgoing(target).all(|edge| &edge.port != *port))
    {
        return graph
            .connect(target, free.clone(), node, catalog)
            .map_err(refusal);
    }
    let Some(next) = graph
        .outgoing(target)
        .find(|edge| edge.port.is_none())
        .map(|edge| edge.to.clone())
    else {
        return Err(NO_FREE_OUTPUT);
    };
    graph.disconnect(target, &None);
    graph
        .connect(target, None, node, catalog)
        .map_err(refusal)?;
    graph.connect(node, None, &next, catalog).map_err(refusal)
}

/// Dépôt d'un bloc de la palette sur la carte `target`.
fn drop_new(
    graph: &FlowGraph,
    target: &str,
    kind: &str,
    label: &str,
    defaults: &Value,
    catalog: &[OwnedBlockSpec],
) -> Result<Dropped, &'static str> {
    let mut next = graph.clone();
    let node_id = next.append(FlowBlockKind::parse(kind), label, defaults.clone(), catalog);
    // `append` relie d'office au dernier bloc libre ; ici c'est la cible qui décide.
    let automatic: Vec<_> = next
        .incoming(&node_id)
        .map(|edge| (edge.from.clone(), edge.port.clone()))
        .collect();
    for (from, port) in automatic {
        next.disconnect(&from, &port);
    }
    attach_after(&mut next, target, &node_id, catalog)?;
    Ok(Dropped {
        graph: next,
        node_id,
    })
}

/// Dépôt de la carte `moved` derrière la carte `target` ; seul un maillon simple se déplace.
fn drop_move(
    graph: &FlowGraph,
    target: &str,
    moved: &str,
    catalog: &[OwnedBlockSpec],
) -> Result<Dropped, &'static str> {
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
    attach_after(&mut next, target, moved, catalog)?;
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
    catalog: &[OwnedBlockSpec],
) -> Result<Dropped, &'static str> {
    match payload {
        DragPayload::Palette {
            kind,
            label,
            defaults,
        } => drop_new(graph, target, kind, label, defaults, catalog),
        DragPayload::Card(moved) => drop_move(graph, target, moved, catalog),
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
    let target_has_input = find_owned(catalog, &target.kind).is_none_or(|spec| spec.has_input);
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
    catalog: &[OwnedBlockSpec],
) -> Result<String, GraphError> {
    let Some(source) = graph.node(&target.source_id) else {
        return Err(GraphError::UnknownNode);
    };
    let kind = spec.kind_enum();
    if !spec.has_input {
        return Err(GraphError::InvalidPort);
    }
    let source_has_outputs = !node_output_ports(graph, source, catalog).is_empty();
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

fn clone_node_with_id(source: &FlowNode, new_id: String) -> FlowNode {
    FlowNode {
        id: new_id,
        kind: source.kind.clone(),
        name: format!("{} (copie)", source.name),
        config: source.config.clone(),
        extra: Map::new(),
    }
}

/// Duplique un bloc existant (nom suffixé « (copie) ») sans copier ses liaisons.
pub fn duplicate_node(graph: &mut FlowGraph, node_id: &str) -> Option<String> {
    let source = graph.node(node_id)?.clone();
    let new_id = next_node_id(graph);
    let duplicated = clone_node_with_id(&source, new_id.clone());
    if let Some(pos) = graph.nodes.iter().position(|node| node.id == node_id) {
        graph.nodes.insert(pos + 1, duplicated);
    } else {
        graph.nodes.push(duplicated);
    }
    Some(new_id)
}

/// Duplique un sous-graphe (`ids`) en conservant uniquement les liaisons internes au groupe.
pub fn duplicate_subgraph(graph: &mut FlowGraph, ids: &HashSet<String>) -> HashMap<String, String> {
    let mut id_map = HashMap::new();
    let sources: Vec<FlowNode> = graph
        .nodes
        .iter()
        .filter(|node| ids.contains(&node.id))
        .cloned()
        .collect();
    for source in sources {
        let new_id = next_node_id(graph);
        let cloned = clone_node_with_id(&source, new_id.clone());
        graph.nodes.push(cloned);
        id_map.insert(source.id, new_id);
    }
    let internal_edges: Vec<FlowConnection> = graph
        .connections
        .iter()
        .filter_map(|edge| {
            let new_from = id_map.get(&edge.from)?;
            let new_to = id_map.get(&edge.to)?;
            Some(FlowConnection {
                from: new_from.clone(),
                to: new_to.clone(),
                port: edge.port.clone(),
            })
        })
        .collect();
    graph.connections.extend(internal_edges);
    id_map
}

#[cfg(test)]
mod tests {
    use super::super::test_support::test_catalog;
    use super::*;
    use serde_json::json;

    fn chain(catalog: &[OwnedBlockSpec]) -> (FlowGraph, String, String) {
        let mut graph = FlowGraph::default();
        let input = graph.append(
            FlowBlockKind::parse("block.core.file-input"),
            "Entrée",
            json!({}),
            catalog,
        );
        let remux = graph.append(
            FlowBlockKind::parse("block.video.remux-mkv"),
            "Remux",
            json!({}),
            catalog,
        );
        graph.append(
            FlowBlockKind::parse("block.core.file-output"),
            "Fin",
            json!({}),
            catalog,
        );
        (graph, input, remux)
    }

    fn palette(kind: &str) -> DragPayload {
        DragPayload::Palette {
            kind: kind.to_owned(),
            label: "Nouveau".into(),
            defaults: json!({}),
        }
    }

    #[test]
    fn a_palette_block_is_inserted_into_the_simple_link() {
        let catalog = test_catalog();
        let (graph, input, remux) = chain(&catalog);
        let dropped = drop_outcome(&graph, &palette("block.core.webhook"), &input, &catalog);
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
        let catalog = test_catalog();
        let mut graph = FlowGraph::default();
        let input = graph.append(
            FlowBlockKind::parse("block.core.file-input"),
            "Entrée",
            json!({}),
            &catalog,
        );
        let dropped = drop_outcome(&graph, &palette("block.video.remux-mkv"), &input, &catalog);
        assert_eq!(dropped.map(|d| d.graph.connections.len()), Ok(1));
    }

    #[test]
    fn a_block_without_input_is_refused_and_leaves_the_graph_alone() {
        let catalog = test_catalog();
        let (graph, input, _) = chain(&catalog);
        let refused = drop_outcome(&graph, &palette("block.core.file-input"), &input, &catalog);
        assert!(refused.is_err());
    }

    #[test]
    fn a_terminal_target_has_no_free_output() {
        let catalog = test_catalog();
        let (graph, _, _) = chain(&catalog);
        let end = graph.nodes[2].id.clone();
        assert_eq!(
            drop_outcome(&graph, &palette("block.core.webhook"), &end, &catalog),
            Err(NO_FREE_OUTPUT)
        );
    }

    #[test]
    fn a_card_moves_behind_another_and_the_chain_closes_up() {
        let catalog = test_catalog();
        let (mut graph, input, remux) = chain(&catalog);
        let hook = graph.append(
            FlowBlockKind::parse("block.core.webhook"),
            "Hook",
            json!({}),
            &catalog,
        );
        let end = graph.nodes[2].id.clone();
        // Fin est terminal : Hook reste non relié, puis s'insère derrière l'entrée.
        let Ok(moved) = drop_outcome(&graph, &DragPayload::Card(remux.clone()), &input, &catalog)
        else {
            panic!("déplacement refusé");
        };
        assert_eq!(moved.graph.connections.len(), graph.connections.len());
        let Ok(moved) = drop_outcome(&graph, &DragPayload::Card(hook.clone()), &input, &catalog)
        else {
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
        let catalog = test_catalog();
        let (graph, input, remux) = chain(&catalog);
        assert!(drop_outcome(&graph, &DragPayload::Card(input), &remux, &catalog).is_err());
    }

    #[test]
    fn a_card_cannot_be_dropped_on_itself_or_move_while_branching() {
        let catalog = test_catalog();
        let (graph, input, _) = chain(&catalog);
        assert!(drop_outcome(&graph, &DragPayload::Card(input.clone()), &input, &catalog).is_err());
        let mut branching = FlowGraph::default();
        branching.append(
            FlowBlockKind::parse("block.core.file-input"),
            "Entrée",
            json!({}),
            &catalog,
        );
        let cond = branching.append(
            FlowBlockKind::parse("block.core.condition"),
            "Si",
            json!({}),
            &catalog,
        );
        let yes = branching.append(
            FlowBlockKind::parse("block.video.remux-mkv"),
            "Oui",
            json!({}),
            &catalog,
        );
        let no = branching.append(
            FlowBlockKind::parse("block.video.remux-mkv"),
            "Non",
            json!({}),
            &catalog,
        );
        let _ = branching.connect(&cond, Some("true".into()), &yes, &catalog);
        let _ = branching.connect(&cond, Some("false".into()), &no, &catalog);
        let other = yes;
        assert_eq!(
            drop_outcome(&branching, &DragPayload::Card(cond), &other, &catalog),
            Err(BRANCHING_BLOCK)
        );
    }

    #[test]
    fn insert_on_branch_bridges_existing_edge_and_rejects_file_input() {
        let catalog = test_catalog();
        let (mut graph, input, remux) = chain(&catalog);
        let webhook_spec = catalog.iter().find(|s| s.matches_kind("webhook")).cloned();
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
            &catalog,
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

        let input_spec = catalog
            .iter()
            .find(|s| s.matches_kind("file-input"))
            .cloned();
        let Some(input_spec) = input_spec else {
            panic!("file-input spec missing");
        };
        assert_eq!(
            insert_on_branch(&mut graph, &target, &input_spec, json!({}), &catalog),
            Err(GraphError::InvalidPort)
        );
    }

    #[test]
    fn duplicate_node_clones_config_and_connect_checked_guards_cycles() {
        let catalog = test_catalog();
        let (mut graph, input, remux) = chain(&catalog);
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

    #[test]
    fn duplicate_subgraph_preserves_internal_edges_only() {
        let catalog = test_catalog();
        let (mut graph, input, remux) = chain(&catalog);
        let Some(output) = graph.outgoing(&remux).next().map(|edge| edge.to.clone()) else {
            panic!("fin absente de la chaîne");
        };
        let selected: HashSet<String> = [remux.clone(), output.clone()].into_iter().collect();

        let id_map = duplicate_subgraph(&mut graph, &selected);
        assert_eq!(id_map.len(), 2);
        let new_remux = &id_map[&remux];
        let new_output = &id_map[&output];

        assert!(graph.outgoing(new_remux).any(|edge| edge.to == *new_output));
        assert!(!graph.incoming(new_remux).any(|edge| edge.from == input));
    }
}
