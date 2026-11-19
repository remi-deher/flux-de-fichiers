//! Rendu visuel des modèles de flux du catalogue : mini-chaîne sur les cartes et diagramme en couches dans le tiroir.

use crate::flows::NodeDomain;
use crate::ui::{Icon, Tag, TagTone};
use forgearr_shared::flows::FlowGraph;
use forgearr_shared::jobs::JobStepLog;
use forgearr_shared::plugins::{CatalogFlowEdge, CatalogFlowNode, CatalogFlowPreview};
use leptos::prelude::*;
use std::collections::{HashMap, HashSet, VecDeque};

const MINI_MAX_NODES: usize = 4;
const MINI_ICON_SIZE: u32 = 12;
const DIAGRAM_ICON_SIZE: u32 = 15;

/// Convertit la définition JSON d'un `FlowRecord` ou d'un `LocalFlowTemplate` en `CatalogFlowPreview`.
pub fn preview_from_definition_json(raw: &str) -> Option<CatalogFlowPreview> {
    let graph = FlowGraph::parse(raw);
    if graph.nodes.is_empty() {
        return None;
    }
    let nodes = graph
        .nodes
        .into_iter()
        .map(|node| {
            let kind = node.kind.to_string();
            let name = if node.name.trim().is_empty() {
                kind.clone()
            } else {
                node.name
            };
            CatalogFlowNode {
                id: node.id,
                kind,
                name,
                summary: Vec::new(),
            }
        })
        .collect();
    let connections = graph
        .connections
        .into_iter()
        .map(|edge| CatalogFlowEdge {
            from: edge.from,
            to: edge.to,
            port: edge.port,
        })
        .collect();
    Some(CatalogFlowPreview {
        file_mask: None,
        nodes,
        connections,
    })
}

/// Convertit les étapes exécutées d'une tâche (`JobStepLog`) en chaîne `CatalogFlowPreview`.
pub fn preview_from_job_steps(steps: &[JobStepLog]) -> Option<CatalogFlowPreview> {
    if steps.is_empty() {
        return None;
    }
    let nodes: Vec<CatalogFlowNode> = steps
        .iter()
        .map(|step| {
            let name = step
                .operation_id
                .as_deref()
                .filter(|s| !s.trim().is_empty())
                .unwrap_or(&step.block_kind)
                .to_owned();
            CatalogFlowNode {
                id: format!("step-{}", step.step_index),
                kind: step.block_kind.clone(),
                name,
                summary: Vec::new(),
            }
        })
        .collect();
    let connections: Vec<CatalogFlowEdge> = nodes
        .windows(2)
        .map(|pair| CatalogFlowEdge {
            from: pair[0].id.clone(),
            to: pair[1].id.clone(),
            port: None,
        })
        .collect();
    Some(CatalogFlowPreview {
        file_mask: None,
        nodes,
        connections,
    })
}

/// Déduit le domaine visuel Forgearr d'un bloc de flux à partir de son type (`file-input`, `condition`, etc.).
pub fn domain_for_node_kind(kind: &str) -> NodeDomain {
    let lower = kind.to_lowercase();
    if lower.contains("input") || lower.contains("source") {
        NodeDomain::Input
    } else if lower.contains("output") || lower.contains("publish") || lower.contains("dest") {
        NodeDomain::Output
    } else if lower.contains("condition") || lower.contains("branch") || lower.contains("if") {
        NodeDomain::Logic
    } else if lower.contains("probe") || lower.contains("inspect") || lower.contains("mediainfo") {
        NodeDomain::Inspect
    } else if lower.contains("sonarr")
        || lower.contains("radarr")
        || lower.contains("plex")
        || lower.contains("jellyfin")
        || lower.contains("notify")
        || lower.contains("webhook")
    {
        NodeDomain::Integration
    } else {
        NodeDomain::Process
    }
}

/// Libellé lisible d'un port de branchement (`true` → `Vrai`, `false` → `Faux`).
pub fn port_label(port: &str) -> String {
    match port.to_lowercase().as_str() {
        "true" | "yes" | "oui" => "Vrai".to_owned(),
        "false" | "no" | "non" => "Faux".to_owned(),
        "error" | "err" => "Erreur".to_owned(),
        _ => port.to_owned(),
    }
}

/// Organise les nœuds d'un flux en couches topologiques pour afficher les branches en parallèle.
pub fn layer_flow_nodes(preview: &CatalogFlowPreview) -> Vec<Vec<CatalogFlowNode>> {
    if preview.nodes.is_empty() {
        return Vec::new();
    }
    let by_id: HashMap<&str, &CatalogFlowNode> = preview
        .nodes
        .iter()
        .map(|node| (node.id.as_str(), node))
        .collect();
    let mut in_degree: HashMap<&str, usize> =
        preview.nodes.iter().map(|n| (n.id.as_str(), 0)).collect();
    let mut outgoing: HashMap<&str, Vec<&str>> = HashMap::new();

    for edge in &preview.connections {
        if by_id.contains_key(edge.from.as_str()) && by_id.contains_key(edge.to.as_str()) {
            outgoing
                .entry(edge.from.as_str())
                .or_default()
                .push(edge.to.as_str());
            if let Some(deg) = in_degree.get_mut(edge.to.as_str()) {
                *deg += 1;
            }
        }
    }

    let mut depth: HashMap<&str, usize> = HashMap::new();
    let mut queue: VecDeque<&str> = VecDeque::new();
    for node in &preview.nodes {
        if in_degree.get(node.id.as_str()).copied().unwrap_or(0) == 0 {
            depth.insert(node.id.as_str(), 0);
            queue.push_back(node.id.as_str());
        }
    }
    if queue.is_empty()
        && let Some(first) = preview.nodes.first()
    {
        depth.insert(first.id.as_str(), 0);
        queue.push_back(first.id.as_str());
    }

    let mut visited_edges: HashSet<(&str, &str)> = HashSet::new();
    while let Some(curr) = queue.pop_front() {
        let curr_depth = depth.get(curr).copied().unwrap_or(0);
        if let Some(nexts) = outgoing.get(curr) {
            for &next in nexts {
                if !visited_edges.insert((curr, next)) {
                    continue;
                }
                let next_depth = curr_depth + 1;
                let entry = depth.entry(next).or_insert(0);
                if next_depth > *entry {
                    *entry = next_depth;
                }
                queue.push_back(next);
            }
        }
    }

    let max_depth = depth.values().copied().max().unwrap_or(0);
    let mut layers: Vec<Vec<CatalogFlowNode>> = vec![Vec::new(); max_depth + 1];
    for node in &preview.nodes {
        let layer_idx = depth.get(node.id.as_str()).copied().unwrap_or(0);
        if let Some(layer) = layers.get_mut(layer_idx) {
            layer.push(node.clone());
        }
    }
    layers.retain(|layer| !layer.is_empty());
    layers
}

/// Mini-aperçu compact des blocs d'un flux affiché directement sur sa carte du Catalogue.
#[component]
pub fn FlowMiniPreview(preview: CatalogFlowPreview) -> impl IntoView {
    let total = preview.nodes.len();
    let visible: Vec<CatalogFlowNode> = preview.nodes.into_iter().take(MINI_MAX_NODES).collect();
    let remaining = total.saturating_sub(visible.len());

    view! {
        <div class="flow-mini" aria-label="Aperçu des blocs du flux">
            {visible.into_iter().enumerate().map(|(idx, node)| {
                let domain = domain_for_node_kind(&node.kind);
                let style = format!("--component-accent: {}", domain.accent_var());
                let tooltip = node.name.clone();
                view! {
                    {(idx > 0).then(|| view! {
                        <span class="flow-mini__arrow" aria-hidden="true">"→"</span>
                    })}
                    <span class="flow-mini__node" style=style title=tooltip>
                        <Icon name=domain.icon() size=MINI_ICON_SIZE />
                        <span class="flow-mini__label">{node.name}</span>
                    </span>
                }
            }).collect_view()}
            {(remaining > 0).then(|| view! {
                <span class="flow-mini__more">{format!("+{remaining}")}</span>
            })}
        </div>
    }
}

/// Diagramme visuel en couches d'un flux affiché dans son tiroir d'inspection.
#[component]
pub fn FlowDiagram(preview: CatalogFlowPreview) -> impl IntoView {
    let layers = layer_flow_nodes(&preview);
    let incoming_ports: HashMap<String, Vec<String>> = {
        let mut map: HashMap<String, Vec<String>> = HashMap::new();
        for edge in &preview.connections {
            if let Some(port) = &edge.port {
                map.entry(edge.to.clone())
                    .or_default()
                    .push(port_label(port));
            }
        }
        map
    };

    view! {
        <div class="flow-diagram">
            <div class="flow-diagram__head">
                <h3 class="form-label">"Aperçu du graphe"</h3>
                {preview.file_mask.map(|mask| view! {
                    <Tag label=mask tone=TagTone::Mono />
                })}
            </div>
            <div class="flow-diagram__canvas">
                {layers.into_iter().enumerate().map(|(layer_idx, nodes)| {
                    let is_branch = nodes.len() > 1;
                    view! {
                        {(layer_idx > 0).then(|| view! {
                            <div class="flow-diagram__connector" aria-hidden="true">
                                <span class="flow-diagram__arrow">"↓"</span>
                            </div>
                        })}
                        <div class="flow-diagram__layer" class:is-branch=is_branch>
                            {nodes.into_iter().map(|node| {
                                let domain = domain_for_node_kind(&node.kind);
                                let style = format!("--component-accent: {}", domain.accent_var());
                                let ports = incoming_ports.get(&node.id).cloned().unwrap_or_default();
                                view! {
                                    <div class="flow-diagram__node" style=style>
                                        {(!ports.is_empty()).then(|| view! {
                                            <div class="flow-diagram__ports">
                                                {ports.into_iter().map(|p| view! {
                                                    <span class="flow-diagram__port">{p}</span>
                                                }).collect_view()}
                                            </div>
                                        })}
                                        <div class="flow-diagram__node-head">
                                            <span class="flow-diagram__node-icon" aria-hidden="true">
                                                <Icon name=domain.icon() size=DIAGRAM_ICON_SIZE />
                                            </span>
                                            <div class="flow-diagram__node-titles">
                                                <span class="flow-diagram__node-title">{node.name}</span>
                                                <span class="flow-diagram__node-kind">{domain.label()}" · "{node.kind}</span>
                                            </div>
                                        </div>
                                        {(!node.summary.is_empty()).then(|| view! {
                                            <div class="flow-diagram__chips">
                                                {node.summary.into_iter().map(|chip| view! {
                                                    <Tag label=chip tone=TagTone::Mono />
                                                }).collect_view()}
                                            </div>
                                        })}
                                    </div>
                                }
                            }).collect_view()}
                        </div>
                    }
                }).collect_view()}
            </div>
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use forgearr_shared::plugins::CatalogFlowEdge;

    #[test]
    fn layer_flow_nodes_places_branches_on_same_layer() {
        let preview = CatalogFlowPreview {
            file_mask: Some("*.mkv".into()),
            nodes: vec![
                CatalogFlowNode {
                    id: "n1".into(),
                    kind: "file-input".into(),
                    name: "Source".into(),
                    summary: Vec::new(),
                },
                CatalogFlowNode {
                    id: "n2".into(),
                    kind: "condition".into(),
                    name: "HEVC ?".into(),
                    summary: Vec::new(),
                },
                CatalogFlowNode {
                    id: "n3".into(),
                    kind: "mkvpropedit".into(),
                    name: "In-Place".into(),
                    summary: Vec::new(),
                },
                CatalogFlowNode {
                    id: "n4".into(),
                    kind: "video-encode".into(),
                    name: "Encode".into(),
                    summary: Vec::new(),
                },
                CatalogFlowNode {
                    id: "n5".into(),
                    kind: "file-output".into(),
                    name: "Sortie".into(),
                    summary: Vec::new(),
                },
            ],
            connections: vec![
                CatalogFlowEdge {
                    from: "n1".into(),
                    to: "n2".into(),
                    port: None,
                },
                CatalogFlowEdge {
                    from: "n2".into(),
                    to: "n3".into(),
                    port: Some("true".into()),
                },
                CatalogFlowEdge {
                    from: "n2".into(),
                    to: "n4".into(),
                    port: Some("false".into()),
                },
                CatalogFlowEdge {
                    from: "n3".into(),
                    to: "n5".into(),
                    port: None,
                },
                CatalogFlowEdge {
                    from: "n4".into(),
                    to: "n5".into(),
                    port: None,
                },
            ],
        };
        let layers = layer_flow_nodes(&preview);
        assert_eq!(layers.len(), 4);
        assert_eq!(layers[0].len(), 1);
        assert_eq!(layers[1].len(), 1);
        assert_eq!(layers[2].len(), 2);
        assert_eq!(layers[3].len(), 1);
    }

    #[test]
    fn domain_and_port_helpers_map_common_kinds() {
        assert_eq!(domain_for_node_kind("file-input"), NodeDomain::Input);
        assert_eq!(domain_for_node_kind("condition"), NodeDomain::Logic);
        assert_eq!(domain_for_node_kind("video-encode"), NodeDomain::Process);
        assert_eq!(domain_for_node_kind("file-output"), NodeDomain::Output);
        assert_eq!(port_label("true"), "Vrai");
        assert_eq!(port_label("false"), "Faux");
    }

    #[test]
    fn preview_from_definition_json_parses_flow_graph() {
        let def = r#"{"nodes":[{"id":"a","type":"file-input","name":"Source"},{"id":"b","type":"file-output","name":"Sortie"}],"connections":[{"from":"a","to":"b"}]}"#;
        let preview = preview_from_definition_json(def);
        assert!(preview.is_some());
        if let Some(p) = preview {
            assert_eq!(p.nodes.len(), 2);
            assert_eq!(p.connections.len(), 1);
        }
        assert!(preview_from_definition_json("{}").is_none());
    }
}
