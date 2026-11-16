//! Simulation à sec d'un flux dans l'éditeur sans lancer de tâche réelle.
//!
//! Évalue localement les nœuds `condition` avec le même évaluateur JSON générique
//! que le moteur (`flux_shared::flows::evaluate_condition`) et trace le parcours sur le graphe.

use super::presentation::{node_summary, output_presentation};
use super::state::Editor;
use crate::ui::{Icon, IconName, Tag, TagTone};
use flux_shared::flows::{
    ConditionConfig, FlowConnection, FlowGraph, FlowNode, OwnedBlockSpec, evaluate_condition,
    find_owned,
};
use leptos::prelude::*;
use serde_json::{Value, json};
use std::collections::HashSet;

const MAX_SIMULATION_STEPS: usize = 64;
const JSON_EDITOR_ROWS: u32 = 6;

/// Exemples prêts à l'emploi de documents JSON de sonde pour tester un flux.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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

/// Entrée fictive utilisée pour simuler le routage d'un flux sur un document de sonde quelconque.
#[derive(Clone, Debug, PartialEq, Eq)]
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
    /// Construit un arbre JSON de sonde utilisable par `flux_shared::flows::evaluate_condition`.
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
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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

    pub const fn tone(self) -> TagTone {
        match self {
            Self::Complete => TagTone::Success,
            Self::IncompletePath => TagTone::Warning,
            Self::CycleDetected | Self::NoEntry => TagTone::Danger,
        }
    }
}

/// Une étape traversée pendant la simulation.
#[derive(Clone, Debug, PartialEq, Eq)]
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
#[derive(Clone, Debug, PartialEq, Eq)]
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
        || (node.config.get("property").is_some()
            && node.config.get("operator").is_some()
            && node.config.get("target").is_some())
}

fn evaluate_node_condition(node: &FlowNode, probe_info: &Value) -> (bool, String) {
    let config = ConditionConfig {
        property: node
            .config
            .get("property")
            .and_then(Value::as_str)
            .unwrap_or("any")
            .trim()
            .to_owned(),
        operator: node
            .config
            .get("operator")
            .and_then(Value::as_str)
            .unwrap_or("eq")
            .trim()
            .to_owned(),
        target: node
            .config
            .get("target")
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim()
            .to_owned(),
    };
    let result = evaluate_condition(&config, probe_info);
    let verdict = if result { "Vrai" } else { "Faux" };
    (
        result,
        format!(
            "{} {} {} → {verdict}",
            config.property, config.operator, config.target
        ),
    )
}

fn find_entry_node<'a>(graph: &'a FlowGraph, catalog: &[OwnedBlockSpec]) -> Option<&'a FlowNode> {
    if let Some(input_node) = graph.nodes.iter().find(|node| {
        let spec = find_owned(catalog, &node.kind);
        spec.is_some_and(|s| !s.has_input)
            || node.kind.as_str() == "file-input"
            || node.kind.as_str() == "block.core.file-input"
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

/// Exécute une simulation déterministe du `graph` sur un document d'entrée `input`.
pub fn simulate_flow(
    graph: &FlowGraph,
    catalog: &[OwnedBlockSpec],
    input: &SimulationInput,
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
            let summary = node_summary(node, spec).unwrap_or_else(|| "Étape exécutée".to_owned());
            (None, summary)
        };

        let is_terminal = spec.is_some_and(|s| s.terminal)
            || node.kind.as_str() == "file-output"
            || node.kind.as_str() == "block.core.file-output";

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
                .map(|p| format!(" sur la sortie « {} »", output_presentation(p).label))
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

/// Panneau latéral de simulation interactive du flux dans l'éditeur.
#[component]
pub fn FlowSimulationPanel(editor: Editor) -> impl IntoView {
    let report = Memo::new(move |_| {
        let graph = editor.graph.get();
        let catalog = editor.catalog.get();
        let input = editor.sim_input.get();
        simulate_flow(&graph, &catalog, &input)
    });

    let step_back = move |_| {
        let len = report.with_untracked(|r| r.steps.len());
        if len == 0 {
            return;
        }
        editor.sim_step.update(|cur| {
            *cur = Some(match *cur {
                Some(i) => i.saturating_sub(1),
                None => len.saturating_sub(1),
            });
        });
    };

    let step_forward = move |_| {
        let len = report.with_untracked(|r| r.steps.len());
        if len == 0 {
            return;
        }
        editor.sim_step.update(|cur| {
            *cur = match *cur {
                None => Some(0),
                Some(i) if i + 1 < len => Some(i + 1),
                Some(_) => None,
            };
        });
    };

    view! {
        <div class="flow-dock-pane flow-sim-pane" aria-label="Simulateur de flux">
            <div class="flow-inspector__shell">
                <header class="flow-inspector__header">
                    <div class="flow-inspector__identity">
                        <span class="flow-inspector__eyebrow">"Test à sec"</span>
                        <h2 class="flow-inspector__title">"Simulation du parcours"</h2>
                    </div>
                </header>

                <div class="flow-inspector__body">
                    <div class="flow-inspector__section">
                        <span class="flow-inspector__section-title">"Entrée de simulation"</span>
                        <div class="flow-sim__presets">
                            {SimulationPreset::ALL.into_iter().map(|preset| view! {
                                <button
                                    type="button"
                                    class="btn btn-secondary btn-sm"
                                    on:click=move |_| {
                                        editor.sim_input.set(preset.input());
                                        editor.sim_step.set(None);
                                    }
                                >
                                    {preset.label()}
                                </button>
                            }).collect_view()}
                        </div>
                        <div class="flow-sim__fields">
                            <div class="form-group">
                                <label for="sim-file-name" class="form-label">"Nom du fichier"</label>
                                <input id="sim-file-name" class="form-input text-mono"
                                    prop:value=move || editor.sim_input.get().file_name
                                    on:input=move |ev| {
                                        editor.sim_input.update(|m| m.file_name = event_target_value(&ev));
                                    }
                                />
                            </div>
                            <div class="form-group">
                                <label for="sim-probe-json" class="form-label">"Métadonnées de sonde (JSON)"</label>
                                <textarea id="sim-probe-json" class="form-input text-mono" rows=JSON_EDITOR_ROWS
                                    prop:value=move || editor.sim_input.get().probe_json
                                    on:input=move |ev| {
                                        editor.sim_input.update(|m| m.probe_json = event_target_value(&ev));
                                    }
                                ></textarea>
                            </div>
                        </div>
                    </div>

                    <div class="flow-inspector__section">
                        <div class="flow-sim__controls">
                            {move || {
                                let status = report.get().status;
                                view! { <Tag label=status.label() tone=status.tone() /> }
                            }}
                            <div class="cluster">
                                <button type="button" class="btn btn-secondary btn-sm"
                                    title="Étape précédente" aria-label="Étape précédente"
                                    on:click=step_back>
                                    <Icon name=IconName::ChevronLeft size=14 />
                                </button>
                                <button type="button" class="btn btn-secondary btn-sm"
                                    title="Étape suivante" aria-label="Étape suivante"
                                    on:click=step_forward>
                                    <Icon name=IconName::ChevronRight size=14 />
                                </button>
                                <button type="button" class="btn btn-secondary btn-sm"
                                    on:click=move |_| editor.sim_step.set(None)>
                                    "Tout"
                                </button>
                            </div>
                        </div>

                        {move || {
                            let rep = report.get();
                            view! {
                                <div class="stack">
                                    {(!rep.warnings.is_empty()).then(|| view! {
                                        <ul class="flow-sim__warnings" role="status">
                                            {rep.warnings.into_iter().map(|warn| view! {
                                                <li>{warn}</li>
                                            }).collect_view()}
                                        </ul>
                                    })}
                                    <ol class="flow-sim__steps">
                                        {rep.steps.into_iter().map(|step| {
                                            let idx = step.index;
                                            let node_id = step.node_id.clone();
                                            let is_focused = move || editor.sim_step.get() == Some(idx);
                                            view! {
                                                <li>
                                                    <button
                                                        type="button"
                                                        class="flow-sim__step-btn"
                                                        class:is-active=is_focused
                                                        on:click=move |_| {
                                                            editor.sim_step.set(Some(idx));
                                                            editor.selected.set(Some(node_id.clone()));
                                                        }
                                                    >
                                                        <span class="flow-sim__step-num text-mono">{format!("#{}", idx + 1)}</span>
                                                        <span class="flow-sim__step-meta">
                                                            <strong>{step.node_name}</strong>
                                                            <span class="text-dim">{step.detail}</span>
                                                        </span>
                                                        {step.port_taken.map(|p| {
                                                            let pres = output_presentation(&p);
                                                            view! { <Tag label=pres.label tone=TagTone::Neutral /> }
                                                        })}
                                                    </button>
                                                </li>
                                            }
                                        }).collect_view()}
                                    </ol>
                                </div>
                            }
                        }}
                    </div>
                </div>
            </div>
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use flux_shared::flows::FlowBlockKind;

    fn test_catalog() -> Vec<OwnedBlockSpec> {
        vec![
            OwnedBlockSpec {
                kind: "block.core.file-input".into(),
                aliases: vec!["file-input".into()],
                label: "Entrée".into(),
                description: String::new(),
                category: "Entrée".into(),
                domain: Some("input".into()),
                has_input: false,
                ports: Vec::new(),
                terminal: false,
                suggested_next: Vec::new(),
                from_plugin: true,
                operation_id: None,
                fields: Vec::new(),
            },
            OwnedBlockSpec {
                kind: "block.core.condition".into(),
                aliases: vec!["condition".into()],
                label: "Condition".into(),
                description: String::new(),
                category: "Logique".into(),
                domain: Some("logic".into()),
                has_input: true,
                ports: vec![
                    ("true".into(), "Vrai".into()),
                    ("false".into(), "Faux".into()),
                ],
                terminal: false,
                suggested_next: Vec::new(),
                from_plugin: true,
                operation_id: None,
                fields: Vec::new(),
            },
            OwnedBlockSpec {
                kind: "block.video.encode".into(),
                aliases: vec![],
                label: "Encodage".into(),
                description: String::new(),
                category: "Vidéo".into(),
                domain: Some("video".into()),
                has_input: true,
                ports: Vec::new(),
                terminal: false,
                suggested_next: Vec::new(),
                from_plugin: true,
                operation_id: None,
                fields: Vec::new(),
            },
            OwnedBlockSpec {
                kind: "block.core.file-output".into(),
                aliases: vec!["file-output".into()],
                label: "Sortie".into(),
                description: String::new(),
                category: "Sortie".into(),
                domain: Some("output".into()),
                has_input: true,
                ports: Vec::new(),
                terminal: true,
                suggested_next: Vec::new(),
                from_plugin: true,
                operation_id: None,
                fields: Vec::new(),
            },
        ]
    }

    #[test]
    fn simulation_follows_true_and_false_condition_branches() {
        let catalog = test_catalog();
        let mut graph = FlowGraph::default();
        let entry = graph.append(
            FlowBlockKind::parse("block.core.file-input"),
            "Entrée",
            json!({}),
            &catalog,
        );
        let cond = graph.append(
            FlowBlockKind::parse("block.core.condition"),
            "Condition codec ?",
            json!({"property": "tracks[type=video].codec", "operator": "eq", "target": "hevc"}),
            &catalog,
        );
        let encode = graph.append(
            FlowBlockKind::parse("block.video.encode"),
            "Traitement",
            json!({}),
            &catalog,
        );
        let out = graph.append(
            FlowBlockKind::parse("block.core.file-output"),
            "Sortie",
            json!({}),
            &catalog,
        );
        // `append` câble automatiquement ; on repart d'arêtes explicites pour tester les deux branches.
        graph.connections.clear();
        assert!(graph.connect(&entry, None, &cond, &catalog).is_ok());
        assert!(
            graph
                .connect(&cond, Some("true".into()), &out, &catalog)
                .is_ok()
        );
        assert!(
            graph
                .connect(&cond, Some("false".into()), &encode, &catalog)
                .is_ok()
        );
        assert!(graph.connect(&encode, None, &out, &catalog).is_ok());

        let h264 = simulate_flow(
            &graph,
            &catalog,
            &SimulationPreset::StructuredTracks.input(),
        );
        assert_eq!(h264.status, SimulationStatus::Complete);
        let h264_nodes: Vec<_> = h264.steps.iter().map(|s| s.node_id.as_str()).collect();
        assert_eq!(
            h264_nodes,
            vec![entry.as_str(), cond.as_str(), encode.as_str(), out.as_str()]
        );

        let hevc_input = SimulationInput {
            file_name: "film-4k.mp4".into(),
            probe_json: r#"{"tracks":[{"type":"video","codec":"hevc"}]}"#.into(),
        };
        let hevc = simulate_flow(&graph, &catalog, &hevc_input);
        assert_eq!(hevc.status, SimulationStatus::Complete);
        let hevc_nodes: Vec<_> = hevc.steps.iter().map(|s| s.node_id.as_str()).collect();
        assert_eq!(
            hevc_nodes,
            vec![entry.as_str(), cond.as_str(), out.as_str()]
        );
    }

    #[test]
    fn simulation_evaluates_generic_paths_and_detects_dead_ends_and_cycles() {
        let catalog = test_catalog();
        let mut graph = FlowGraph::default();
        let entry = graph.append(
            FlowBlockKind::parse("block.core.file-input"),
            "Entrée",
            json!({}),
            &catalog,
        );
        let doc_cond = graph.append(
            FlowBlockKind::parse("block.core.condition"),
            "Format EPUB ?",
            json!({"property": "format", "operator": "exact", "target": "epub"}),
            &catalog,
        );
        let dead = graph.append(
            FlowBlockKind::parse("block.video.encode"),
            "Traitement sans suite",
            json!({}),
            &catalog,
        );
        graph.connections.clear();
        assert!(graph.connect(&entry, None, &doc_cond, &catalog).is_ok());
        assert!(
            graph
                .connect(&doc_cond, Some("true".into()), &dead, &catalog)
                .is_ok()
        );

        let dead_rep = simulate_flow(
            &graph,
            &catalog,
            &SimulationPreset::DocumentMetadata.input(),
        );
        assert_eq!(dead_rep.status, SimulationStatus::IncompletePath);
        assert_eq!(
            dead_rep.steps.last().map(|s| s.node_id.as_str()),
            Some(dead.as_str())
        );

        let miss_rep = simulate_flow(&graph, &catalog, &SimulationPreset::Minimal.input());
        assert_eq!(miss_rep.status, SimulationStatus::IncompletePath);
        assert_eq!(
            miss_rep.steps.last().map(|s| s.node_id.as_str()),
            Some(doc_cond.as_str())
        );

        // L'éditeur bloque la boucle à la connexion, et le simulateur détecte une boucle héritée.
        assert!(graph.connect(&dead, None, &doc_cond, &catalog).is_err());
        graph.connections.push(FlowConnection {
            from: dead.clone(),
            to: doc_cond.clone(),
            port: None,
        });
        let cycle_rep = simulate_flow(
            &graph,
            &catalog,
            &SimulationPreset::DocumentMetadata.input(),
        );
        assert_eq!(cycle_rep.status, SimulationStatus::CycleDetected);

        let empty = simulate_flow(&FlowGraph::default(), &catalog, &SimulationInput::default());
        assert_eq!(empty.status, SimulationStatus::NoEntry);
    }
}
