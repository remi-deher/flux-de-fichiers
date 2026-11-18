//! Flux : liste, création et éditeur (graphe, réglages générés, liaisons) (ADR 0014).

mod canvas;
mod canvas_keys;
mod dnd;
mod drag;
mod editor;
mod fields;
mod geometry;
mod inspector;
mod palette;
mod pointer;
mod presentation;
mod quick_add;
mod script_inspector;
mod simulation;
mod state;
mod versions;

pub use editor::FlowEditor;
pub(crate) use presentation::NodeDomain;

use crate::route::{Link, Route, encode_segment, use_router};
use crate::ui::filters::{Filters, FlowFilter, flow_filter};
use crate::ui::search::{matches, query};
use crate::ui::topbar_action::{ActionSpec, provide_topbar_action};
use crate::ui::{
    Drawer, Icon, IconName, MetricTone, Registre, RegistreMessage, StatItem, StatStrip,
    StatusBadge, Tag, TagTone, Toasts, on_activate,
};
use crate::{api, format};
use flux_shared::Permission;
use flux_shared::flows::{
    FlowGraph, FlowRecord, FlowVersionSummary, UpdateFlowRequest, new_flow_id,
};
use flux_shared::libraries::Library;
use flux_shared::local::{CreateFlowTemplateRequest, LocalFlowTemplate, LocalLibrarySnapshot};
use flux_shared::session::Me;
use leptos::prelude::*;

const FLOWS_PATH: &str = "/api/v1/flows";
const LIBRARIES_PATH: &str = "/api/v1/libraries";
const LOCAL_PATH: &str = "/api/v1/local";
const LOCAL_FLOW_TEMPLATES_PATH: &str = "/api/v1/local/flow-templates";
const LOCAL_ITEMS_PATH: &str = "/api/v1/local/items";
const PIPELINE_PREVIEW_LIMIT: usize = 4;
const STEP_ICON_SIZE: u32 = 13;
const ID_SUFFIX_MODULO: u64 = 100_000;

/// Mode d'affichage de la section Flows & Modèles.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FlowsCatalogFilter {
    #[default]
    Flows,
    Templates,
    All,
}

/// Étape résumée dans la prévisualisation d'un pipeline de flux.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PipelineStepPreview {
    pub label: String,
    pub kind: String,
    pub domain: NodeDomain,
    pub is_branching: bool,
}

fn infer_domain_from_kind(kind: &str, has_branches: bool) -> NodeDomain {
    let lower = kind.to_ascii_lowercase();
    if lower.contains("input") {
        NodeDomain::Input
    } else if lower.contains("condition") || has_branches {
        NodeDomain::Logic
    } else if lower.contains("inspect") {
        NodeDomain::Inspect
    } else if lower.contains("output") {
        NodeDomain::Output
    } else if lower.contains("webhook")
        || lower.contains("sonarr")
        || lower.contains("radarr")
        || lower.contains("notification")
        || lower.contains("integration")
    {
        NodeDomain::Integration
    } else {
        NodeDomain::Process
    }
}

/// Extrait jusqu'à `PIPELINE_PREVIEW_LIMIT` étapes enrichies, le total de blocs et le total de liaisons.
pub fn pipeline_preview_steps(graph_json: &str) -> (Vec<PipelineStepPreview>, usize, usize) {
    let graph = FlowGraph::parse(graph_json);
    let total_blocks = graph.nodes.len();
    let total_connections = graph.connections.len();
    let steps = graph
        .nodes
        .iter()
        .take(PIPELINE_PREVIEW_LIMIT)
        .map(|node| {
            let outgoing_count = graph.outgoing(&node.id).count();
            let has_named_port = graph.outgoing(&node.id).any(|edge| edge.port.is_some());
            let has_branches = outgoing_count > 1 || has_named_port;
            let kind = node.kind.as_str().to_owned();
            let is_branching = has_branches || kind.to_ascii_lowercase().contains("condition");
            let domain = infer_domain_from_kind(&kind, has_branches);
            PipelineStepPreview {
                label: node.name.clone(),
                kind,
                domain,
                is_branching,
            }
        })
        .collect();
    (steps, total_blocks, total_connections)
}

/// Extrait jusqu'à `PIPELINE_PREVIEW_LIMIT` libellés d'étapes pour l'aperçu d'un flux.
pub fn pipeline_preview_labels(graph_json: &str) -> Vec<String> {
    pipeline_preview_steps(graph_json)
        .0
        .into_iter()
        .map(|step| step.label)
        .collect()
}

/// Noms des bibliothèques liées à un flux donné.
pub fn bound_library_names(flow_id: &str, libraries: &[Library]) -> Vec<String> {
    libraries
        .iter()
        .filter(|lib| lib.flow_id.as_deref() == Some(flow_id))
        .map(|lib| lib.name.clone())
        .collect()
}

/// Nombre de bibliothèques liées à un flux donné.
pub fn bound_library_count(flow_id: &str, libraries: &[Library]) -> usize {
    libraries
        .iter()
        .filter(|lib| lib.flow_id.as_deref() == Some(flow_id))
        .count()
}

/// Compteurs affichés dans le bandeau `StatStrip` des flows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FlowUsageCounts {
    pub total: usize,
    pub used: usize,
    pub unused: usize,
}

/// Synthèse `(total, utilisés, inutilisés)` pour le bandeau des flows.
pub fn flow_usage_counts(flows: &[FlowRecord], libraries: &[Library]) -> FlowUsageCounts {
    let used = flows
        .iter()
        .filter(|flow| bound_library_count(&flow.id, libraries) > 0)
        .count();
    let total = flows.len();
    FlowUsageCounts {
        total,
        used,
        unused: total.saturating_sub(used),
    }
}

fn flow_template_id(flow_id: &str) -> String {
    let raw = flow_id.strip_prefix("flow.").unwrap_or(flow_id);
    let slug: String = raw
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '-' | '_') {
                c
            } else {
                '-'
            }
        })
        .take(64)
        .collect();
    let trimmed = slug.trim_matches('-');
    let safe = if trimmed.is_empty() { "flow" } else { trimmed };
    format!("flow.template.{safe}")
}

#[component]
fn FlowPipelinePreview(
    steps: Vec<PipelineStepPreview>,
    extra_blocks: usize,
    empty_label: &'static str,
    aria_label: &'static str,
) -> impl IntoView {
    view! {
        <div class="flow-card__pipeline">
            {if steps.is_empty() {
                view! { <span class="text-dim text-xs">{empty_label}</span> }.into_any()
            } else {
                let count = steps.len();
                view! {
                    <div class="flow-pipeline" aria-label=aria_label>
                        {steps.into_iter().enumerate().map(|(idx, step)| {
                            let step_style = format!("--step-accent: {};", step.domain.accent_var());
                            let icon_name = step.domain.icon();
                            view! {
                                <span class="pipeline-step-node" style=step_style>
                                    <span class="pipeline-step-node__icon" aria-hidden="true">
                                        <Icon name=icon_name size=STEP_ICON_SIZE />
                                    </span>
                                    <span class="pipeline-step-node__label">{step.label}</span>
                                    {step.is_branching.then(|| view! {
                                        <span class="pipeline-step-node__branch" title="Embranchement conditionnel">"⑂"</span>
                                    })}
                                </span>
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
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum SelectedFlowEntry {
    Flow(String),
    Template(String),
}

fn flow_libs_label(bound_libraries: usize) -> String {
    match bound_libraries {
        0 => "Aucune bibliothèque".to_owned(),
        1 => "1 bibliothèque".to_owned(),
        count => format!("{count} bibliothèques"),
    }
}

#[component]
fn FlowPipelineSection(definition: String, empty_label: &'static str) -> impl IntoView {
    let graph = FlowGraph::parse(&definition);
    let (steps, total_blocks, _) = pipeline_preview_steps(&definition);
    let extra_blocks = total_blocks.saturating_sub(steps.len());

    view! {
        <div class="inspection-panel__section">
            <p class="inspection-panel__title">"Aperçu du pipeline"</p>
            <FlowPipelinePreview
                steps=steps
                extra_blocks=extra_blocks
                empty_label=empty_label
                aria_label="Aperçu du pipeline"
            />
            {(!graph.nodes.is_empty()).then(|| {
                let rows = graph
                    .nodes
                    .iter()
                    .enumerate()
                    .map(|(index, node)| {
                        let targets: Vec<String> = graph
                            .outgoing(&node.id)
                            .map(|edge| {
                                let dest = graph
                                    .node(&edge.to)
                                    .map_or(edge.to.as_str(), |target| target.name.as_str());
                                match edge.port.as_deref() {
                                    Some(port) => format!("{port} → {dest}"),
                                    None => dest.to_owned(),
                                }
                            })
                            .collect();
                        let outputs_label = if targets.is_empty() {
                            "—".to_owned()
                        } else {
                            targets.join(", ")
                        };
                        (
                            format!("#{}", index + 1),
                            node.name.clone(),
                            node.kind.as_str().to_owned(),
                            outputs_label,
                        )
                    })
                    .collect::<Vec<_>>();
                view! {
                    <div class="table-wrapper no-margin-top">
                        <table class="data-table" aria-label="Blocs du pipeline">
                            <thead>
                                <tr>
                                    <th>"#"</th>
                                    <th>"Bloc"</th>
                                    <th>"Type"</th>
                                    <th>"Sorties"</th>
                                </tr>
                            </thead>
                            <tbody>
                                {rows.into_iter().map(|(idx, name, kind, outputs)| view! {
                                    <tr>
                                        <td class="text-mono">{idx}</td>
                                        <td class="text-strong">{name}</td>
                                        <td class="text-mono text-dim">{kind}</td>
                                        <td class="text-dim">{outputs}</td>
                                    </tr>
                                }).collect_view()}
                            </tbody>
                        </table>
                    </div>
                }
            })}
        </div>
    }
}

#[component]
fn FlowDetailDrawer(
    flow: FlowRecord,
    bound_libs: Vec<String>,
    tab: RwSignal<usize>,
    can_manage: bool,
    can_templates: bool,
    on_changed: Callback<()>,
    on_closed: Callback<()>,
) -> impl IntoView {
    let feedback = RwSignal::new(None::<String>);
    let versions_reload = RwSignal::new(0_u32);
    let stored_flow = StoredValue::new(flow.clone());
    let stored_libs = StoredValue::new(bound_libs);

    let versions_id = flow.id.clone();
    let versions = LocalResource::new(move || {
        versions_reload.track();
        let path = format!("{FLOWS_PATH}/{}/versions", encode_segment(&versions_id));
        async move { api::get::<Vec<FlowVersionSummary>>(&path).await }
    });

    let restore_id = flow.id.clone();
    let restore = Action::new_local(move |version: &i64| {
        let path = format!(
            "{FLOWS_PATH}/{}/versions/{version}/restore",
            encode_segment(&restore_id)
        );
        async move { api::post::<(), FlowRecord>(&path, None).await }
    });
    Effect::new(move |_| match restore.value().get() {
        Some(Ok(restored)) => {
            Toasts::notify_success(format!("Version v{} restaurée.", restored.version));
            versions_reload.update(|v| *v = v.wrapping_add(1));
            on_changed.run(());
        }
        Some(Err(error)) => Toasts::set_error(feedback, error.message()),
        None => {}
    });

    let delete_path = format!("{FLOWS_PATH}/{}", encode_segment(&flow.id));
    let remove = Action::new_local(move |_: &()| {
        let path = delete_path.clone();
        async move { api::delete(&path).await }
    });
    Effect::new(move |_| match remove.value().get() {
        Some(Ok(())) => {
            Toasts::notify_success("Flux supprimé.");
            on_closed.run(());
            on_changed.run(());
        }
        Some(Err(error)) => Toasts::set_error(feedback, error.message()),
        None => {}
    });

    let dup_name = format!("{} (copie)", flow.name);
    let dup_req = UpdateFlowRequest {
        name: dup_name.clone(),
        description: flow.description.clone(),
        file_mask: flow.file_mask.clone(),
        is_default: false,
        definition: flow.definition.clone(),
    };
    let duplicate = Action::new_local(move |_: &()| {
        let suffix = (js_sys::Date::now() as u64 % ID_SUFFIX_MODULO).to_string();
        let id = new_flow_id(&dup_name, &suffix);
        let path = format!("{FLOWS_PATH}/{}", encode_segment(&id));
        let req = dup_req.clone();
        async move { api::put::<UpdateFlowRequest, FlowRecord>(&path, &req).await }
    });
    Effect::new(move |_| match duplicate.value().get() {
        Some(Ok(_)) => {
            Toasts::notify_success("Flux dupliqué.");
            on_changed.run(());
        }
        Some(Err(error)) => Toasts::set_error(feedback, error.message()),
        None => {}
    });

    let tpl_req = CreateFlowTemplateRequest {
        id: Some(flow_template_id(&flow.id)),
        name: flow.name.clone(),
        description: flow.description.clone(),
        file_mask: Some(flow.file_mask.clone()),
        definition: serde_json::from_str(&flow.definition)
            .unwrap_or_else(|_| serde_json::json!({"nodes": [], "connections": []})),
    };
    let save_template = Action::new_local(move |_: &()| {
        let req = tpl_req.clone();
        async move {
            api::post::<CreateFlowTemplateRequest, LocalFlowTemplate>(
                LOCAL_FLOW_TEMPLATES_PATH,
                Some(&req),
            )
            .await
        }
    });
    Effect::new(move |_| match save_template.value().get() {
        Some(Ok(_)) => {
            Toasts::notify_success("Flux enregistré comme modèle.");
            on_changed.run(());
        }
        Some(Err(error)) => Toasts::set_error(feedback, error.message()),
        None => {}
    });

    let removable = can_manage && !flow.is_default;
    let flow_id = flow.id.clone();

    view! {
        <div class="inspection-panel">
            {move || match tab.get() {
                0 => {
                    let flow = stored_flow.get_value();
                    let libs = stored_libs.get_value();
                    let (_, total_blocks, total_connections) = pipeline_preview_steps(&flow.definition);
                    let desc = flow
                        .description
                        .as_deref()
                        .map(str::trim)
                        .filter(|text| !text.is_empty())
                        .unwrap_or("—")
                        .to_owned();
                    let libs_summary = if libs.is_empty() {
                        "Aucune bibliothèque".to_owned()
                    } else {
                        libs.join(", ")
                    };
                    view! {
                        <div class="inspection-panel__section">
                            <p class="inspection-panel__title">"Général"</p>
                            <div class="detail-grid">
                                <div class="detail-term">"Nom"</div>
                                <div class="detail-val">{flow.name}</div>
                                <div class="detail-term">"Identifiant"</div>
                                <div class="detail-val text-mono">{flow.id}</div>
                                <div class="detail-term">"Description"</div>
                                <div class="detail-val">{desc}</div>
                                <div class="detail-term">"Version"</div>
                                <div class="detail-val text-mono">{format!("v{}", flow.version)}</div>
                                <div class="detail-term">"Masque de fichiers"</div>
                                <div class="detail-val text-mono">{flow.file_mask}</div>
                                <div class="detail-term">"Flux par défaut"</div>
                                <div class="detail-val">{if flow.is_default { "Oui" } else { "Non" }}</div>
                                <div class="detail-term">"Dernière modification"</div>
                                <div class="detail-val">{format::datetime(flow.updated_at)}</div>
                            </div>
                        </div>

                        <div class="inspection-panel__section">
                            <p class="inspection-panel__title">"Utilisation & Structure"</p>
                            <div class="detail-grid">
                                <div class="detail-term">"Bibliothèques liées"</div>
                                <div class="detail-val">{libs_summary}</div>
                                <div class="detail-term">"Blocs"</div>
                                <div class="detail-val">{total_blocks.to_string()}</div>
                                <div class="detail-term">"Liaisons"</div>
                                <div class="detail-val">{total_connections.to_string()}</div>
                            </div>
                        </div>

                        <FlowPipelineSection
                            definition=flow.definition
                            empty_label="Pipeline vide"
                        />
                    }.into_any()
                }
                _ => {
                    let current_version = stored_flow.get_value().version;
                    const VERSION_HEADERS: [&str; 4] = ["Version", "Nom", "Enregistrée", "Action"];
                    view! {
                        <div class="inspection-panel__section">
                            <p class="inspection-panel__title">"Historique des versions"</p>
                            <Registre aria_label="Versions du flux" headers=VERSION_HEADERS.to_vec()>
                                {move || match versions.get() {
                                    None => view! {
                                        <RegistreMessage span=VERSION_HEADERS.len() text="Chargement des versions…" />
                                    }.into_any(),
                                    Some(Err(error)) => view! {
                                        <RegistreMessage span=VERSION_HEADERS.len() text=error.message() />
                                    }.into_any(),
                                    Some(Ok(list)) if list.is_empty() => view! {
                                        <RegistreMessage span=VERSION_HEADERS.len() text="Aucune version enregistrée." />
                                    }.into_any(),
                                    Some(Ok(list)) => list.into_iter().map(|version| {
                                        let number = version.version;
                                        let is_current = number == current_version;
                                        view! {
                                            <tr>
                                                <td class="text-mono">
                                                    {format!("v{number}")}
                                                    {is_current.then_some(" (courante)")}
                                                </td>
                                                <td>{version.name}</td>
                                                <td class="text-dim">{format::datetime(version.created_at)}</td>
                                                <td class="cell-end">
                                                    {(can_manage && !is_current).then(|| view! {
                                                        <button
                                                            type="button"
                                                            class="btn btn-secondary btn-sm"
                                                            disabled=move || restore.pending().get()
                                                            on:click=move |_| { restore.dispatch(number); }
                                                        >
                                                            "Restaurer"
                                                        </button>
                                                    })}
                                                </td>
                                            </tr>
                                        }
                                    }).collect_view().into_any(),
                                }}
                            </Registre>
                        </div>
                    }.into_any()
                }
            }}

            {move || feedback.get().map(|err| view! {
                <p class="form-help text-danger" role="status">{err}</p>
            })}

            <div class="inspection-drawer__actions">
                <Link to=Route::FlowEditor(flow_id) class="btn btn-primary">"Ouvrir l'atelier"</Link>
                {can_manage.then(|| view! {
                    <button
                        type="button"
                        class="btn btn-secondary"
                        disabled=move || duplicate.pending().get()
                        on:click=move |_| { duplicate.dispatch(()); }
                    >
                        "Dupliquer"
                    </button>
                })}
                {can_templates.then(|| view! {
                    <button
                        type="button"
                        class="btn btn-secondary"
                        disabled=move || save_template.pending().get()
                        on:click=move |_| { save_template.dispatch(()); }
                    >
                        "Enregistrer comme modèle"
                    </button>
                })}
                {removable.then(|| view! {
                    <button
                        type="button"
                        class="btn btn-danger-quiet"
                        disabled=move || remove.pending().get()
                        on:click=move |_| { remove.dispatch(()); }
                    >
                        "Supprimer"
                    </button>
                })}
            </div>
        </div>
    }
}

#[component]
fn FlowTemplateDetailDrawer(
    template: LocalFlowTemplate,
    can_manage: bool,
    can_delete: bool,
    on_changed: Callback<()>,
    on_closed: Callback<()>,
) -> impl IntoView {
    let router = use_router();
    let def_str = serde_json::to_string(&template.definition).unwrap_or_else(|_| "{}".into());
    let (_, total_blocks, total_connections) = pipeline_preview_steps(&def_str);
    let feedback = RwSignal::new(None::<String>);

    let name_for_create = template.name.clone();
    let desc_for_create = template.description.clone();
    let mask_for_create = template.file_mask.clone();
    let def_for_create = def_str.clone();
    let create_from_tpl = Action::new_local(move |_: &()| {
        let suffix = (js_sys::Date::now() as u64 % ID_SUFFIX_MODULO).to_string();
        let id = new_flow_id(&name_for_create, &suffix);
        let path = format!("{FLOWS_PATH}/{}", encode_segment(&id));
        let req = UpdateFlowRequest {
            name: name_for_create.clone(),
            description: desc_for_create.clone(),
            file_mask: mask_for_create.clone(),
            is_default: false,
            definition: def_for_create.clone(),
        };
        async move { api::put::<UpdateFlowRequest, FlowRecord>(&path, &req).await }
    });
    Effect::new(move |_| match create_from_tpl.value().get() {
        Some(Ok(created)) => {
            Toasts::notify_success("Flux créé depuis le modèle.");
            on_closed.run(());
            router.go(Route::FlowEditor(created.id));
        }
        Some(Err(error)) => Toasts::set_error(feedback, error.message()),
        None => {}
    });

    let del_id = template.id.clone();
    let remove = Action::new_local(move |_: &()| {
        let path = format!("{LOCAL_ITEMS_PATH}/{}", encode_segment(&del_id));
        async move { api::delete(&path).await }
    });
    Effect::new(move |_| match remove.value().get() {
        Some(Ok(())) => {
            Toasts::notify_success("Modèle supprimé.");
            on_closed.run(());
            on_changed.run(());
        }
        Some(Err(error)) => Toasts::set_error(feedback, error.message()),
        None => {}
    });

    let desc = template
        .description
        .as_deref()
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .unwrap_or("—")
        .to_owned();

    view! {
        <div class="inspection-panel">
            <div class="inspection-panel__section">
                <p class="inspection-panel__title">"Général"</p>
                <div class="detail-grid">
                    <div class="detail-term">"Nom"</div>
                    <div class="detail-val">{template.name}</div>
                    <div class="detail-term">"Identifiant"</div>
                    <div class="detail-val text-mono">{template.id}</div>
                    <div class="detail-term">"Origine"</div>
                    <div class="detail-val">
                        {if template.is_local { "Modèle local" } else { "Modèle catalogue" }}
                    </div>
                    <div class="detail-term">"Description"</div>
                    <div class="detail-val">{desc}</div>
                    <div class="detail-term">"Version"</div>
                    <div class="detail-val text-mono">{format!("v{}", template.version)}</div>
                    <div class="detail-term">"Masque de fichiers"</div>
                    <div class="detail-val text-mono">{template.file_mask}</div>
                </div>
            </div>

            <div class="inspection-panel__section">
                <p class="inspection-panel__title">"Structure"</p>
                <div class="detail-grid">
                    <div class="detail-term">"Blocs"</div>
                    <div class="detail-val">{total_blocks.to_string()}</div>
                    <div class="detail-term">"Liaisons"</div>
                    <div class="detail-val">{total_connections.to_string()}</div>
                </div>
            </div>

            <FlowPipelineSection definition=def_str empty_label="Modèle vide" />

            {move || feedback.get().map(|err| view! {
                <p class="form-help text-danger" role="status">{err}</p>
            })}

            <div class="inspection-drawer__actions">
                {can_manage.then(|| view! {
                    <button
                        type="button"
                        class="btn btn-primary"
                        disabled=move || create_from_tpl.pending().get()
                        on:click=move |_| { create_from_tpl.dispatch(()); }
                    >
                        "Utiliser ce modèle"
                    </button>
                })}
                {(can_delete && template.is_local).then(|| view! {
                    <button
                        type="button"
                        class="btn btn-danger-quiet"
                        disabled=move || remove.pending().get()
                        on:click=move |_| { remove.dispatch(()); }
                    >
                        "Supprimer"
                    </button>
                })}
            </div>
        </div>
    }
}

#[component]
pub fn FlowsPage(me: Me) -> impl IntoView {
    view! { <FlowsSection me=me /> }
}

#[component]
pub fn FlowsSection(me: Me, #[prop(optional)] on_changed: Option<Callback<()>>) -> impl IntoView {
    let can_manage = me.allows(Permission::ManageFlows);
    let can_templates = me.allows(Permission::ManageSettings);
    let can_libraries = me.allows(Permission::ViewLibraries);
    let filters = use_context::<Filters>();
    let reload = RwSignal::new(0_u32);
    let view_mode = RwSignal::new(FlowsCatalogFilter::Flows);
    let selected_entry = RwSignal::new(None::<SelectedFlowEntry>);
    let flow_drawer_open = RwSignal::new(false);
    let tpl_drawer_open = RwSignal::new(false);
    let flow_tab = RwSignal::new(0_usize);

    let flows = LocalResource::new(move || {
        reload.track();
        api::get::<Vec<FlowRecord>>(FLOWS_PATH)
    });
    let local_snapshot = LocalResource::new(move || {
        reload.track();
        api::get::<LocalLibrarySnapshot>(LOCAL_PATH)
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
    let on_removed = Callback::new(move |()| {
        reload.update(|count| *count = count.wrapping_add(1));
        if let Some(cb) = on_changed {
            cb.run(());
        }
    });
    let close_drawers = Callback::new(move |()| {
        flow_drawer_open.set(false);
        tpl_drawer_open.set(false);
        selected_entry.set(None);
    });
    Effect::new(move |_| {
        if !flow_drawer_open.get()
            && matches!(
                selected_entry.get_untracked(),
                Some(SelectedFlowEntry::Flow(_))
            )
        {
            selected_entry.set(None);
        }
    });
    Effect::new(move |_| {
        if !tpl_drawer_open.get()
            && matches!(
                selected_entry.get_untracked(),
                Some(SelectedFlowEntry::Template(_))
            )
        {
            selected_entry.set(None);
        }
    });

    let open_flow = move |id: String| {
        tpl_drawer_open.set(false);
        flow_tab.set(0);
        selected_entry.set(Some(SelectedFlowEntry::Flow(id)));
        flow_drawer_open.set(true);
    };
    let open_template = move |id: String| {
        flow_drawer_open.set(false);
        selected_entry.set(Some(SelectedFlowEntry::Template(id)));
        tpl_drawer_open.set(true);
    };

    let router = use_router();
    if can_manage {
        provide_topbar_action(ActionSpec {
            label: "Nouveau flux",
            on_click: Callback::new(move |()| router.go(Route::NewFlow)),
        });
    }
    let stats = Signal::derive(move || {
        let list = flows.get().and_then(Result::ok).unwrap_or_default();
        let templates_count = local_snapshot
            .get()
            .and_then(Result::ok)
            .map_or(0, |s| s.flow_templates.len());
        let libs = libraries.get().unwrap_or_default();
        let counts = flow_usage_counts(&list, &libs);
        let active = flow_filter();
        let mode = view_mode.get();
        let set_filter = move |next: FlowFilter| {
            Callback::new(move |()| {
                view_mode.set(FlowsCatalogFilter::Flows);
                if let Some(filters) = filters {
                    filters.flow.set(next);
                }
            })
        };
        let mut items = vec![
            StatItem::new("Flows", counts.total.to_string(), MetricTone::Neutral).with_filter(
                mode == FlowsCatalogFilter::Flows && active == FlowFilter::All,
                set_filter(FlowFilter::All),
            ),
        ];
        if can_libraries {
            items.push(
                StatItem::new("Utilisés", counts.used.to_string(), MetricTone::Success)
                    .with_detail("≥ 1 bibliothèque")
                    .with_filter(
                        mode == FlowsCatalogFilter::Flows && active == FlowFilter::Used,
                        set_filter(FlowFilter::Used),
                    ),
            );
            let unused_tone = if counts.unused > 0 {
                MetricTone::Warning
            } else {
                MetricTone::Neutral
            };
            items.push(
                StatItem::new("Inutilisés", counts.unused.to_string(), unused_tone)
                    .with_detail("aucune bibliothèque")
                    .with_filter(
                        mode == FlowsCatalogFilter::Flows && active == FlowFilter::Unused,
                        set_filter(FlowFilter::Unused),
                    ),
            );
        }
        items.push(
            StatItem::new("Modèles", templates_count.to_string(), MetricTone::Neutral).with_filter(
                mode == FlowsCatalogFilter::Templates,
                Callback::new(move |()| view_mode.set(FlowsCatalogFilter::Templates)),
            ),
        );
        items.push(
            StatItem::new(
                "Tous",
                (counts.total + templates_count).to_string(),
                MetricTone::Neutral,
            )
            .with_filter(
                mode == FlowsCatalogFilter::All,
                Callback::new(move |()| view_mode.set(FlowsCatalogFilter::All)),
            ),
        );
        items
    });

    const HEADERS: [&str; 6] = [
        "Flow",
        "Masque",
        "Structure",
        "Bibliothèques",
        "Mise à jour",
        "Action",
    ];

    let registre_body = move || {
        let text = query();
        let active_filter = flow_filter();
        let mode = view_mode.get();
        let libs = libraries.get().unwrap_or_default();
        let templates = local_snapshot
            .get()
            .and_then(Result::ok)
            .map(|s| s.flow_templates)
            .unwrap_or_default();
        let shown_templates: Vec<LocalFlowTemplate> = if matches!(
            mode,
            FlowsCatalogFilter::Templates | FlowsCatalogFilter::All
        ) {
            templates
                .into_iter()
                .filter(|tpl| matches(&[&tpl.name, &tpl.id, &tpl.file_mask], &text))
                .collect()
        } else {
            Vec::new()
        };

        let render_template_rows = move |items: Vec<LocalFlowTemplate>| {
            items
                .into_iter()
                .map(|tpl| {
                    let def_str =
                        serde_json::to_string(&tpl.definition).unwrap_or_else(|_| "{}".into());
                    let (_, total_blocks, total_connections) = pipeline_preview_steps(&def_str);
                    let tpl_id = tpl.id.clone();
                    let row_id = tpl.id.clone();
                    let inspect_id = tpl.id.clone();
                    let desc = tpl
                        .description
                        .filter(|value| !value.trim().is_empty());
                    view! {
                        <tr
                            class="registre-row is-clickable"
                            class:is-open=move || {
                                selected_entry.get()
                                    == Some(SelectedFlowEntry::Template(row_id.clone()))
                            }
                            tabindex="0"
                            on:click={
                                let tpl_id = tpl_id.clone();
                                move |_| open_template(tpl_id.clone())
                            }
                            on:keydown=on_activate(move || open_template(tpl_id.clone()))
                        >
                            <td>
                                <div class="cluster cluster-wrap">
                                    <span class="text-strong">{tpl.name}</span>
                                    <Tag
                                        label=if tpl.is_local {
                                            "Modèle local"
                                        } else {
                                            "Modèle catalogue"
                                        }
                                        tone=if tpl.is_local {
                                            TagTone::Success
                                        } else {
                                            TagTone::Primary
                                        }
                                    />
                                    <Tag label=format!("v{}", tpl.version) tone=TagTone::Mono />
                                </div>
                                {desc.map(|text| view! { <div class="text-dim text-xs">{text}</div> })}
                            </td>
                            <td>
                                <Tag label=tpl.file_mask tone=TagTone::Primary />
                            </td>
                            <td class="text-dim">
                                {format!("{total_blocks} bloc(s) · {total_connections} liaison(s)")}
                            </td>
                            <td class="text-dim">"—"</td>
                            <td class="text-mono text-xs text-dim">{tpl.id}</td>
                            <td class="cell-end">
                                <button
                                    type="button"
                                    class="btn btn-secondary btn-sm"
                                    on:click=move |ev| {
                                        ev.stop_propagation();
                                        open_template(inspect_id.clone());
                                    }
                                >
                                    "Inspecter"
                                </button>
                            </td>
                        </tr>
                    }
                })
                .collect_view()
        };

        if mode == FlowsCatalogFilter::Templates {
            if shown_templates.is_empty() {
                return view! {
                    <RegistreMessage
                        span=HEADERS.len()
                        text="Aucun modèle de flow. Transformez un flow en modèle ou synchronisez un dépôt."
                    />
                }
                .into_any();
            }
            return render_template_rows(shown_templates).into_any();
        }

        match flows.get() {
            None => view! {
                <RegistreMessage span=HEADERS.len() text="Chargement des flux…" />
            }
            .into_any(),
            Some(Err(error)) => view! {
                <RegistreMessage span=HEADERS.len() text=error.message() />
            }
            .into_any(),
            Some(Ok(list)) => {
                let shown_flows: Vec<FlowRecord> = list
                    .into_iter()
                    .filter(|flow| {
                        let usage_ok = !can_libraries
                            || active_filter.accepts(bound_library_count(&flow.id, &libs));
                        usage_ok && matches(&[&flow.name, &flow.id, &flow.file_mask], &text)
                    })
                    .collect();

                if shown_flows.is_empty() && shown_templates.is_empty() {
                    let unfiltered = text.trim().is_empty() && active_filter == FlowFilter::All;
                    return view! {
                        <RegistreMessage
                            span=HEADERS.len()
                            text=if unfiltered {
                                "Aucun flux installé."
                            } else {
                                "Aucun flux correspondant aux filtres."
                            }
                        />
                    }
                    .into_any();
                }

                view! {
                    {shown_flows.into_iter().map(|flow| {
                        let (_, total_blocks, total_connections) =
                            pipeline_preview_steps(&flow.definition);
                        let bound_libraries = bound_library_count(&flow.id, &libs);
                        let flow_id = flow.id.clone();
                        let row_id = flow.id.clone();
                        let editor_id = flow.id.clone();
                        let desc = flow
                            .description
                            .filter(|value| !value.trim().is_empty());
                        view! {
                            <tr
                                class="registre-row is-clickable"
                                class:is-open=move || {
                                    selected_entry.get()
                                        == Some(SelectedFlowEntry::Flow(row_id.clone()))
                                }
                                tabindex="0"
                                on:click={
                                    let flow_id = flow_id.clone();
                                    move |_| open_flow(flow_id.clone())
                                }
                                on:keydown=on_activate(move || open_flow(flow_id.clone()))
                            >
                                <td>
                                    <div class="cluster cluster-wrap">
                                        <span class="text-strong">{flow.name}</span>
                                        {flow.is_default.then(|| view! {
                                            <StatusBadge label="Défaut" variant="success" />
                                        })}
                                        <Tag label=format!("v{}", flow.version) tone=TagTone::Mono />
                                    </div>
                                    {desc.map(|text| view! { <div class="text-dim text-xs">{text}</div> })}
                                </td>
                                <td>
                                    <Tag label=flow.file_mask tone=TagTone::Primary />
                                </td>
                                <td class="text-dim">
                                    {format!("{total_blocks} bloc(s) · {total_connections} liaison(s)")}
                                </td>
                                <td>
                                    <Tag
                                        label=flow_libs_label(bound_libraries)
                                        tone={if bound_libraries > 0 { TagTone::Success } else { TagTone::Neutral }}
                                        icon=Some(IconName::Libraries)
                                    />
                                </td>
                                <td class="text-dim">{format::datetime(flow.updated_at)}</td>
                                <td class="cell-end">
                                    <button
                                        type="button"
                                        class="btn btn-secondary btn-sm"
                                        on:click=move |ev| {
                                            ev.stop_propagation();
                                            router.go(Route::FlowEditor(editor_id.clone()));
                                        }
                                    >
                                        "Atelier"
                                    </button>
                                </td>
                            </tr>
                        }
                    }).collect_view()}
                    {render_template_rows(shown_templates)}
                }
                .into_any()
            }
        }
    };

    let flow_title = Signal::derive(move || match selected_entry.get() {
        Some(SelectedFlowEntry::Flow(id)) => flows
            .get()
            .and_then(Result::ok)
            .and_then(|list| list.into_iter().find(|f| f.id == id).map(|f| f.name))
            .unwrap_or(id),
        _ => String::new(),
    });
    let tpl_title = Signal::derive(move || match selected_entry.get() {
        Some(SelectedFlowEntry::Template(id)) => local_snapshot
            .get()
            .and_then(Result::ok)
            .and_then(|snap| {
                snap.flow_templates
                    .into_iter()
                    .find(|t| t.id == id)
                    .map(|t| t.name)
            })
            .unwrap_or(id),
        _ => String::new(),
    });

    view! {
        <div class="tab-panel" role="tabpanel">
            <StatStrip aria_label="Synthèse des flows" items=stats />
            <div class="registre">
                <Registre aria_label="Catalogue des flows" headers=HEADERS.to_vec()>
                    {registre_body}
                </Registre>
            </div>
            <Drawer
                open=flow_drawer_open
                eyebrow="Flux"
                title=flow_title
                tabs=vec!["Information", "Versions"]
                tab=flow_tab
            >
                {move || {
                    let Some(SelectedFlowEntry::Flow(id)) = selected_entry.get() else {
                        return None;
                    };
                    let flow = flows
                        .get()
                        .and_then(Result::ok)
                        .and_then(|list| list.into_iter().find(|f| f.id == id))?;
                    let libs = libraries.get().unwrap_or_default();
                    let bound_libs = bound_library_names(&flow.id, &libs);
                    Some(view! {
                        <FlowDetailDrawer
                            flow=flow
                            bound_libs=bound_libs
                            tab=flow_tab
                            can_manage=can_manage
                            can_templates=can_templates
                            on_changed=on_removed
                            on_closed=close_drawers
                        />
                    })
                }}
            </Drawer>
            <Drawer open=tpl_drawer_open eyebrow="Modèle" title=tpl_title>
                {move || {
                    let Some(SelectedFlowEntry::Template(id)) = selected_entry.get() else {
                        return None;
                    };
                    let template = local_snapshot
                        .get()
                        .and_then(Result::ok)
                        .and_then(|snap| snap.flow_templates.into_iter().find(|t| t.id == id))?;
                    Some(view! {
                        <FlowTemplateDetailDrawer
                            template=template
                            can_manage=can_manage
                            can_delete=can_templates
                            on_changed=on_removed
                            on_closed=close_drawers
                        />
                    })
                }}
            </Drawer>
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
                owner: None,
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
    fn flow_usage_counts_tracks_used_and_unused_flows() {
        let now = Utc::now();
        let make_flow = |id: &str| FlowRecord {
            id: id.into(),
            name: id.into(),
            description: None,
            file_mask: "*.mkv".into(),
            is_default: false,
            definition: String::new(),
            version: 1,
            created_at: now,
            updated_at: now,
        };
        let flows = [make_flow("f1"), make_flow("f2"), make_flow("f3")];
        let make_lib = |name: &str, flow_id: Option<&str>| Library {
            id: uuid::Uuid::nil(),
            name: name.into(),
            flow_id: flow_id.map(str::to_owned),
            workers: Vec::new(),
            scan_interval_minutes: 0,
            last_scanned_at: None,
            include_extensions: Vec::new(),
            exclude_patterns: Vec::new(),
            min_file_bytes: 0,
            output_policy: flux_shared::libraries::LibraryOutputPolicy::OutputDir,
            created_at: now,
            updated_at: now,
        };
        let libraries = [
            make_lib("Films", Some("f1")),
            make_lib("Séries", Some("f1")),
            make_lib("Orpheline", None),
        ];

        assert_eq!(bound_library_count("f1", &libraries), 2);
        assert_eq!(bound_library_count("f2", &libraries), 0);
        assert_eq!(
            bound_library_names("f1", &libraries),
            vec!["Films", "Séries"]
        );
        assert!(bound_library_names("f2", &libraries).is_empty());
        assert_eq!(
            flow_usage_counts(&flows, &libraries),
            FlowUsageCounts {
                total: 3,
                used: 1,
                unused: 2,
            }
        );
    }

    #[test]
    fn pipeline_preview_steps_identifies_domains_and_branches() {
        let catalog = test_support::test_catalog();
        let mut graph = FlowGraph::default();
        let entry = graph.append(
            FlowBlockKind::parse("block.core.file-input"),
            "Entrée",
            serde_json::json!({}),
            &catalog,
        );
        let cond = graph.append(
            FlowBlockKind::parse("block.core.condition"),
            "HEVC ?",
            serde_json::json!({}),
            &catalog,
        );
        let hook = graph.append(
            FlowBlockKind::parse("block.core.webhook"),
            "Webhook",
            serde_json::json!({}),
            &catalog,
        );
        let out = graph.append(
            FlowBlockKind::parse("block.core.file-output"),
            "Sortie",
            serde_json::json!({}),
            &catalog,
        );
        graph.connections.clear();
        assert!(graph.connect(&entry, None, &cond, &catalog).is_ok());
        assert!(
            graph
                .connect(&cond, Some("true".into()), &out, &catalog)
                .is_ok()
        );
        assert!(
            graph
                .connect(&cond, Some("false".into()), &hook, &catalog)
                .is_ok()
        );

        let json = serde_json::to_string(&graph).unwrap_or_default();
        let (steps, total_blocks, total_connections) = pipeline_preview_steps(&json);
        assert_eq!(total_blocks, 4);
        assert_eq!(total_connections, 3);
        assert_eq!(steps.len(), 4);
        assert_eq!(steps[0].domain, NodeDomain::Input);
        assert!(!steps[0].is_branching);
        assert_eq!(steps[1].domain, NodeDomain::Logic);
        assert!(steps[1].is_branching);
        assert_eq!(steps[2].domain, NodeDomain::Integration);
        assert_eq!(steps[3].domain, NodeDomain::Output);
    }
}
