//! Atelier d'édition de flux : barre de commande, palette, canevas, inspecteur, réglages et validation.

use super::canvas::GraphCanvas;
use super::inspector::NodeInspector;
use super::palette::Palette;
use super::presentation::{DiagnosticLevel, FlowDiagnostic, diagnose_graph};
use super::state::{Editor, incomplete_blocks};
use super::versions::FlowVersions;
use crate::ai::AiAssistantState;
use crate::api;
use crate::route::{Link, Route, encode_segment, use_router};
use crate::ui::{Icon, IconName, Toasts};
use chrono::{DateTime, Utc};
use flux_shared::Permission;
use flux_shared::flows::{FlowGraph, FlowRecord, OwnedBlockSpec, UpdateFlowRequest, new_flow_id};
use flux_shared::session::Me;
use leptos::ev;
use leptos::prelude::*;
use wasm_bindgen::JsCast;

const FLOWS_PATH: &str = "/api/v1/flows";
const BLOCKS_PATH: &str = "/api/v1/flows/blocks";
const DEFAULT_FILE_MASK: &str = "*";
const CMD_ICON_SIZE: u32 = 15;
const CHEVRON_ICON_SIZE: u32 = 18;

/// Flux encore jamais enregistré : graphe initialement vide en attendant le catalogue des plugins.
fn blank_flow() -> (FlowRecord, FlowGraph) {
    let graph = FlowGraph::default();
    let record = FlowRecord {
        id: String::new(),
        name: "Nouveau flux".into(),
        description: None,
        file_mask: DEFAULT_FILE_MASK.into(),
        is_default: false,
        definition: String::new(),
        version: 0,
        created_at: DateTime::<Utc>::UNIX_EPOCH,
        updated_at: DateTime::<Utc>::UNIX_EPOCH,
    };
    (record, graph)
}

/// Message bloquant l'enregistrement, s'il manque un réglage obligatoire.
pub fn save_blocker(graph: &FlowGraph, catalog: &[OwnedBlockSpec]) -> Option<String> {
    let incomplete = incomplete_blocks(graph, catalog);
    (!incomplete.is_empty()).then(|| {
        let list = incomplete
            .into_iter()
            .map(|(block, fields)| format!("{block} ({})", fields.join(", ")))
            .collect::<Vec<_>>()
            .join(" ; ");
        format!("Réglages obligatoires manquants : {list}.")
    })
}

fn is_text_input_target<T: JsCast>(target: Option<T>) -> bool {
    target
        .and_then(|node| node.dyn_into::<web_sys::Element>().ok())
        .is_some_and(|el| matches!(el.tag_name().as_str(), "INPUT" | "TEXTAREA" | "SELECT"))
}

#[component]
fn FlowCommandBar(
    editor: Editor,
    meta: RwSignal<FlowRecord>,
    diagnostics: Memo<Vec<FlowDiagnostic>>,
    can_manage: bool,
    saving: Signal<bool>,
    on_save: Callback<()>,
) -> impl IntoView {
    let editing_title = RwSignal::new(false);
    let draft_title = RwSignal::new(String::new());
    let title_input_ref = NodeRef::<leptos::html::Input>::new();

    Effect::new(move |_| {
        if editing_title.get()
            && let Some(input) = title_input_ref.get()
        {
            let _ = input.focus();
        }
    });

    let commit_title = move || {
        let trimmed = draft_title.get_untracked().trim().to_owned();
        if !trimmed.is_empty() && trimmed != meta.with_untracked(|r| r.name.clone()) {
            meta.update(|record| record.name = trimmed);
            editor.dirty.set(true);
        }
        editing_title.set(false);
    };

    let toggle_palette = move || editor.toggle_palette();

    let toggle_settings = move || {
        if editor.settings_open.get_untracked() {
            editor.close_right_panel();
        } else {
            editor.open_settings();
        }
    };

    let toggle_simulation = move || {
        if editor.simulation_open.get_untracked() {
            editor.close_right_panel();
        } else {
            editor.open_simulation();
        }
    };

    let ai_state = use_context::<AiAssistantState>();

    view! {
        <header class="flow-command-bar" role="toolbar" aria-label="Commandes de l'atelier">
            <div class="flow-island flow-island--identity">
                <Link to=Route::Flows class="header-icon-btn">
                    <span title="Retour aux flux" aria-label="Retour aux flux">
                        <Icon name=IconName::ChevronLeft size=CHEVRON_ICON_SIZE />
                    </span>
                </Link>

                <div class="flow-command-bar__identity">
                    <div class="flow-command-bar__title-row">
                        {move || {
                            if editing_title.get() && can_manage {
                                view! {
                                    <input
                                        node_ref=title_input_ref
                                        type="text"
                                        class="flow-title-input"
                                        aria-label="Nom du flux"
                                        prop:value=move || draft_title.get()
                                        on:input=move |event| draft_title.set(event_target_value(&event))
                                        on:blur=move |_| commit_title()
                                        on:keydown=move |event| {
                                            match event.key().as_str() {
                                                "Enter" => {
                                                    event.prevent_default();
                                                    commit_title();
                                                }
                                                "Escape" => {
                                                    event.prevent_default();
                                                    editing_title.set(false);
                                                }
                                                _ => {}
                                            }
                                        }
                                    />
                                }
                                .into_any()
                            } else {
                                view! {
                                    <button
                                        type="button"
                                        class="flow-title-btn"
                                        disabled=!can_manage
                                        title="Cliquer pour renommer le flux"
                                        on:click=move |_| {
                                            if !can_manage {
                                                return;
                                            }
                                            draft_title.set(meta.with_untracked(|r| r.name.clone()));
                                            editing_title.set(true);
                                        }
                                    >
                                        <span class="flow-title-btn__text">{move || meta.get().name}</span>
                                    </button>
                                }
                                .into_any()
                            }
                        }}
                    </div>

                    <div class="flow-command-bar__pills">
                        <button
                            type="button"
                            class="flow-pill flow-pill--mask text-mono"
                            title="Modifier le masque de fichiers et les réglages"
                            on:click=move |_| toggle_settings()
                        >
                            {move || meta.get().file_mask}
                        </button>
                        {move || meta.get().is_default.then(|| view! {
                            <span class="flow-pill flow-pill--default">"Par défaut"</span>
                        })}
                        <span class="flow-pill flow-pill--version">
                            {move || match meta.get().version {
                                0 => "Nouveau".to_owned(),
                                version => format!("v{version}"),
                            }}
                        </span>
                        <span
                            class="flow-pill flow-pill--state"
                            class:is-unsaved=move || editor.dirty.get()
                            class:is-saved=move || !editor.dirty.get()
                            role="status"
                        >
                            <span class="status-dot" aria-hidden="true"></span>
                            <span>{move || if editor.dirty.get() { "Modifié" } else { "Enregistré" }}</span>
                        </span>
                    </div>
                </div>
            </div>

            <div class="flow-island flow-island--tools">
                {can_manage.then(|| view! {
                    <>
                        <button
                            type="button"
                            class="btn btn-secondary btn-sm flow-cmd-btn"
                            aria-pressed=move || editor.palette_open.get().to_string()
                            title="Afficher / masquer les composants (Ctrl+B ou /)"
                            on:click=move |_| toggle_palette()
                        >
                            <Icon name=IconName::Plus size=CMD_ICON_SIZE />
                            <span>"Composants"</span>
                        </button>
                        <span class="flow-island__sep" aria-hidden="true"></span>
                    </>
                })}

                <button
                    type="button"
                    class="btn btn-secondary btn-sm flow-cmd-btn"
                    aria-pressed=move || editor.diagnostics_open.get().to_string()
                    title="Vérifier la validité du flux"
                    on:click=move |_| editor.diagnostics_open.update(|open| *open = !*open)
                >
                    {move || {
                        let count = diagnostics.with(Vec::len);
                        let has_err = diagnostics
                            .with(|list| list.iter().any(|d| d.level == DiagnosticLevel::Error));
                        let icon = if count > 0 { IconName::Alert } else { IconName::Check };
                        view! {
                            <Icon name=icon size=CMD_ICON_SIZE />
                            <span>"Vérifier"</span>
                            {(count > 0).then(|| view! {
                                <span
                                    class="flow-cmd-btn__count"
                                    class:is-error=has_err
                                >
                                    {count}
                                </span>
                            })}
                        }
                    }}
                </button>

                <button
                    type="button"
                    class="btn btn-secondary btn-sm flow-cmd-btn"
                    aria-pressed=move || editor.simulation_open.get().to_string()
                    title="Simuler l'exécution sur un média type"
                    on:click=move |_| toggle_simulation()
                >
                    <Icon name=IconName::Activity size=CMD_ICON_SIZE />
                    <span>"Simuler"</span>
                </button>

                <button
                    type="button"
                    class="btn btn-secondary btn-sm flow-cmd-btn"
                    aria-pressed=move || editor.settings_open.get().to_string()
                    title="Réglages généraux et historique des versions"
                    on:click=move |_| toggle_settings()
                >
                    <Icon name=IconName::Sliders size=CMD_ICON_SIZE />
                    <span>"Réglages"</span>
                </button>

                {ai_state.map(|ai| view! {
                    <button
                        type="button"
                        class="btn btn-secondary btn-sm flow-cmd-btn"
                        title="Analyser ou optimiser ce flux avec l'assistant IA"
                        on:click=move |_| {
                            let record = meta.get_untracked();
                            let prompt = if record.id.is_empty() {
                                "Quels blocs et bonnes pratiques recommandes-tu pour structurer ce nouveau flux ?".to_owned()
                            } else {
                                format!(
                                    "Analyse le flux `{}` (id: `{}`), vérifie sa validité avec `validate_flow` / `simulate_flow` et propose d'éventuelles améliorations.",
                                    record.name, record.id
                                )
                            };
                            let hint = Some(format!("Atelier de flux — {} ({})", record.name, record.id));
                            ai.ask(prompt, hint);
                        }
                    >
                        <Icon name=IconName::Sparkles size=CMD_ICON_SIZE />
                        <span>"Assistant IA"</span>
                    </button>
                })}
            </div>

            <div class="flow-island flow-island--publish">
                <span class="flow-pill flow-pill--count">
                    {move || {
                        let count = editor.graph.with(|g| g.nodes.len());
                        if count <= 1 {
                            format!("{count} bloc")
                        } else {
                            format!("{count} blocs")
                        }
                    }}
                </span>
                {can_manage.then(|| view! {
                    <button
                        type="button"
                        class="btn btn-primary btn-sm"
                        title="Enregistrer le flux (Ctrl+S)"
                        disabled=move || saving.get() || (!editor.dirty.get() && meta.get().version > 0)
                        on:click=move |_| on_save.run(())
                    >
                        {move || if saving.get() { "Enregistrement…" } else { "Enregistrer" }}
                    </button>
                })}
            </div>
        </header>
    }
}

#[component]
fn FlowSettingsPane(
    editor: Editor,
    meta: RwSignal<FlowRecord>,
    feedback: RwSignal<Option<String>>,
    can_manage: bool,
) -> impl IntoView {
    view! {
        <div class="flow-dock-pane flow-settings-pane" aria-label="Réglages et versions du flux">
            {move || {
                if !editor.settings_open.get() {
                    return None;
                }
                let record = meta.get_untracked();
                Some(view! {
                    <div class="flow-inspector__shell">
                        <header class="flow-inspector__header">
                            <div class="flow-inspector__identity">
                                <span class="flow-inspector__eyebrow">"Métadonnées & Historique"</span>
                                <h2 class="flow-inspector__title">"Réglages du flux"</h2>
                            </div>
                        </header>

                        <div class="flow-inspector__body">
                            <div class="form-group">
                                <label for="flow-settings-name" class="form-label">"Nom du flux"</label>
                                <input
                                    id="flow-settings-name"
                                    class="form-input"
                                    disabled=!can_manage
                                    prop:value=move || meta.get().name
                                    on:input=move |event| {
                                        meta.update(|r| r.name = event_target_value(&event));
                                        editor.dirty.set(true);
                                    }
                                />
                            </div>
                            <div class="form-group">
                                <label for="flow-settings-mask" class="form-label">"Masque de fichiers"</label>
                                <input
                                    id="flow-settings-mask"
                                    class="form-input text-mono"
                                    disabled=!can_manage
                                    prop:value=move || meta.get().file_mask
                                    on:input=move |event| {
                                        meta.update(|r| r.file_mask = event_target_value(&event));
                                        editor.dirty.set(true);
                                    }
                                />
                            </div>
                            <div class="form-group">
                                <label for="flow-settings-desc" class="form-label">"Description"</label>
                                <input
                                    id="flow-settings-desc"
                                    class="form-input"
                                    placeholder="Description optionnelle…"
                                    disabled=!can_manage
                                    prop:value=move || meta.get().description.unwrap_or_default()
                                    on:input=move |event| {
                                        let text = event_target_value(&event);
                                        meta.update(|r| r.description = (!text.trim().is_empty()).then_some(text));
                                        editor.dirty.set(true);
                                    }
                                />
                            </div>
                            <label class="flow-settings__toggle">
                                <input
                                    type="checkbox"
                                    disabled=!can_manage
                                    prop:checked=move || meta.get().is_default
                                    on:change=move |event| {
                                        meta.update(|r| r.is_default = event_target_checked(&event));
                                        editor.dirty.set(true);
                                    }
                                />
                                <span>"Utiliser comme flux par défaut"</span>
                            </label>

                            {(!record.id.is_empty() && record.version > 0).then(|| view! {
                                <div class="flow-inspector__section">
                                    <h3 class="flow-inspector__section-title">"Versions publiées"</h3>
                                    <FlowVersions
                                        flow_id=record.id
                                        current=record.version
                                        editor=editor
                                        meta=meta
                                        on_restored=Callback::new(move |version: i64| {
                                            editor.close_right_panel();
                                            let msg = format!(
                                                "Version {version} chargée : enregistrez pour la publier."
                                            );
                                            Toasts::notify_info(msg.clone());
                                            feedback.set(Some(msg));
                                        })
                                    />
                                </div>
                            })}
                        </div>
                    </div>
                })
            }}
        </div>
    }
}

#[component]
fn LeftPaletteDock(editor: Editor, can_manage: bool) -> impl IntoView {
    if !can_manage {
        return ().into_any();
    }
    view! {
        <aside
            class="flow-palette-dock"
            class:is-closed=move || !editor.palette_open.get()
            aria-label="Catalogue de composants"
        >
            <Palette editor=editor />
        </aside>
    }
    .into_any()
}

#[component]
fn RightDock(
    editor: Editor,
    meta: RwSignal<FlowRecord>,
    feedback: RwSignal<Option<String>>,
    can_manage: bool,
) -> impl IntoView {
    let is_open = move || editor.any_right_panel_open();
    let has_selection = move || editor.selected.get().is_some();

    view! {
        <aside
            class="flow-right-dock"
            class:is-closed=move || !is_open()
            aria-label="Panneau latéral de l'atelier"
        >
            <div class="flow-right-dock__bar">
                <div class="flow-right-dock__tabs" role="tablist" aria-label="Vues du panneau">
                    <button
                        type="button"
                        role="tab"
                        class="flow-right-dock__tab"
                        disabled=move || !has_selection()
                        aria-selected=move || {
                            (editor.inspector_open.get() && has_selection()).to_string()
                        }
                        on:click=move |_| editor.open_inspector()
                    >
                        "Bloc"
                    </button>
                    <button
                        type="button"
                        role="tab"
                        class="flow-right-dock__tab"
                        aria-selected=move || editor.simulation_open.get().to_string()
                        on:click=move |_| editor.open_simulation()
                    >
                        "Simuler"
                    </button>
                    <button
                        type="button"
                        role="tab"
                        class="flow-right-dock__tab"
                        aria-selected=move || editor.settings_open.get().to_string()
                        on:click=move |_| editor.open_settings()
                    >
                        "Réglages"
                    </button>
                </div>
                <button
                    type="button"
                    class="header-icon-btn"
                    title="Fermer le panneau (Échap)"
                    aria-label="Fermer le panneau"
                    on:click=move |_| editor.close_right_panel()
                >
                    <Icon name=IconName::Close size=CMD_ICON_SIZE />
                </button>
            </div>
            <div class="flow-right-dock__body">
                {move || {
                    if editor.inspector_open.get() && has_selection() {
                        view! { <NodeInspector editor=editor can_manage=can_manage /> }.into_any()
                    } else if editor.simulation_open.get() {
                        view! { <super::simulation::FlowSimulationPanel editor=editor /> }.into_any()
                    } else if editor.settings_open.get() {
                        view! {
                            <FlowSettingsPane
                                editor=editor
                                meta=meta
                                feedback=feedback
                                can_manage=can_manage
                            />
                        }
                        .into_any()
                    } else {
                        ().into_any()
                    }
                }}
            </div>
        </aside>
    }
}

#[component]
fn FlowDiagnosticsTray(editor: Editor, diagnostics: Memo<Vec<FlowDiagnostic>>) -> impl IntoView {
    view! {
        <section
            class="flow-diagnostics-tray"
            class:is-closed=move || !editor.diagnostics_open.get()
            aria-label="Validation du flux"
        >
            {move || {
                if !editor.diagnostics_open.get() {
                    return None;
                }
                let list = diagnostics.get();
                let count = list.len();
                Some(view! {
                    <div class="flow-diagnostics__inner">
                        <header class="flow-diagnostics__header">
                            <div class="flow-diagnostics__title-wrap">
                                <h2 class="flow-diagnostics__title">"Validation du flux"</h2>
                                <span class="flow-diagnostics__count">
                                    {if count == 0 {
                                        "Aucune alerte".to_owned()
                                    } else {
                                        format!("{count} point(s) à vérifier")
                                    }}
                                </span>
                            </div>
                            <button
                                type="button"
                                class="header-icon-btn"
                                aria-label="Fermer le panneau de validation"
                                on:click=move |_| editor.diagnostics_open.set(false)
                            >
                                <Icon name=IconName::Close size=15 />
                            </button>
                        </header>

                        <div class="flow-diagnostics__list">
                            {if list.is_empty() {
                                view! {
                                    <div class="flow-diagnostic-item" data-level="ok">
                                        <Icon name=IconName::Check size=15 />
                                        <span>"Le graphe est valide et complet."</span>
                                    </div>
                                }
                                .into_any()
                            } else {
                                list.into_iter()
                                    .map(|diag| {
                                        let node_id = diag.node_id;
                                        let level_label = match diag.level {
                                            DiagnosticLevel::Error => "Erreur",
                                            DiagnosticLevel::Warning => "Alerte",
                                        };
                                        view! {
                                            <div class="flow-diagnostic-item" data-level=diag.level.as_attr()>
                                                <span class="flow-diagnostic-item__level">{level_label}</span>
                                                <span class="flow-diagnostic-item__msg">{diag.message}</span>
                                                {node_id.map(|id| view! {
                                                    <button
                                                        type="button"
                                                        class="btn btn-secondary btn-sm"
                                                        on:click=move |_| editor.select_node(Some(id.clone()), true)
                                                    >
                                                        "Inspecter"
                                                    </button>
                                                })}
                                            </div>
                                        }
                                    })
                                    .collect_view()
                                    .into_any()
                            }}
                        </div>
                    </div>
                })
            }}
        </section>
    }
}

#[component]
fn Workspace(me: Me, record: FlowRecord, graph: FlowGraph) -> impl IntoView {
    let can_manage = me.allows(Permission::ManageFlows);
    let router = use_router();
    let initial_id = (!record.id.is_empty()).then(|| record.id.clone());
    let editor = Editor::for_flow(initial_id, graph);
    let meta = RwSignal::new(record);

    Effect::new(move |_| router.set_unsaved(editor.dirty.get()));
    on_cleanup(move || router.set_unsaved(false));

    let feedback = RwSignal::new(None::<String>);
    let blocks = LocalResource::new(|| api::get::<Vec<OwnedBlockSpec>>(BLOCKS_PATH));
    Effect::new(move |_| match blocks.get() {
        Some(Ok(catalog)) => {
            let should_seed = meta.with_untracked(|m| m.id.is_empty())
                && editor.graph.with_untracked(|g| g.nodes.is_empty());
            if should_seed && let Some(entry) = catalog.iter().find(|s| !s.has_input) {
                let mut seeded = FlowGraph::default();
                seeded.append(
                    entry.kind_enum(),
                    &entry.label,
                    entry.default_config(),
                    &catalog,
                );
                editor.seed_initial_graph(seeded);
            }
            editor.catalog.set(catalog);
            editor.catalog_loaded.set(true);
        }
        Some(Err(_)) => {
            editor.catalog_loaded.set(true);
        }
        None => {}
    });

    let diagnostics = Memo::new(move |_| {
        editor
            .graph
            .with(|g| editor.catalog.with(|c| diagnose_graph(g, c)))
    });

    let save = Action::new_local(move |(id, body): &(String, UpdateFlowRequest)| {
        let path = format!("{FLOWS_PATH}/{}", encode_segment(id));
        let body = body.clone();
        async move {
            api::put_with_header::<UpdateFlowRequest, FlowRecord>(&path, &body, "x-flux-warnings")
                .await
        }
    });
    Effect::new(move |_| match save.value().get() {
        Some(Ok((saved, warnings))) => {
            let created = meta.get_untracked().id.is_empty();
            let msg = format!("Enregistré : version {}.", saved.version);
            Toasts::notify_success(msg.clone());
            if let Some(warnings) = warnings {
                let warn_msg = format!("Avertissements de préparation : {warnings}");
                Toasts::notify_info(warn_msg.clone());
                feedback.set(Some(format!("{msg} — {warn_msg}")));
            } else {
                feedback.set(Some(msg));
            }
            editor.mark_saved(Some(saved.id.clone()));
            if created {
                router.go(Route::FlowEditor(saved.id.clone()));
            }
            meta.set(saved);
        }
        Some(Err(error)) => {
            Toasts::notify_error(error.message());
            feedback.set(Some(error.message()));
        }
        None => {}
    });

    let trigger_save = Callback::new(move |()| {
        if !can_manage || save.pending().get_untracked() {
            return;
        }
        let graph = editor.graph.get_untracked();
        let catalog = editor.catalog.get_untracked();
        if let Some(message) = save_blocker(&graph, &catalog) {
            Toasts::notify_error(message.clone());
            feedback.set(Some(message));
            editor.diagnostics_open.set(true);
            return;
        }
        let record = meta.get_untracked();
        let id = if record.id.is_empty() {
            new_flow_id(
                &record.name,
                &format!("{:x}", Utc::now().timestamp_millis()),
            )
        } else {
            record.id.clone()
        };
        match UpdateFlowRequest::keeping(&record, &graph) {
            Ok(body) if body.name.trim().is_empty() => {
                Toasts::notify_error("Donnez un nom au flux.");
            }
            Ok(body) => {
                save.dispatch((id, body));
            }
            Err(_) => {
                Toasts::notify_error("Graphe illisible : rien n'a été enregistré.");
            }
        }
    });

    let _shortcuts = window_event_listener(ev::keydown, move |event| {
        let ctrl = event.ctrl_key() || event.meta_key();
        let key = event.key();
        let in_input = is_text_input_target(event.target());
        if ctrl && key.eq_ignore_ascii_case("s") {
            event.prevent_default();
            trigger_save.run(());
            return;
        }
        if ctrl && key.eq_ignore_ascii_case("b") && can_manage {
            event.prevent_default();
            editor.toggle_palette();
            return;
        }
        if !ctrl && !in_input && can_manage && key == "/" {
            event.prevent_default();
            editor.focus_palette_search();
            return;
        }
        if ctrl && !in_input && key.eq_ignore_ascii_case("a") {
            event.prevent_default();
            editor.select_all_nodes();
            return;
        }
        if ctrl && !in_input && can_manage {
            if key.eq_ignore_ascii_case("d") {
                event.prevent_default();
                editor.duplicate_selected();
                return;
            }
            if key.eq_ignore_ascii_case("z") {
                event.prevent_default();
                if event.shift_key() {
                    editor.redo();
                } else {
                    editor.undo();
                }
                return;
            }
            if key.eq_ignore_ascii_case("y") {
                event.prevent_default();
                editor.redo();
                return;
            }
        }
        if !ctrl && !in_input && can_manage && matches!(key.as_str(), "Delete" | "Backspace") {
            if editor.disconnect_selected_wire() || editor.remove_selected_nodes() > 0 {
                event.prevent_default();
            }
            return;
        }
        if key == "Escape" {
            if editor.quick_add.get_untracked().is_some() {
                editor.quick_add.set(None);
            } else if editor.pending_wire.get_untracked().is_some() {
                editor.pending_wire.set(None);
            } else if editor.branch_target.get_untracked().is_some() {
                editor.branch_target.set(None);
            } else if editor.selected_wire.get_untracked().is_some() {
                editor.selected_wire.set(None);
            } else if editor.inspector_open.get_untracked()
                || editor.settings_open.get_untracked()
                || editor.simulation_open.get_untracked()
            {
                editor.close_right_panel();
            } else if editor.diagnostics_open.get_untracked() {
                editor.diagnostics_open.set(false);
            } else if editor.selected.get_untracked().is_some()
                || !editor
                    .selected_ids
                    .with_untracked(std::collections::HashSet::is_empty)
            {
                editor.clear_selection();
            } else if editor.palette_open.get_untracked() {
                editor.close_palette();
            }
        }
    });

    let saving = Signal::derive(move || save.pending().get());

    view! {
        <div class="flow-workbench">
            {(!can_manage).then(|| view! {
                <p class="flow-workbench__readonly" role="status">
                    "Lecture seule : modifier un flux demande le droit de gestion des flux."
                </p>
            })}

            {move || feedback.get().map(|text| view! {
                <p class="flow-workbench__feedback" role="status">{text}</p>
            })}

            <div
                class="flow-workspace-body"
                class:is-palette-open=move || can_manage && editor.palette_open.get()
                class:is-panel-open=move || editor.any_right_panel_open()
            >
                <FlowCommandBar
                    editor=editor
                    meta=meta
                    diagnostics=diagnostics
                    can_manage=can_manage
                    saving=saving
                    on_save=trigger_save
                />
                <LeftPaletteDock editor=editor can_manage=can_manage />
                <GraphCanvas editor=editor can_manage=can_manage />
                <RightDock
                    editor=editor
                    meta=meta
                    feedback=feedback
                    can_manage=can_manage
                />
                <FlowDiagnosticsTray editor=editor diagnostics=diagnostics />
            </div>
        </div>
    }
}

#[component]
pub fn FlowEditor(me: Me, flow_id: Option<String>) -> impl IntoView {
    let Some(id) = flow_id else {
        let (record, graph) = blank_flow();
        return view! { <Workspace me=me record=record graph=graph /> }.into_any();
    };
    let path = format!("{FLOWS_PATH}/{}", encode_segment(&id));
    let flow = LocalResource::new(move || {
        let path = path.clone();
        async move { api::get::<FlowRecord>(&path).await }
    });
    view! {
        {move || match flow.get() {
            None => view! { <p class="text-dim">"Chargement du flux…"</p> }.into_any(),
            Some(Err(error)) => view! { <p class="settings-alert" role="alert">{error.message()}</p> }.into_any(),
            Some(Ok(record)) => {
                let graph = FlowGraph::parse(&record.definition);
                view! { <Workspace me=me.clone() record=record graph=graph /> }.into_any()
            }
        }}
    }
    .into_any()
}

#[cfg(test)]
mod tests {
    use super::super::test_support::test_catalog;
    use super::*;
    use flux_shared::flows::FlowBlockKind;
    use serde_json::json;

    #[test]
    fn a_blank_flow_starts_empty_and_has_no_id() {
        let (record, graph) = blank_flow();
        assert!(record.id.is_empty());
        assert!(graph.nodes.is_empty());
    }

    #[test]
    fn saving_is_blocked_by_an_empty_required_field() {
        let catalog = test_catalog();
        let mut graph = FlowGraph::default();
        graph.append(
            FlowBlockKind::parse("essentiel.webhook"),
            "Hook",
            json!({}),
            &catalog,
        );
        assert!(
            save_blocker(&graph, &catalog).is_some_and(|message| message.contains("Hook (URL)"))
        );
        graph.nodes[0].config = json!({"url": "https://hook.example"});
        assert_eq!(save_blocker(&graph, &catalog), None);
        assert_eq!(encode_segment("flux à"), "flux%20%C3%A0");
    }
}
