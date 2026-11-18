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
    EmptyState, Icon, IconName, MetricTone, ResourceCard, StatItem, StatStrip, StatusBadge, Tag,
    TagTone, Toasts,
};
use crate::{api, format};
use flux_shared::Permission;
use flux_shared::flows::{FlowGraph, FlowRecord, UpdateFlowRequest, new_flow_id};
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

#[component]
fn FlowCard(
    flow: FlowRecord,
    bound_libraries: usize,
    can_manage: bool,
    can_templates: bool,
    on_removed: Callback<()>,
) -> impl IntoView {
    let router = use_router();
    let (steps, total_blocks, total_connections) = pipeline_preview_steps(&flow.definition);
    let extra_blocks = total_blocks.saturating_sub(steps.len());
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
            on_removed.run(());
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
        {can_manage.then(|| view! {
            <button type="button" class="btn btn-secondary btn-sm" disabled=move || duplicate.pending().get()
                on:click=move |_| { duplicate.dispatch(()); }>"Dupliquer"</button>
        })}
        {can_templates.then(|| view! {
            <button type="button" class="btn btn-secondary btn-sm" disabled=move || save_template.pending().get()
                on:click=move |_| { save_template.dispatch(()); }>"Modèle"</button>
        })}
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
            footer_meta=format!("{total_blocks} bloc(s) · {total_connections} liaison(s) · {updated}")
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
            <FlowPipelinePreview
                steps=steps
                extra_blocks=extra_blocks
                empty_label="Pipeline vide"
                aria_label="Aperçu du pipeline"
            />
            {move || feedback.get().map(|err| view! {
                <p class="text-danger text-xs" role="status">{err}</p>
            })}
        </ResourceCard>
    }
}

#[component]
fn FlowTemplateCard(
    template: LocalFlowTemplate,
    can_manage: bool,
    can_delete: bool,
    on_changed: Callback<()>,
) -> impl IntoView {
    let router = use_router();
    let def_str = serde_json::to_string(&template.definition).unwrap_or_else(|_| "{}".into());
    let (steps, total_blocks, total_connections) = pipeline_preview_steps(&def_str);
    let extra_blocks = total_blocks.saturating_sub(steps.len());
    let feedback = RwSignal::new(None::<String>);

    let name_for_create = template.name.clone();
    let desc_for_create = template.description.clone();
    let mask_for_create = template.file_mask.clone();
    let def_for_create = def_str;
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
            on_changed.run(());
        }
        Some(Err(error)) => Toasts::set_error(feedback, error.message()),
        None => {}
    });

    let badge = view! {
        <Tag
            label=if template.is_local { "Modèle local" } else { "Modèle catalogue" }
            tone=if template.is_local { TagTone::Success } else { TagTone::Primary }
        />
    }
    .into_any();
    let actions = view! {
        {can_manage.then(|| view! {
            <button type="button" class="btn btn-primary btn-sm" disabled=move || create_from_tpl.pending().get()
                on:click=move |_| { create_from_tpl.dispatch(()); }>"Utiliser ce modèle"</button>
        })}
        {(can_delete && template.is_local).then(|| view! {
            <button type="button" class="btn btn-sm btn-danger-quiet" disabled=move || remove.pending().get()
                on:click=move |_| { remove.dispatch(()); }>"Supprimer"</button>
        })}
    }
    .into_any();

    view! {
        <ResourceCard
            title=template.name
            subtitle=template.description
            icon=IconName::Layers
            tone=MetricTone::Neutral
            badge=Some(badge)
            footer_meta=format!("{total_blocks} bloc(s) · {total_connections} liaison(s) · {}", template.id)
            footer_actions=Some(actions)
        >
            <div class="cluster">
                <Tag label=format!("v{}", template.version) tone=TagTone::Mono />
                <Tag label=template.file_mask tone=TagTone::Primary />
            </div>
            <FlowPipelinePreview
                steps=steps
                extra_blocks=extra_blocks
                empty_label="Modèle vide"
                aria_label="Aperçu du modèle"
            />
            {move || feedback.get().map(|err| view! {
                <p class="text-danger text-xs" role="status">{err}</p>
            })}
        </ResourceCard>
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
    let grid = move || {
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

        if mode == FlowsCatalogFilter::Templates {
            if shown_templates.is_empty() {
                return view! {
                    <div class="empty-state-panel">
                        <EmptyState
                            title="Aucun modèle de flow"
                            message="Transformez un flow existant en modèle ou synchronisez un dépôt pour voir des modèles."
                            icon_name=IconName::Layers
                        />
                    </div>
                }
                .into_any();
            }
            return view! {
                <div class="resource-grid">
                    {shown_templates
                        .into_iter()
                        .map(|tpl| view! {
                            <FlowTemplateCard
                                template=tpl
                                can_manage=can_manage
                                can_delete=can_templates
                                on_changed=on_removed
                            />
                        })
                        .collect_view()}
                </div>
            }
            .into_any();
        }

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
                        let usage_ok = !can_libraries
                            || active_filter.accepts(bound_library_count(&flow.id, &libs));
                        usage_ok && matches(&[&flow.name, &flow.id, &flow.file_mask], &text)
                    })
                    .collect();
                if shown.is_empty() && shown_templates.is_empty() {
                    let unfiltered = text.trim().is_empty() && active_filter == FlowFilter::All;
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
                                let bound_libraries = bound_library_count(&flow.id, &libs);
                                view! {
                                    <FlowCard
                                        flow=flow
                                        bound_libraries=bound_libraries
                                        can_manage=can_manage
                                        can_templates=can_templates
                                        on_removed=on_removed
                                    />
                                }
                            })
                            .collect_view()}
                        {shown_templates
                            .into_iter()
                            .map(|tpl| view! {
                                <FlowTemplateCard
                                    template=tpl
                                    can_manage=can_manage
                                    can_delete=can_templates
                                    on_changed=on_removed
                                />
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
        let libraries = [
            Library {
                id: uuid::Uuid::nil(),
                name: "Films".into(),
                flow_id: Some("f1".into()),
                workers: Vec::new(),
                created_at: now,
                updated_at: now,
            },
            Library {
                id: uuid::Uuid::nil(),
                name: "Séries".into(),
                flow_id: Some("f1".into()),
                workers: Vec::new(),
                created_at: now,
                updated_at: now,
            },
            Library {
                id: uuid::Uuid::nil(),
                name: "Orpheline".into(),
                flow_id: None,
                workers: Vec::new(),
                created_at: now,
                updated_at: now,
            },
        ];

        assert_eq!(bound_library_count("f1", &libraries), 2);
        assert_eq!(bound_library_count("f2", &libraries), 0);
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
