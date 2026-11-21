//! Simulation à sec d'un flux dans l'éditeur sans lancer de tâche réelle.
//!
//! Évalue localement les nœuds `condition` avec le même évaluateur JSON générique
//! que le moteur (`forgearr_shared::flows::evaluate_condition`) et trace le parcours sur le graphe.

use super::presentation::{node_summary, output_presentation};
use super::state::Editor;
use crate::api;
use crate::ui::{Icon, IconName, Tag, TagTone};
use forgearr_shared::flow_analysis::{FlowMediaSample, simulate_flow_with};
pub use forgearr_shared::flow_analysis::{
    SimulationInput, SimulationPreset, SimulationReport, SimulationStatus,
};
use forgearr_shared::flows::{FlowGraph, OwnedBlockSpec};
use leptos::prelude::*;

const JSON_EDITOR_ROWS: u32 = 6;
const NO_SAMPLE_MESSAGE: &str = "Aucune métadonnée de job réel pour ce flux : elles sont retenues après la première tâche exécutée.";
const SAMPLE_UNAVAILABLE_MESSAGE: &str = "Métadonnées du dernier job indisponibles.";

/// Chemin du dernier échantillon de métadonnées d'un flux enregistré.
fn media_sample_path(flow_id: &str) -> String {
    format!("/api/v1/flows/{flow_id}/media-sample")
}

trait SimulationStatusTone {
    fn tone(self) -> TagTone;
}

impl SimulationStatusTone for SimulationStatus {
    fn tone(self) -> TagTone {
        match self {
            Self::Complete => TagTone::Success,
            Self::IncompletePath => TagTone::Warning,
            Self::CycleDetected | Self::NoEntry => TagTone::Danger,
        }
    }
}

/// Exécute une simulation déterministe du `graph` sur un document d'entrée `input`.
pub fn simulate_flow(
    graph: &FlowGraph,
    catalog: &[OwnedBlockSpec],
    input: &SimulationInput,
) -> SimulationReport {
    simulate_flow_with(graph, catalog, input, &node_summary, &|port| {
        output_presentation(port).label.to_owned()
    })
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

    let sample_note = RwSignal::new(None::<String>);
    let load_sample = move |_| {
        let Some(flow_id) = editor.flow_id.get_untracked() else {
            sample_note.set(Some(
                "Enregistrez le flux pour charger les métadonnées d'un job.".into(),
            ));
            return;
        };
        leptos::task::spawn_local(async move {
            match api::get::<FlowMediaSample>(&media_sample_path(&flow_id)).await {
                Ok(sample) => {
                    editor.sim_input.set(sample.to_simulation_input());
                    editor.sim_step.set(None);
                    sample_note.set(Some(format!(
                        "Métadonnées de « {} » (tâche du {}).",
                        sample.source_name, sample.recorded_at
                    )));
                }
                Err(error) if error.code() == Some("not_found") => {
                    sample_note.set(Some(NO_SAMPLE_MESSAGE.into()));
                }
                Err(_) => sample_note.set(Some(SAMPLE_UNAVAILABLE_MESSAGE.into())),
            }
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
                    <super::plan::ProcessingPlan graph=Signal::derive(move || editor.graph.get()) report=report flow_id=editor.flow_id />
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
                            <button type="button" class="btn btn-secondary btn-sm"
                                on:click=load_sample>
                                "Dernier job réel"
                            </button>
                        </div>
                        {move || sample_note.get().map(|note| view! { <p class="form-help">{note}</p> })}
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
    use forgearr_shared::flows::{FlowBlockKind, FlowConnection};
    use serde_json::json;

    fn test_catalog() -> Vec<OwnedBlockSpec> {
        vec![
            OwnedBlockSpec {
                kind: "essentiel.file-input".into(),
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
                owner: None,
                fields: Vec::new(),
            },
            OwnedBlockSpec {
                kind: "essentiel.condition".into(),
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
                owner: None,
                fields: Vec::new(),
            },
            OwnedBlockSpec {
                kind: "video.encode".into(),
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
                owner: None,
                fields: Vec::new(),
            },
            OwnedBlockSpec {
                kind: "essentiel.file-output".into(),
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
                owner: None,
                fields: Vec::new(),
            },
        ]
    }

    #[test]
    fn simulation_follows_true_and_false_condition_branches() {
        let catalog = test_catalog();
        let mut graph = FlowGraph::default();
        let entry = graph.append(
            FlowBlockKind::parse("essentiel.file-input"),
            "Entrée",
            json!({}),
            &catalog,
        );
        let cond = graph.append(
            FlowBlockKind::parse("essentiel.condition"),
            "Condition codec ?",
            json!({"property": "tracks[type=video].codec", "operator": "eq", "target": "hevc"}),
            &catalog,
        );
        let encode = graph.append(
            FlowBlockKind::parse("video.encode"),
            "Traitement",
            json!({}),
            &catalog,
        );
        let out = graph.append(
            FlowBlockKind::parse("essentiel.file-output"),
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
            FlowBlockKind::parse("essentiel.file-input"),
            "Entrée",
            json!({}),
            &catalog,
        );
        let doc_cond = graph.append(
            FlowBlockKind::parse("essentiel.condition"),
            "Format EPUB ?",
            json!({"property": "format", "operator": "exact", "target": "epub"}),
            &catalog,
        );
        let dead = graph.append(
            FlowBlockKind::parse("video.encode"),
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

    #[test]
    fn the_sample_path_targets_the_flow_media_sample_route() {
        assert_eq!(
            media_sample_path("flow.test"),
            "/api/v1/flows/flow.test/media-sample"
        );
    }
}
