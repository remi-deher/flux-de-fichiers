//! Squelettes de départ : graphes minimaux construits depuis le catalogue de blocs.

use forgearr_shared::flows::{FlowGraph, OwnedBlockSpec};
use serde::{Deserialize, Serialize};

/// Nombre minimal de sorties nommées qui font d'un bloc un aiguillage.
const BRANCH_MIN_PORTS: usize = 2;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Skeleton {
    /// Un bloc d'entrée seul.
    #[default]
    Blank,
    /// Entrée puis sortie.
    InOut,
    /// Entrée, aiguillage, puis une sortie par branche.
    Branch,
}

impl Skeleton {
    pub const ALL: [Self; 3] = [Self::Blank, Self::InOut, Self::Branch];

    pub fn label(self) -> &'static str {
        match self {
            Self::Blank => "Vide",
            Self::InOut => "Entrée → Sortie",
            Self::Branch => "Aiguillage",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Self::Blank => "Un bloc d'entrée, pour tout construire vous-même",
            Self::InOut => "La base de tout traitement : un fichier entre, un fichier sort",
            Self::Branch => "Une condition, puis une sortie par branche",
        }
    }

    /// Nom proposé au flux créé depuis ce squelette.
    pub fn default_name(self) -> &'static str {
        match self {
            Self::Blank => "Nouveau flux",
            Self::InOut | Self::Branch => self.label(),
        }
    }

    /// Graphe du squelette ; un bloc que le catalogue ne fournit pas est simplement omis.
    pub fn build(self, catalog: &[OwnedBlockSpec]) -> FlowGraph {
        let mut graph = FlowGraph::default();
        let Some(entry) = catalog.iter().find(|spec| !spec.has_input) else {
            return graph;
        };
        add_block(&mut graph, entry, catalog);
        match self {
            Self::Blank => {}
            Self::InOut => {
                if let Some(output) = find_output(catalog) {
                    add_block(&mut graph, output, catalog);
                }
            }
            Self::Branch => add_branch(&mut graph, catalog),
        }
        graph
    }
}

fn find_output(catalog: &[OwnedBlockSpec]) -> Option<&OwnedBlockSpec> {
    catalog.iter().find(|spec| spec.terminal && spec.has_input)
}

fn find_branch(catalog: &[OwnedBlockSpec]) -> Option<&OwnedBlockSpec> {
    catalog
        .iter()
        .find(|spec| spec.has_input && !spec.terminal && spec.ports.len() >= BRANCH_MIN_PORTS)
}

fn add_block(graph: &mut FlowGraph, spec: &OwnedBlockSpec, catalog: &[OwnedBlockSpec]) -> String {
    graph.append(spec.kind_enum(), &spec.label, spec.default_config(), catalog)
}

/// Aiguillage relié à l'entrée, puis une sortie au bout de chaque branche.
fn add_branch(graph: &mut FlowGraph, catalog: &[OwnedBlockSpec]) {
    let Some(branch) = find_branch(catalog) else {
        return;
    };
    let branch_id = add_block(graph, branch, catalog);
    let Some(output) = find_output(catalog) else {
        return;
    };
    for (port, _) in &branch.ports {
        let output_id = add_block(graph, output, catalog);
        // Les ports viennent du catalogue et la sortie est neuve : la liaison ne peut pas être refusée.
        let _ = graph.connect(&branch_id, Some(port.clone()), &output_id, catalog);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
            from_plugin: false,
            operation_id: None,
            owner: None,
        }
    }

    fn catalog() -> Vec<OwnedBlockSpec> {
        vec![
            spec("essentiel.file-input", false, &[], false),
            spec("essentiel.condition", true, &["true", "false"], false),
            spec("essentiel.file-output", true, &[], true),
        ]
    }

    fn kinds(graph: &FlowGraph) -> Vec<&str> {
        graph.nodes.iter().map(|node| node.kind.as_str()).collect()
    }

    #[test]
    fn blank_is_the_entry_block_alone() {
        let graph = Skeleton::Blank.build(&catalog());
        assert_eq!(kinds(&graph), vec!["essentiel.file-input"]);
        assert!(graph.connections.is_empty());
    }

    #[test]
    fn in_out_links_the_entry_to_the_output() {
        let graph = Skeleton::InOut.build(&catalog());
        assert_eq!(
            kinds(&graph),
            vec!["essentiel.file-input", "essentiel.file-output"]
        );
        assert_eq!(graph.connections.len(), 1);
    }

    #[test]
    fn branch_has_one_output_per_port() {
        let graph = Skeleton::Branch.build(&catalog());
        assert_eq!(
            kinds(&graph),
            vec![
                "essentiel.file-input",
                "essentiel.condition",
                "essentiel.file-output",
                "essentiel.file-output"
            ]
        );
        let ports: Vec<Option<&str>> = graph
            .connections
            .iter()
            .map(|edge| edge.port.as_deref())
            .collect();
        assert_eq!(ports, vec![None, Some("true"), Some("false")]);
    }

    #[test]
    fn missing_blocks_are_left_out() {
        let only_entry = vec![spec("essentiel.file-input", false, &[], false)];
        assert_eq!(Skeleton::InOut.build(&only_entry).nodes.len(), 1);
        assert_eq!(Skeleton::Branch.build(&only_entry).nodes.len(), 1);
        assert!(Skeleton::Blank.build(&[]).nodes.is_empty());
    }

    #[test]
    fn skeletons_round_trip_through_serde() {
        for skeleton in Skeleton::ALL {
            let text = serde_json::to_string(&skeleton).unwrap_or_default();
            assert_eq!(serde_json::from_str::<Skeleton>(&text).ok(), Some(skeleton));
        }
    }

    #[test]
    fn only_blank_keeps_the_generic_default_name() {
        assert_eq!(Skeleton::Blank.default_name(), "Nouveau flux");
        assert_eq!(Skeleton::Branch.default_name(), Skeleton::Branch.label());
    }
}
