//! Opérations d'édition du graphe et disposition en couches pour l'affichage.

use super::{FlowBlockKind, FlowConnection, FlowGraph, FlowNode, OwnedBlockSpec, find_owned};
use serde_json::{Map, Value};
use std::collections::{HashMap, HashSet, VecDeque};

/// Refus d'une modification du graphe.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GraphError {
    UnknownNode,
    SelfLink,
    /// Cette sortie est déjà reliée (une sortie mène à un seul bloc).
    PortTaken,
    /// La liaison fermerait une boucle.
    Cycle,
    /// Le bloc de départ n'a pas cette sortie, ou le bloc d'arrivée pas d'entrée.
    InvalidPort,
}

impl GraphError {
    pub fn message(&self) -> &'static str {
        match self {
            Self::UnknownNode => "Bloc introuvable.",
            Self::SelfLink => "Un bloc ne peut pas se relier à lui-même.",
            Self::PortTaken => "Cette sortie est déjà reliée.",
            Self::Cycle => "Cette liaison fermerait une boucle.",
            Self::InvalidPort => "Ces blocs ne peuvent pas être reliés ainsi.",
        }
    }
}

/// Place d'un bloc : `layer` = profondeur depuis l'entrée, `slot` = rang dans la couche.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GraphPosition {
    pub layer: usize,
    pub slot: usize,
}

impl FlowGraph {
    pub fn node(&self, id: &str) -> Option<&FlowNode> {
        self.nodes.iter().find(|node| node.id == id)
    }

    pub fn node_mut(&mut self, id: &str) -> Option<&mut FlowNode> {
        self.nodes.iter_mut().find(|node| node.id == id)
    }

    pub fn outgoing(&self, id: &str) -> impl Iterator<Item = &FlowConnection> {
        self.connections.iter().filter(move |edge| edge.from == id)
    }

    pub fn incoming(&self, id: &str) -> impl Iterator<Item = &FlowConnection> {
        self.connections.iter().filter(move |edge| edge.to == id)
    }

    /// Sorties d'un bloc : celles de sa fiche plugin, sinon une sortie simple (`None`),
    /// plus toute sortie nommée déjà utilisée par une liaison enregistrée.
    pub fn output_ports(&self, node: &FlowNode, catalog: &[OwnedBlockSpec]) -> Vec<Option<String>> {
        let spec = find_owned(catalog, &node.kind);
        let mut ports: Vec<Option<String>> = match spec {
            Some(spec) if spec.terminal => Vec::new(),
            Some(spec) if !spec.ports.is_empty() => spec
                .ports
                .iter()
                .map(|(port, _)| Some(port.clone()))
                .collect(),
            _ => vec![None],
        };
        for edge in self.outgoing(&node.id) {
            if !ports.contains(&edge.port) {
                ports.push(edge.port.clone());
            }
        }
        ports
    }

    fn next_id(&self) -> String {
        let used: HashSet<&str> = self.nodes.iter().map(|node| node.id.as_str()).collect();
        (self.nodes.len() + 1..)
            .map(|index| format!("node-{index}"))
            .find(|id| !used.contains(id.as_str()))
            .unwrap_or_default()
    }

    /// Ajoute un bloc et le relie au dernier bloc qui a encore une sortie simple libre.
    pub fn append(
        &mut self,
        kind: FlowBlockKind,
        name: &str,
        config: Value,
        catalog: &[OwnedBlockSpec],
    ) -> String {
        let id = self.next_id();
        let tail = self
            .nodes
            .iter()
            .rev()
            .find(|node| {
                self.output_ports(node, catalog).contains(&None)
                    && self.outgoing(&node.id).next().is_none()
            })
            .map(|node| node.id.clone());
        let accepts_input = find_owned(catalog, &kind).is_none_or(|spec| spec.has_input);
        self.nodes.push(FlowNode {
            id: id.clone(),
            kind,
            name: name.to_owned(),
            config,
            extra: Map::new(),
        });
        if let (Some(from), true) = (tail, accepts_input) {
            self.connections.push(FlowConnection {
                from,
                to: id.clone(),
                port: None,
            });
        }
        id
    }

    /// Retire un bloc et ses liaisons ; un maillon simple (une entrée, une sortie simple) est raccordé.
    pub fn remove(&mut self, id: &str) {
        let incoming: Vec<FlowConnection> = self.incoming(id).cloned().collect();
        let outgoing: Vec<FlowConnection> = self.outgoing(id).cloned().collect();
        self.nodes.retain(|node| node.id != id);
        self.connections
            .retain(|edge| edge.from != id && edge.to != id);
        if let ([before], [after]) = (incoming.as_slice(), outgoing.as_slice())
            && after.port.is_none()
        {
            self.connections.push(FlowConnection {
                from: before.from.clone(),
                to: after.to.clone(),
                port: before.port.clone(),
            });
        }
    }

    fn reaches(&self, from: &str, target: &str) -> bool {
        let mut seen = HashSet::new();
        let mut queue = VecDeque::from([from]);
        while let Some(current) = queue.pop_front() {
            if current == target {
                return true;
            }
            if seen.insert(current) {
                queue.extend(self.outgoing(current).map(|edge| edge.to.as_str()));
            }
        }
        false
    }

    /// Relie la sortie `port` de `from` à l'entrée de `to`.
    pub fn connect(
        &mut self,
        from: &str,
        port: Option<String>,
        to: &str,
        catalog: &[OwnedBlockSpec],
    ) -> Result<(), GraphError> {
        let (Some(source), Some(target)) = (self.node(from), self.node(to)) else {
            return Err(GraphError::UnknownNode);
        };
        if from == to {
            return Err(GraphError::SelfLink);
        }
        let target_has_input = find_owned(catalog, &target.kind).is_none_or(|spec| spec.has_input);
        if !self.output_ports(source, catalog).contains(&port) || !target_has_input {
            return Err(GraphError::InvalidPort);
        }
        if self.outgoing(from).any(|edge| edge.port == port) {
            return Err(GraphError::PortTaken);
        }
        if self.reaches(to, from) {
            return Err(GraphError::Cycle);
        }
        self.connections.push(FlowConnection {
            from: from.to_owned(),
            to: to.to_owned(),
            port,
        });
        Ok(())
    }

    /// Retire la liaison qui part de la sortie `port` de `from`.
    pub fn disconnect(&mut self, from: &str, port: &Option<String>) {
        self.connections
            .retain(|edge| !(edge.from == from && &edge.port == port));
    }
}

/// Disposition en couches : chaque bloc est placé une couche sous son plus long chemin
/// depuis les blocs sans entrée ; les blocs d'une couche gardent l'ordre du graphe.
pub fn layout(graph: &FlowGraph) -> HashMap<String, GraphPosition> {
    let mut depth: HashMap<&str, usize> = HashMap::new();
    let mut indegree: HashMap<&str, usize> = graph
        .nodes
        .iter()
        .map(|node| (node.id.as_str(), graph.incoming(&node.id).count()))
        .collect();
    let mut queue: VecDeque<&str> = graph
        .nodes
        .iter()
        .filter(|node| indegree.get(node.id.as_str()) == Some(&0))
        .map(|node| node.id.as_str())
        .collect();
    while let Some(current) = queue.pop_front() {
        let current_depth = *depth.entry(current).or_insert(0);
        for edge in graph.outgoing(current) {
            let next = edge.to.as_str();
            let entry = depth.entry(next).or_insert(0);
            *entry = (*entry).max(current_depth + 1);
            if let Some(remaining) = indegree.get_mut(next) {
                *remaining = remaining.saturating_sub(1);
                if *remaining == 0 {
                    queue.push_back(next);
                }
            }
        }
    }
    let mut slots: HashMap<usize, usize> = HashMap::new();
    graph
        .nodes
        .iter()
        .map(|node| {
            // Un bloc pris dans une boucle (définition invalide) reste visible en couche 0.
            let layer = depth.get(node.id.as_str()).copied().unwrap_or(0);
            let slot = slots.entry(layer).or_insert(0);
            let position = GraphPosition { layer, slot: *slot };
            *slot += 1;
            (node.id.clone(), position)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn spec(kind: &str, has_input: bool, ports: &[&str], terminal: bool) -> OwnedBlockSpec {
        OwnedBlockSpec {
            kind: kind.into(),
            aliases: Vec::new(),
            label: kind.into(),
            category: "Test".into(),
            domain: None,
            description: String::new(),
            has_input,
            ports: ports
                .iter()
                .map(|port| ((*port).to_owned(), (*port).to_owned()))
                .collect(),
            terminal,
            fields: Vec::new(),
            suggested_next: Vec::new(),
            from_plugin: true,
            operation_id: None,
        }
    }

    fn test_catalog() -> Vec<OwnedBlockSpec> {
        vec![
            spec("block.core.file-input", false, &[], false),
            spec("block.core.condition", true, &["true", "false"], false),
            spec("block.video.remux-mkv", true, &[], false),
            spec("block.core.webhook", true, &[], false),
            spec("block.core.file-output", true, &[], true),
        ]
    }

    fn chain(catalog: &[OwnedBlockSpec]) -> FlowGraph {
        let mut graph = FlowGraph::default();
        graph.append("block.core.file-input".into(), "Entrée", json!({}), catalog);
        graph.append("block.core.condition".into(), "HEVC ?", json!({}), catalog);
        graph
    }

    #[test]
    fn append_links_to_the_last_free_simple_output() {
        let catalog = test_catalog();
        let mut graph = chain(&catalog);
        assert_eq!(
            graph.connections,
            vec![FlowConnection {
                from: "node-1".into(),
                to: "node-2".into(),
                port: None,
            }]
        );
        // La condition n'a que des sorties nommées : le bloc suivant n'est pas relié d'office.
        let after = graph.append("block.video.remux-mkv".into(), "Remux", json!({}), &catalog);
        assert_eq!(graph.incoming(&after).count(), 0);
        assert_eq!(after, "node-3");
    }

    #[test]
    fn connect_checks_ports_duplicates_and_cycles() {
        let catalog = test_catalog();
        let mut graph = chain(&catalog);
        let yes = graph.append("block.video.remux-mkv".into(), "Oui", json!({}), &catalog);
        let no = graph.append("block.core.file-output".into(), "Fin", json!({}), &catalog);
        assert_eq!(
            graph.connect("node-2", Some("true".into()), &yes, &catalog),
            Ok(())
        );
        assert_eq!(
            graph.connect("node-2", Some("true".into()), &no, &catalog),
            Err(GraphError::PortTaken)
        );
        assert_eq!(
            graph.connect("node-2", None, &no, &catalog),
            Err(GraphError::InvalidPort)
        );
        assert_eq!(
            graph.connect(&yes, None, "node-1", &catalog),
            Err(GraphError::InvalidPort)
        );
        assert_eq!(
            graph.connect(&yes, None, &yes, &catalog),
            Err(GraphError::SelfLink)
        );
        assert_eq!(
            graph.connect(&yes, None, "absent", &catalog),
            Err(GraphError::UnknownNode)
        );
        assert_eq!(
            graph.connect(&no, None, &yes, &catalog),
            Err(GraphError::InvalidPort)
        );
        let mut looped = chain(&catalog);
        let a = looped.append("block.video.remux-mkv".into(), "A", json!({}), &catalog);
        assert_eq!(
            looped.connect("node-2", Some("false".into()), &a, &catalog),
            Ok(())
        );
        let b = looped.append("block.video.remux-mkv".into(), "B", json!({}), &catalog);
        assert_eq!(
            looped.connect(&b, None, "node-2", &catalog),
            Err(GraphError::Cycle)
        );
        graph.disconnect("node-2", &Some("true".into()));
        assert_eq!(
            graph.connect("node-2", Some("true".into()), &no, &catalog),
            Ok(())
        );
    }

    #[test]
    fn remove_bridges_a_simple_link_and_keeps_the_port() {
        let catalog = test_catalog();
        let mut graph = chain(&catalog);
        let middle = graph.append("block.video.remux-mkv".into(), "Remux", json!({}), &catalog);
        assert_eq!(
            graph.connect("node-2", Some("false".into()), &middle, &catalog),
            Ok(())
        );
        let end = graph.append("block.core.file-output".into(), "Fin", json!({}), &catalog);
        graph.remove(&middle);
        assert_eq!(
            graph
                .incoming(&end)
                .next()
                .map(|edge| (edge.from.as_str(), edge.port.clone())),
            Some(("node-2", Some("false".into())))
        );
        assert!(graph.node(&middle).is_none());
    }

    fn edge(from: &str, port: Option<&str>, to: &str) -> FlowConnection {
        FlowConnection {
            from: from.into(),
            to: to.into(),
            port: port.map(str::to_owned),
        }
    }

    #[test]
    fn layout_places_blocks_by_longest_path() {
        let catalog = test_catalog();
        let mut graph = chain(&catalog);
        for (kind, name) in [
            ("block.video.remux-mkv", "Oui"),
            ("block.core.webhook", "Non"),
            ("block.core.file-output", "Fin"),
        ] {
            graph.append(kind.into(), name, json!({}), &catalog);
        }
        graph.connections = vec![
            edge("node-1", None, "node-2"),
            edge("node-2", Some("true"), "node-3"),
            edge("node-2", Some("false"), "node-4"),
            edge("node-4", None, "node-5"),
            edge("node-3", None, "node-5"),
        ];
        let places = layout(&graph);
        assert_eq!(places["node-1"], GraphPosition { layer: 0, slot: 0 });
        assert_eq!(places["node-2"].layer, 1);
        assert_eq!(places["node-3"], GraphPosition { layer: 2, slot: 0 });
        assert_eq!(places["node-4"], GraphPosition { layer: 2, slot: 1 });
        assert_eq!(places["node-5"].layer, 3);
    }

    #[test]
    fn output_ports_follow_the_spec_and_keep_stored_ones() {
        let catalog = test_catalog();
        let graph = chain(&catalog);
        let condition = graph.node("node-2");
        assert_eq!(
            condition.map(|node| graph.output_ports(node, &catalog)),
            Some(vec![Some("true".into()), Some("false".into())])
        );
        let mut legacy = chain(&catalog);
        legacy.connections.push(FlowConnection {
            from: "node-2".into(),
            to: "node-1".into(),
            port: Some("fallback".into()),
        });
        let ports = legacy
            .node("node-2")
            .map(|node| legacy.output_ports(node, &catalog));
        assert_eq!(ports.map(|ports| ports.len()), Some(3));
    }
}
