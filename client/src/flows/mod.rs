//! Flux : liste, création et éditeur (graphe, réglages générés, liaisons) (ADR 0014).

mod canvas;
mod dnd;
mod drag;
mod editor;
mod fields;
mod geometry;
mod inspector;
mod palette;
mod presentation;
mod quick_add;
mod simulation;
mod state;
mod versions;

pub use editor::FlowEditor;

use crate::route::{Link, Route, encode_segment, use_router};
use crate::ui::filters::{Filters, FlowFilter, flow_filter};
use crate::ui::search::{matches, query};
use crate::ui::topbar_action::{ActionSpec, provide_topbar_action};
use crate::ui::{
    EmptyState, Icon, IconName, MetricTone, ResourceCard, StatItem, StatStrip, StatusBadge, Tag,
    TagTone, Toasts,
};
use crate::{api, format};
use flux_shared::Permission;
use flux_shared::flows::{FlowGraph, FlowRecord};
use flux_shared::libraries::Library;
use flux_shared::session::Me;
use leptos::prelude::*;

const FLOWS_PATH: &str = "/api/v1/flows";
const LIBRARIES_PATH: &str = "/api/v1/libraries";
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
fn FlowCard(
    flow: FlowRecord,
    bound_libraries: usize,
    can_manage: bool,
    on_removed: Callback<()>,
) -> impl IntoView {
    let router = use_router();
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
    let editor_id = flow.id.clone();
    let open_editor = Callback::new(move |()| router.go(Route::FlowEditor(editor_id.clone())));
    let badge = flow
        .is_default
        .then(|| view! { <StatusBadge label="Défaut" variant="success" /> }.into_any());
    let actions = view! {
        <Link to=Route::FlowEditor(flow.id) class="btn btn-secondary btn-sm">"Ouvrir l'atelier"</Link>
        {removable.then(|| view! {
            <button type="button" class="btn btn-sm btn-danger-quiet" disabled=move || remove.pending().get()
                on:click=move |_| { remove.dispatch(()); }>"Supprimer"</button>
        })}
    }
    .into_any();
    let libs_label = match bound_libraries {
        0 => "Aucune bibliothèque".to_owned(),
        1 => "1 bibliothèque".to_owned(),
        count => format!("{count} bibliothèques"),
    };

    view! {
        <ResourceCard
            title=flow.name
            subtitle=description
            icon=IconName::Flows
            tone=if flow.is_default { MetricTone::Success } else { MetricTone::Neutral }
            badge=badge
            on_click=Some(open_editor)
            footer_meta=format!("{total_blocks} bloc(s) · {updated}")
            footer_actions=Some(actions)
        >
            <div class="cluster">
                <Tag label=format!("v{}", flow.version) tone=TagTone::Mono />
                <Tag label=flow.file_mask tone=TagTone::Primary />
                <Tag
                    label=libs_label
                    tone={if bound_libraries > 0 { TagTone::Success } else { TagTone::Neutral }}
                    icon=Some(IconName::Libraries)
                />
            </div>
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
            {move || feedback.get().map(|err| view! {
                <p class="text-danger text-xs" role="status">{err}</p>
            })}
        </ResourceCard>
    }
}

#[component]
pub fn FlowsPage(me: Me) -> impl IntoView {
    let can_manage = me.allows(Permission::ManageFlows);
    let can_libraries = me.allows(Permission::ViewLibraries);
    let filters = use_context::<Filters>();
    let reload = RwSignal::new(0_u32);
    let flows = LocalResource::new(move || {
        reload.track();
        api::get::<Vec<FlowRecord>>(FLOWS_PATH)
    });
    let libraries = LocalResource::new(move || {
        reload.track();
        async move {
            if can_libraries {
                api::get::<Vec<Library>>(LIBRARIES_PATH)
                    .await
                    .unwrap_or_default()
            } else {
                Vec::new()
            }
        }
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
        let active = flow_filter();
        let set_filter = move |next: FlowFilter| {
            Callback::new(move |()| {
                if let Some(filters) = filters {
                    filters.flow.set(next);
                }
            })
        };
        vec![
            StatItem::new("Total", total.to_string(), MetricTone::Neutral)
                .with_filter(active == FlowFilter::All, set_filter(FlowFilter::All)),
            StatItem::new("Par défaut", defaults.to_string(), MetricTone::Success).with_filter(
                active == FlowFilter::DefaultOnly,
                set_filter(FlowFilter::DefaultOnly),
            ),
            StatItem::new("Blocs configurés", blocks.to_string(), MetricTone::Neutral),
        ]
    });
    let grid = move || {
        let text = query();
        let active_filter = flow_filter();
        let libs = libraries.get().unwrap_or_default();
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
                    .filter(|flow| {
                        active_filter.accepts(flow.is_default)
                            && matches(&[&flow.name, &flow.id, &flow.file_mask], &text)
                    })
                    .collect();
                if shown.is_empty() {
                    let unfiltered = text.trim().is_empty() && active_filter == FlowFilter::All;
                    let create_action = (can_manage && unfiltered)
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
                                title=if unfiltered {
                                    "Aucun flux installé"
                                } else {
                                    "Aucun flux correspondant"
                                }
                                message=if unfiltered {
                                    "Créez un premier pipeline nodal pour automatiser le traitement de vos médias."
                                } else {
                                    "Ajustez vos filtres pour afficher un flux."
                                }
                                icon_name=IconName::Flows
                                action=create_action
                            />
                        </div>
                    }
                    .into_any();
                }
                view! {
                    <div class="resource-grid">
                        {shown
                            .into_iter()
                            .map(|flow| {
                                let bound_libraries = libs
                                    .iter()
                                    .filter(|lib| lib.flow_id.as_deref() == Some(flow.id.as_str()))
                                    .count();
                                view! {
                                    <FlowCard
                                        flow=flow
                                        bound_libraries=bound_libraries
                                        can_manage=can_manage
                                        on_removed=on_removed
                                    />
                                }
                            })
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
pub(super) mod test_support {
    use flux_shared::flows::{OwnedBlockSpec, OwnedFieldKind, OwnedFieldSpec};
    use serde_json::json;

    struct BlockBuilder(OwnedBlockSpec);

    impl BlockBuilder {
        fn new(kind: &str, alias: &str, label: &str, category: &str, domain: &str) -> Self {
            Self(OwnedBlockSpec {
                kind: kind.into(),
                aliases: if alias.is_empty() {
                    Vec::new()
                } else {
                    vec![alias.into()]
                },
                label: label.into(),
                category: category.into(),
                domain: Some(domain.into()),
                description: String::new(),
                has_input: true,
                ports: Vec::new(),
                terminal: false,
                fields: Vec::new(),
                suggested_next: Vec::new(),
                from_plugin: true,
                operation_id: None,
            })
        }

        fn no_input(mut self) -> Self {
            self.0.has_input = false;
            self
        }

        fn ports(mut self, ports: &[(&str, &str)]) -> Self {
            self.0.ports = ports
                .iter()
                .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
                .collect();
            self
        }

        fn terminal(mut self) -> Self {
            self.0.terminal = true;
            self
        }

        fn fields(mut self, fields: Vec<OwnedFieldSpec>) -> Self {
            self.0.fields = fields;
            self
        }

        fn suggest(mut self, next: &[&str]) -> Self {
            self.0.suggested_next = next.iter().map(|s| (*s).to_owned()).collect();
            self
        }

        fn build(self) -> OwnedBlockSpec {
            self.0
        }
    }

    pub(super) fn test_catalog() -> Vec<OwnedBlockSpec> {
        vec![
            BlockBuilder::new(
                "block.core.file-input",
                "file-input",
                "Fichier source",
                "Entrée",
                "input",
            )
            .no_input()
            .suggest(&["block.core.condition", "block.video.remux-mkv"])
            .build(),
            BlockBuilder::new(
                "block.core.condition",
                "condition",
                "Condition",
                "Logique",
                "logic",
            )
            .ports(&[("true", "✓ Vrai"), ("false", "✗ Faux")])
            .fields(vec![OwnedFieldSpec {
                name: "target".into(),
                label: "Valeur".into(),
                kind: OwnedFieldKind::Text,
                required: true,
                help: String::new(),
                default: None,
            }])
            .suggest(&["block.video.encode", "block.video.remux-mkv"])
            .build(),
            BlockBuilder::new(
                "block.media.inspect",
                "media-inspect",
                "Inspection média",
                "Média",
                "inspect",
            )
            .build(),
            BlockBuilder::new(
                "block.metadata.mkvpropedit",
                "mkvpropedit",
                "Édition des en-têtes",
                "MKVToolNix",
                "mkv",
            )
            .build(),
            BlockBuilder::new(
                "block.video.remux-mkv",
                "remux-mkv",
                "Remuxage Matroska",
                "MKVToolNix",
                "mkv",
            )
            .suggest(&["block.core.file-output"])
            .build(),
            BlockBuilder::new(
                "block.video.encode",
                "video-encode",
                "Encodage vidéo",
                "Média",
                "video",
            )
            .suggest(&["block.video.remux-mkv"])
            .build(),
            BlockBuilder::new(
                "block.audio.encode",
                "audio-encode",
                "Encodage audio",
                "Média",
                "audio",
            )
            .build(),
            BlockBuilder::new(
                "block.sonarr.rescan",
                "sonarr-rescan",
                "Sonarr : réanalyser",
                "Intégrations",
                "integration",
            )
            .build(),
            BlockBuilder::new(
                "block.sonarr.rename",
                "sonarr-rename",
                "Sonarr : renommer",
                "Intégrations",
                "integration",
            )
            .build(),
            BlockBuilder::new(
                "block.core.webhook",
                "webhook",
                "Webhook",
                "Notification",
                "integration",
            )
            .fields(vec![
                OwnedFieldSpec {
                    name: "url".into(),
                    label: "URL".into(),
                    kind: OwnedFieldKind::Text,
                    required: true,
                    help: String::new(),
                    default: None,
                },
                OwnedFieldSpec {
                    name: "method".into(),
                    label: "Méthode".into(),
                    kind: OwnedFieldKind::Select(vec![("POST".into(), "POST".into())]),
                    required: false,
                    help: String::new(),
                    default: Some(json!("POST")),
                },
                OwnedFieldSpec {
                    name: "custom_headers".into(),
                    label: "En-têtes".into(),
                    kind: OwnedFieldKind::Headers,
                    required: false,
                    help: String::new(),
                    default: Some(json!({})),
                },
                OwnedFieldSpec {
                    name: "fail_on_error".into(),
                    label: "Échec bloquant".into(),
                    kind: OwnedFieldKind::Bool,
                    required: false,
                    help: String::new(),
                    default: Some(json!(false)),
                },
            ])
            .suggest(&["block.core.file-output"])
            .build(),
            BlockBuilder::new(
                "block.core.file-output",
                "file-output",
                "Fin du flux",
                "Sortie",
                "output",
            )
            .terminal()
            .build(),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use flux_shared::flows::FlowBlockKind;

    #[test]
    fn pipeline_preview_labels_limits_steps_and_handles_invalid_json() {
        let catalog = test_support::test_catalog();
        let mut graph = FlowGraph::default();
        for idx in 1..=5 {
            graph.append(
                FlowBlockKind::parse("block.core.file-input"),
                &format!("Étape {idx}"),
                serde_json::json!({}),
                &catalog,
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
        let catalog = test_support::test_catalog();
        let mut graph = FlowGraph::default();
        graph.append(
            FlowBlockKind::parse("block.core.file-input"),
            "Entrée",
            serde_json::json!({}),
            &catalog,
        );
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
