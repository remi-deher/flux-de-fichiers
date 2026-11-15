//! Flux : liste, création et éditeur (graphe, réglages générés, liaisons) (ADR 0014).

mod canvas;
mod dnd;
mod drag;
mod editor;
mod fields;
mod geometry;
mod inspector;
mod palette;
mod state;
mod versions;

pub use editor::FlowEditor;

use crate::route::{Link, Route, encode_segment, use_router};
use crate::ui::search::{matches, query};
use crate::ui::topbar_action::{ActionSpec, provide_topbar_action};
use crate::ui::{EmptyState, Icon, IconName, MetricTone, StatItem, StatStrip, StatusBadge, Toasts};
use crate::{api, format};
use flux_shared::Permission;
use flux_shared::flows::{FlowGraph, FlowRecord};
use flux_shared::session::Me;
use leptos::prelude::*;

const FLOWS_PATH: &str = "/api/v1/flows";
const PIPELINE_PREVIEW_LIMIT: usize = 4;

fn pipeline_preview(graph_json: &str) -> (Vec<String>, usize) {
    let nodes = FlowGraph::parse(graph_json).nodes;
    let total = nodes.len();
    let labels = nodes
        .into_iter()
        .take(PIPELINE_PREVIEW_LIMIT)
        .map(|node| node.name)
        .collect();
    (labels, total)
}

/// Extrait jusqu'à `PIPELINE_PREVIEW_LIMIT` libellés d'étapes pour l'aperçu d'un flux.
pub fn pipeline_preview_labels(graph_json: &str) -> Vec<String> {
    pipeline_preview(graph_json).0
}

/// Synthèse `(total, par_défaut, blocs_configurés)` pour le bandeau des flows.
pub fn flow_counts(flows: &[FlowRecord]) -> (usize, usize, usize) {
    let total = flows.len();
    let defaults = flows.iter().filter(|flow| flow.is_default).count();
    let blocks = flows
        .iter()
        .map(|flow| FlowGraph::parse(&flow.definition).nodes.len())
        .sum();
    (total, defaults, blocks)
}

#[component]
fn FlowCard(flow: FlowRecord, can_manage: bool, on_removed: Callback<()>) -> impl IntoView {
    let (preview, total_blocks) = pipeline_preview(&flow.definition);
    let extra_blocks = total_blocks.saturating_sub(preview.len());
    let feedback = RwSignal::new(None::<String>);
    let path = format!("{FLOWS_PATH}/{}", encode_segment(&flow.id));
    let remove = Action::new_local(move |_: &()| {
        let path = path.clone();
        async move { api::delete(&path).await }
    });
    Effect::new(move |_| match remove.value().get() {
        Some(Ok(())) => {
            Toasts::notify_success("Flux supprimé.");
            on_removed.run(());
        }
        Some(Err(error)) => Toasts::set_error(feedback, error.message()),
        None => {}
    });
    let removable = can_manage && !flow.is_default;
    let updated = format::datetime(flow.updated_at);
    let description = flow.description.filter(|text| !text.trim().is_empty());

    view! {
        <article class="flow-card">
            <div class="flow-card__head">
                <div class="flow-card__title-group">
                    <h2 class="flow-card__title">{flow.name}</h2>
                    <div class="flow-card__badges">
                        <span class="badge text-mono">{format!("v{}", flow.version)}</span>
                        {flow.is_default.then(|| view! { <StatusBadge label="Défaut" variant="success" /> })}
                        <span class="flow-pattern-tag text-mono">{flow.file_mask}</span>
                    </div>
                </div>
            </div>
            {description.map(|desc| view! {
                <p class="flow-card__desc text-dim">{desc}</p>
            })}
            <div class="flow-card__pipeline">
                {if preview.is_empty() {
                    view! { <span class="text-dim text-xs">"Pipeline vide"</span> }.into_any()
                } else {
                    let count = preview.len();
                    view! {
                        <div class="flow-pipeline" aria-label="Aperçu du pipeline">
                            {preview.into_iter().enumerate().map(|(idx, step)| {
                                view! {
                                    <span class="pipeline-step-node">{step}</span>
                                    {(idx + 1 < count).then(|| view! {
                                        <span class="pipeline-connector" aria-hidden="true">"→"</span>
                                    })}
                                }
                            }).collect_view()}
                            {(extra_blocks > 0).then(|| view! {
                                <span class="pipeline-connector" aria-hidden="true">"→"</span>
                                <span class="pipeline-step-node pipeline-step-node--more">{format!("+{extra_blocks}")}</span>
                            })}
                        </div>
                    }.into_any()
                }}
            </div>
            <div class="flow-card__foot">
                <span class="text-dim text-xs">{format!("{total_blocks} bloc(s) · {updated}")}</span>
                <div class="flow-card__actions">
                    <Link to=Route::FlowEditor(flow.id) class="btn btn-secondary btn-sm">"Ouvrir l'atelier"</Link>
                    {removable.then(|| view! {
                        <button type="button" class="btn btn-sm btn-danger-quiet" disabled=move || remove.pending().get()
                            on:click=move |_| { remove.dispatch(()); }>"Supprimer"</button>
                    })}
                </div>
            </div>
            {move || feedback.get().map(|err| view! {
                <p class="text-danger text-xs" role="status">{err}</p>
            })}
        </article>
    }
}

#[component]
pub fn FlowsPage(me: Me) -> impl IntoView {
    let can_manage = me.allows(Permission::ManageFlows);
    let reload = RwSignal::new(0_u32);
    let flows = LocalResource::new(move || {
        reload.track();
        api::get::<Vec<FlowRecord>>(FLOWS_PATH)
    });
    let on_removed = Callback::new(move |()| reload.update(|count| *count = count.wrapping_add(1)));
    let router = use_router();
    if can_manage {
        provide_topbar_action(ActionSpec {
            label: "Nouveau flux",
            on_click: Callback::new(move |()| router.go(Route::NewFlow)),
        });
    }
    let stats = Signal::derive(move || {
        let list = flows.get().and_then(Result::ok).unwrap_or_default();
        let (total, defaults, blocks) = flow_counts(&list);
        vec![
            StatItem::new("Total", total.to_string(), MetricTone::Neutral),
            StatItem::new("Par défaut", defaults.to_string(), MetricTone::Success),
            StatItem::new("Blocs configurés", blocks.to_string(), MetricTone::Neutral),
        ]
    });
    let grid = move || {
        let text = query();
        match flows.get() {
            None => view! {
                <div class="empty-state-panel">
                    <p class="text-dim">"Chargement des flux…"</p>
                </div>
            }
            .into_any(),
            Some(Err(error)) => view! {
                <div class="empty-state-panel">
                    <p class="form-error">{error.message()}</p>
                </div>
            }
            .into_any(),
            Some(Ok(list)) => {
                let shown: Vec<FlowRecord> = list
                    .into_iter()
                    .filter(|flow| matches(&[&flow.name, &flow.id, &flow.file_mask], &text))
                    .collect();
                if shown.is_empty() {
                    let create_action = (can_manage && text.trim().is_empty())
                        .then(|| {
                            view! {
                                <Link to=Route::NewFlow class="btn btn-primary">
                                    <Icon name=IconName::Plus size=15 />
                                    <span>"Nouveau flux"</span>
                                </Link>
                            }
                        })
                        .into_any();
                    return view! {
                        <div class="empty-state-panel">
                            <EmptyState
                                title=if text.trim().is_empty() {
                                    "Aucun flux installé"
                                } else {
                                    "Aucun flux correspondant"
                                }
                                message=if text.trim().is_empty() {
                                    "Créez un premier pipeline nodal pour automatiser le traitement de vos médias."
                                } else {
                                    "Ajustez votre recherche pour afficher un flux."
                                }
                                icon_name=IconName::Flows
                                action=create_action
                            />
                        </div>
                    }
                    .into_any();
                }
                view! {
                    <div class="flows-grid">
                        {shown
                            .into_iter()
                            .map(|flow| view! { <FlowCard flow=flow can_manage=can_manage on_removed=on_removed /> })
                            .collect_view()}
                    </div>
                }
                .into_any()
            }
        }
    };
    view! {
        <div class="tab-panel" role="tabpanel">
            <StatStrip aria_label="Synthèse des flows" items=stats />
            {grid}
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use flux_shared::flows::FlowBlockKind;

    #[test]
    fn pipeline_preview_labels_limits_steps_and_handles_invalid_json() {
        let mut graph = FlowGraph::default();
        for idx in 1..=5 {
            graph.append(
                FlowBlockKind::FileInput,
                &format!("Étape {idx}"),
                serde_json::json!({}),
            );
        }
        let json = serde_json::to_string(&graph).unwrap_or_default();
        assert_eq!(
            pipeline_preview_labels(&json),
            vec!["Étape 1", "Étape 2", "Étape 3", "Étape 4"]
        );
        assert!(pipeline_preview_labels("not-json").is_empty());
    }

    #[test]
    fn flow_counts_aggregates_defaults_and_blocks() {
        let mut graph = FlowGraph::default();
        graph.append(FlowBlockKind::FileInput, "Entrée", serde_json::json!({}));
        let def = serde_json::to_string(&graph).unwrap_or_default();
        let now = Utc::now();
        let flows = [
            FlowRecord {
                id: "f1".into(),
                name: "Remux".into(),
                description: None,
                file_mask: "*.mkv".into(),
                is_default: true,
                definition: def.clone(),
                version: 1,
                created_at: now,
                updated_at: now,
            },
            FlowRecord {
                id: "f2".into(),
                name: "Léger".into(),
                description: None,
                file_mask: "*.mp4".into(),
                is_default: false,
                definition: def,
                version: 2,
                created_at: now,
                updated_at: now,
            },
        ];
        assert_eq!(flow_counts(&flows), (2, 1, 2));
    }
}
