//! Simulation à sec et comparaison de graphes de flux partagées entre l'interface et MCP.

use crate::flows::{
    ConditionConfig, FlowConnection, FlowGraph, FlowNode, OwnedBlockSpec, describe_condition,
    evaluate_condition, find_owned,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};

pub const MAX_SIMULATION_STEPS: usize = 64;

/// Exemples prêts à l'emploi de documents JSON de sonde pour tester un flux.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SimulationPreset {
    StructuredTracks,
    DocumentMetadata,
    Minimal,
}

impl SimulationPreset {
    pub const ALL: [Self; 3] = [
        Self::StructuredTracks,
        Self::DocumentMetadata,
        Self::Minimal,
    ];

    pub const fn label(self) -> &'static str {
        match self {
            Self::StructuredTracks => "Pistes structurées",
            Self::DocumentMetadata => "Document / Livre",
            Self::Minimal => "JSON vide",
        }
    }

    pub fn input(self) -> SimulationInput {
        let (file_name, sample) = match self {
            Self::StructuredTracks => (
                "exemple.bin",
                json!({
                    "format": "container",
                    "tracks": [
                        { "type": "video", "codec": "h264", "properties": { "width": 1920, "height": 1080 } },
                        { "type": "audio", "codec": "aac", "properties": { "language": "fre" } }
                    ]
                }),
            ),
            Self::DocumentMetadata => (
                "document.epub",
                json!({
                    "format": "epub",
                    "pages": 320,
                    "metadata": { "language": "fre", "author": "Exemple" }
                }),
            ),
            Self::Minimal => ("fichier.dat", json!({})),
        };
        SimulationInput {
            file_name: file_name.to_owned(),
            probe_json: serde_json::to_string_pretty(&sample).unwrap_or_else(|_| "{}".to_owned()),
        }
    }
}

/// Métadonnées du dernier vrai job d'un flux, proposées comme échantillon de simulation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlowMediaSample {
    pub job_id: uuid::Uuid,
    pub source_name: String,
    pub media_info: serde_json::Value,
    pub recorded_at: String,
}

impl FlowMediaSample {
    /// Taille maximale des métadonnées conservées (JSON sérialisé) ; au-delà, rien n'est enregistré.
    pub const MAX_BYTES: usize = 256 * 1024;

    /// Entrée de simulation correspondante (nom du fichier et métadonnées en JSON lisible).
    pub fn to_simulation_input(&self) -> SimulationInput {
        SimulationInput {
            file_name: self.source_name.clone(),
            probe_json: serde_json::to_string_pretty(&self.media_info).unwrap_or_default(),
        }
    }
}

/// Entrée fictive utilisée pour simuler le routage d'un flux sur un document de sonde quelconque.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SimulationInput {
    pub file_name: String,
    pub probe_json: String,
}

impl Default for SimulationInput {
    fn default() -> Self {
        SimulationPreset::StructuredTracks.input()
    }
}

impl SimulationInput {
    /// Construit un arbre JSON de sonde utilisable par `forgearr_shared::flows::evaluate_condition`.
    pub fn to_probe_value(&self) -> Result<Value, String> {
        let mut parsed: Value = serde_json::from_str(&self.probe_json)
            .map_err(|err| format!("JSON de sonde invalide : {err}"))?;
        if let Some(obj) = parsed.as_object_mut()
            && !self.file_name.trim().is_empty()
            && !obj.contains_key("file_name")
        {
            obj.insert(
                "file_name".to_owned(),
                Value::String(self.file_name.trim().to_owned()),
            );
        }
        Ok(parsed)
    }
}

/// Statut final d'une simulation de flux.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SimulationStatus {
    Complete,
    IncompletePath,
    CycleDetected,
    NoEntry,
}

impl SimulationStatus {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Complete => "Chemin complet",
            Self::IncompletePath => "Impasse (liaison manquante)",
            Self::CycleDetected => "Boucle détectée",
            Self::NoEntry => "Aucun point d'entrée",
        }
    }
}

/// Une étape traversée pendant la simulation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SimulationStep {
    pub index: usize,
    pub node_id: String,
    pub node_name: String,
    pub block_kind: String,
    pub detail: String,
    pub port_taken: Option<String>,
    pub next_edge: Option<FlowConnection>,
}

/// Résultat complet d'une simulation de graphe.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SimulationReport {
    pub status: SimulationStatus,
    pub steps: Vec<SimulationStep>,
    pub warnings: Vec<String>,
}

impl SimulationReport {
    pub fn visited_nodes(&self, limit: Option<usize>) -> HashSet<String> {
        let take = limit.map_or(self.steps.len(), |idx| (idx + 1).min(self.steps.len()));
        self.steps
            .iter()
            .take(take)
            .map(|s| s.node_id.clone())
            .collect()
    }

    pub fn active_node(&self, cursor: Option<usize>) -> Option<&str> {
        match cursor {
            Some(idx) => self.steps.get(idx).map(|s| s.node_id.as_str()),
            None => self.steps.last().map(|s| s.node_id.as_str()),
        }
    }

    pub fn edge_is_active(&self, edge: &FlowConnection, cursor: Option<usize>) -> bool {
        let limit = cursor.map_or(self.steps.len(), |idx| idx.min(self.steps.len()));
        self.steps
            .iter()
            .take(limit)
            .any(|step| step.next_edge.as_ref() == Some(edge))
    }
}

fn is_condition_node(node: &FlowNode, spec: Option<&OwnedBlockSpec>) -> bool {
    let has_branch_ports = spec.is_some_and(|s| s.ports.len() >= 2);
    has_branch_ports
        || node.kind.as_str().ends_with("condition")
        || node.config.get("group").is_some()
        || (node.config.get("property").is_some()
            && node.config.get("operator").is_some()
            && node.config.get("target").is_some())
}

fn evaluate_node_condition(node: &FlowNode, probe_info: &Value) -> (bool, String) {
    let config: ConditionConfig = serde_json::from_value(node.config.clone()).unwrap_or_default();
    let result = evaluate_condition(&config, probe_info);
    let verdict = if result { "Vrai" } else { "Faux" };
    (
        result,
        format!("{} → {verdict}", describe_condition(&config)),
    )
}

fn find_entry_node<'a>(graph: &'a FlowGraph, catalog: &[OwnedBlockSpec]) -> Option<&'a FlowNode> {
    if let Some(input_node) = graph.nodes.iter().find(|node| {
        let spec = find_owned(catalog, &node.kind);
        spec.is_some_and(|s| !s.has_input)
            || node.kind.as_str() == "file-input"
            || node.kind.as_str() == "essentiel.file-input"
    }) {
        return Some(input_node);
    }
    graph
        .nodes
        .iter()
        .find(|node| graph.incoming(&node.id).next().is_none())
        .or_else(|| graph.nodes.first())
}

fn pick_outgoing<'a>(
    graph: &'a FlowGraph,
    node_id: &str,
    port: Option<&str>,
) -> Option<&'a FlowConnection> {
    match port {
        Some(desired) => graph
            .outgoing(node_id)
            .find(|conn| conn.port.as_deref() == Some(desired)),
        None => graph
            .outgoing(node_id)
            .find(|conn| conn.port.is_none())
            .or_else(|| graph.outgoing(node_id).next()),
    }
}

/// Exécute une simulation déterministe du `graph` avec mise en forme optionnelle du résumé d'étape et de port.
pub fn simulate_flow_with(
    graph: &FlowGraph,
    catalog: &[OwnedBlockSpec],
    input: &SimulationInput,
    summarize_node: &dyn Fn(&FlowNode, Option<&OwnedBlockSpec>) -> Option<String>,
    port_label: &dyn Fn(&str) -> String,
) -> SimulationReport {
    let Some(entry) = find_entry_node(graph, catalog) else {
        return SimulationReport {
            status: SimulationStatus::NoEntry,
            steps: Vec::new(),
            warnings: vec!["Le flux ne contient aucun bloc à simuler.".to_owned()],
        };
    };

    let mut visited = HashSet::new();
    let mut steps = Vec::new();
    let mut warnings = Vec::new();
    let probe_info = match input.to_probe_value() {
        Ok(value) => value,
        Err(err) => {
            warnings.push(err);
            json!({ "file_name": input.file_name.trim() })
        }
    };
    let mut current_id = Some(entry.id.clone());

    while let Some(node_id) = current_id {
        if steps.len() >= MAX_SIMULATION_STEPS || !visited.insert(node_id.clone()) {
            warnings.push(format!(
                "Boucle ou limite atteinte sur le bloc « {node_id} »."
            ));
            return SimulationReport {
                status: SimulationStatus::CycleDetected,
                steps,
                warnings,
            };
        }

        let Some(node) = graph.node(&node_id) else {
            warnings.push(format!("Bloc introuvable : {node_id}"));
            return SimulationReport {
                status: SimulationStatus::IncompletePath,
                steps,
                warnings,
            };
        };

        let spec = find_owned(catalog, &node.kind);
        if spec.is_some_and(|s| !s.missing_fields(&node.config).is_empty()) {
            warnings.push(format!(
                "« {} » a au moins un paramètre requis non renseigné.",
                node.name
            ));
        }

        let (selected_port, detail) = if is_condition_node(node, spec) {
            let (matched, note) = evaluate_node_condition(node, &probe_info);
            let port_key = if matched { "true" } else { "false" };
            (Some(port_key.to_owned()), note)
        } else {
            let summary = summarize_node(node, spec).unwrap_or_else(|| "Étape exécutée".to_owned());
            (None, summary)
        };

        let is_terminal = spec.is_some_and(|s| s.terminal)
            || node.kind.as_str() == "file-output"
            || node.kind.as_str() == "essentiel.file-output";

        let next_edge = if is_terminal {
            None
        } else {
            pick_outgoing(graph, &node.id, selected_port.as_deref()).cloned()
        };

        let next_id = next_edge.as_ref().map(|e| e.to.clone());
        let index = steps.len();
        steps.push(SimulationStep {
            index,
            node_id: node.id.clone(),
            node_name: node.name.clone(),
            block_kind: node.kind.as_str().to_owned(),
            detail,
            port_taken: selected_port.clone(),
            next_edge,
        });

        if is_terminal {
            return SimulationReport {
                status: SimulationStatus::Complete,
                steps,
                warnings,
            };
        }

        if next_id.is_none() {
            let port_note = selected_port
                .as_deref()
                .map(|p| format!(" sur la sortie « {} »", port_label(p)))
                .unwrap_or_default();
            warnings.push(format!(
                "Aucune liaison sortante depuis « {} »{}.",
                node.name, port_note
            ));
            return SimulationReport {
                status: SimulationStatus::IncompletePath,
                steps,
                warnings,
            };
        }
        current_id = next_id;
    }

    SimulationReport {
        status: SimulationStatus::Complete,
        steps,
        warnings,
    }
}

/// Simulation par défaut (sans mise en forme spécifique UI).
pub fn simulate_flow_basic(
    graph: &FlowGraph,
    catalog: &[OwnedBlockSpec],
    input: &SimulationInput,
) -> SimulationReport {
    simulate_flow_with(
        graph,
        catalog,
        input,
        &|node, spec| {
            spec.map(|s| s.label.clone())
                .or_else(|| Some(node.kind.as_str().to_owned()))
        },
        &|port| port.to_owned(),
    )
}

/// Écart structurel entre deux graphes de flux (nœuds et connexions).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlowNodeDiff {
    pub id: String,
    pub name: String,
    pub block_kind: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlowNodeModification {
    pub id: String,
    pub old_name: String,
    pub new_name: String,
    pub old_kind: String,
    pub new_kind: String,
    pub config_changed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlowGraphDiff {
    pub added_nodes: Vec<FlowNodeDiff>,
    pub removed_nodes: Vec<FlowNodeDiff>,
    pub modified_nodes: Vec<FlowNodeModification>,
    pub added_edges: Vec<FlowConnection>,
    pub removed_edges: Vec<FlowConnection>,
}

/// Compare deux `FlowGraph` et résume les nœuds/liaisons ajoutés, retirés ou modifiés.
pub fn diff_flow_graphs(before: &FlowGraph, after: &FlowGraph) -> FlowGraphDiff {
    let before_nodes: HashMap<&str, &FlowNode> =
        before.nodes.iter().map(|n| (n.id.as_str(), n)).collect();
    let after_nodes: HashMap<&str, &FlowNode> =
        after.nodes.iter().map(|n| (n.id.as_str(), n)).collect();

    let added_nodes = after
        .nodes
        .iter()
        .filter(|n| !before_nodes.contains_key(n.id.as_str()))
        .map(|n| FlowNodeDiff {
            id: n.id.clone(),
            name: n.name.clone(),
            block_kind: n.kind.as_str().to_owned(),
        })
        .collect();

    let removed_nodes = before
        .nodes
        .iter()
        .filter(|n| !after_nodes.contains_key(n.id.as_str()))
        .map(|n| FlowNodeDiff {
            id: n.id.clone(),
            name: n.name.clone(),
            block_kind: n.kind.as_str().to_owned(),
        })
        .collect();

    let mut modified_nodes = Vec::new();
    for node in &after.nodes {
        let Some(prev) = before_nodes.get(node.id.as_str()) else {
            continue;
        };
        let config_changed = prev.config != node.config;
        if prev.name != node.name || prev.kind != node.kind || config_changed {
            modified_nodes.push(FlowNodeModification {
                id: node.id.clone(),
                old_name: prev.name.clone(),
                new_name: node.name.clone(),
                old_kind: prev.kind.as_str().to_owned(),
                new_kind: node.kind.as_str().to_owned(),
                config_changed,
            });
        }
    }

    fn edge_key(c: &FlowConnection) -> (&str, &str, Option<&str>) {
        (c.from.as_str(), c.to.as_str(), c.port.as_deref())
    }
    let before_edges: HashSet<(&str, &str, Option<&str>)> =
        before.connections.iter().map(edge_key).collect();
    let after_edges: HashSet<(&str, &str, Option<&str>)> =
        after.connections.iter().map(edge_key).collect();

    let added_edges = after
        .connections
        .iter()
        .filter(|e| !before_edges.contains(&edge_key(e)))
        .cloned()
        .collect();
    let removed_edges = before
        .connections
        .iter()
        .filter(|e| !after_edges.contains(&edge_key(e)))
        .cloned()
        .collect();

    FlowGraphDiff {
        added_nodes,
        removed_nodes,
        modified_nodes,
        added_edges,
        removed_edges,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flows::FlowBlockKind;
    use serde_json::Map;

    fn node(id: &str, kind: &str, name: &str, config: Value) -> FlowNode {
        FlowNode {
            id: id.to_owned(),
            kind: FlowBlockKind::parse(kind),
            name: name.to_owned(),
            config,
            extra: Map::new(),
        }
    }

    #[test]
    fn simulate_flow_basic_traces_condition_branch_and_diff_detects_edits() {
        let g1 = FlowGraph {
            nodes: vec![
                node("n1", "file-input", "Entrée", json!({})),
                node(
                    "n2",
                    "condition",
                    "Conteneur ?",
                    json!({ "property": "format", "operator": "eq", "target": "container" }),
                ),
                node("n3", "file-output", "Sortie", json!({})),
            ],
            connections: vec![
                FlowConnection {
                    from: "n1".into(),
                    to: "n2".into(),
                    port: None,
                },
                FlowConnection {
                    from: "n2".into(),
                    to: "n3".into(),
                    port: Some("true".into()),
                },
            ],
            extra: Map::new(),
        };
        let tracks_input = SimulationPreset::StructuredTracks.input();
        let report_true = simulate_flow_basic(&g1, &[], &tracks_input);
        assert_eq!(report_true.status, SimulationStatus::Complete);
        assert_eq!(report_true.steps.len(), 3);

        let report_false = simulate_flow_basic(&g1, &[], &SimulationPreset::Minimal.input());
        assert_eq!(report_false.status, SimulationStatus::IncompletePath);

        let mut g2 = g1.clone();
        g2.nodes
            .push(node("n4", "video.encode", "AV1", json!({ "codec": "av1" })));
        let diff = diff_flow_graphs(&g1, &g2);
        assert_eq!(diff.added_nodes.len(), 1);
        assert_eq!(diff.added_nodes[0].id, "n4");
        assert!(diff.removed_nodes.is_empty());
    }

    #[test]
    fn simulate_flow_basic_evaluates_grouped_condition() {
        let group = json!({ "group": { "combinator": "any", "rules": [
            { "property": "format", "operator": "exact", "target": "pdf" },
            { "property": "tracks[type=video].codec", "operator": "in", "target": "h264, hevc" }
        ] } });
        let graph = FlowGraph {
            nodes: vec![
                node("n1", "file-input", "Entrée", json!({})),
                node("n2", "essentiel.filtre", "Filtre", group),
                node("n3", "file-output", "Sortie", json!({})),
            ],
            connections: vec![
                FlowConnection {
                    from: "n1".into(),
                    to: "n2".into(),
                    port: None,
                },
                FlowConnection {
                    from: "n2".into(),
                    to: "n3".into(),
                    port: Some("true".into()),
                },
            ],
            extra: Map::new(),
        };
        let report = simulate_flow_basic(&graph, &[], &SimulationPreset::StructuredTracks.input());
        assert_eq!(report.status, SimulationStatus::Complete);
        let step = &report.steps[1];
        assert_eq!(step.port_taken.as_deref(), Some("true"));
        assert_eq!(
            step.detail,
            "(format = pdf OU tracks[type=video].codec ∈ h264, hevc) → Vrai"
        );
    }
}
