//! Page d'édition d'un script local, en pleine page (ADR 0030) : le code à gauche, les propriétés,
//! variables et dépendances à droite. Remplace le tiroir de l'onglet Scripts.

use super::code_editor::CodeEditor;
use super::requirements_editor::{RequirementsEditor, requirement_rows_from, rows_to_requirements};
use super::script_draft::{
    CATEGORIES, OpenedScript, ScriptDraft, category_value, is_starter_code, opened_script_warning,
    parse_category, parse_tags, starter_script_code,
};
use super::variables_editor::{VariablesEditor, rows_from, rows_to_variables, variable_names};
use super::{LOCAL_PATH, LOCAL_SCRIPTS_PATH, LOCAL_TOPICS};
use crate::api;
use crate::live::{Loaded, load, use_live};
use crate::route::{Route, use_router};
use crate::ui::{Tag, TagTone, Toasts};
use forgearr_shared::Permission;
use forgearr_shared::local::{
    LocalLibrarySnapshot, LocalScriptSummary, ScriptInterpreter, UpsertLocalScriptRequest,
};
use forgearr_shared::session::Me;
use leptos::prelude::*;

/// Panneau affiché à droite du code.
#[derive(Clone, Copy, PartialEq, Eq)]
enum SidePanel {
    Properties,
    Variables,
    Dependencies,
}

impl SidePanel {
    const ALL: [Self; 3] = [Self::Properties, Self::Variables, Self::Dependencies];

    const fn label(self) -> &'static str {
        match self {
            Self::Properties => "Propriétés",
            Self::Variables => "Variables",
            Self::Dependencies => "Dépendances",
        }
    }
}

/// Titre de la page : le nom du script, ou « Nouveau script » tant qu'il n'en a pas.
fn page_title(name: &str) -> String {
    let name = name.trim();
    if name.is_empty() {
        "Nouveau script".to_owned()
    } else {
        name.to_owned()
    }
}

#[component]
pub fn ScriptEditor(me: Me, script_id: Option<String>) -> impl IntoView {
    let can_manage = me.allows(Permission::ManageSettings);
    let router = use_router();
    let live = use_live();
    let snapshot = load::<LocalLibrarySnapshot>(LOCAL_TOPICS, LOCAL_PATH);
    let draft = RwSignal::new(None::<ScriptDraft>);
    let missing = RwSignal::new(false);
    let feedback = RwSignal::new(None::<String>);
    let is_new = script_id.is_none();
    on_cleanup(move || router.set_unsaved(false));

    if is_new {
        draft.set(Some(ScriptDraft::blank()));
    } else {
        // Le script s'ouvre dès que la librairie est lue ; absent, la page le dit.
        Effect::new(move |_| {
            if draft.with_untracked(Option::is_some) {
                return;
            }
            if let Some(Ok(data)) = snapshot.get() {
                match data
                    .scripts
                    .iter()
                    .find(|s| Some(&s.id) == script_id.as_ref())
                {
                    Some(script) => draft.set(Some(ScriptDraft::from_summary(script))),
                    None => missing.set(true),
                }
            }
        });
    }

    let save = Action::new_local(move |req: &UpsertLocalScriptRequest| {
        let req = req.clone();
        async move {
            api::post::<UpsertLocalScriptRequest, LocalScriptSummary>(
                LOCAL_SCRIPTS_PATH,
                Some(&req),
            )
            .await
        }
    });
    Effect::new(move |_| match save.value().get() {
        Some(Ok(saved)) => {
            router.set_unsaved(false);
            Toasts::notify_success("Script enregistré dans la librairie locale.");
            live.touch(LOCAL_TOPICS);
            if is_new {
                router.go(Route::ScriptEditor(saved.id));
            } else {
                draft.set(Some(ScriptDraft::from_summary(&saved)));
            }
        }
        Some(Err(err)) => Toasts::set_error(feedback, err.message()),
        None => {}
    });

    let pending = Signal::derive(move || save.pending().get());
    let on_submit = Callback::new(move |req: UpsertLocalScriptRequest| {
        feedback.set(None);
        save.dispatch(req);
    });
    let on_reload = Callback::new(move |current: ScriptDraft| draft.set(Some(current)));

    view! {
        {move || draft.get().map(|opened| view! {
            <ScriptForm
                draft=opened
                snapshot=snapshot
                can_manage=can_manage
                pending=pending
                feedback=feedback
                on_submit=on_submit
                on_reload=on_reload
            />
        })}
        {move || (draft.with(Option::is_none) && missing.get()).then(|| view! {
            <div class="flow-workbench">
                <p class="flow-workbench__feedback" role="status">
                    "Ce script n'existe pas ou a été supprimé de la librairie."
                </p>
                <div class="script-workbench__bar">
                    <button type="button" class="btn btn-secondary"
                        on:click=move |_| router.go(Route::Libraries)>"Retour aux librairies"</button>
                </div>
            </div>
        })}
        {move || (draft.with(Option::is_none) && !missing.get()).then(|| view! {
            <div class="flow-workbench">
                <p class="flow-workbench__feedback" role="status">"Chargement du script…"</p>
            </div>
        })}
    }
}

#[component]
fn ScriptForm(
    draft: ScriptDraft,
    snapshot: Loaded<LocalLibrarySnapshot>,
    can_manage: bool,
    pending: Signal<bool>,
    feedback: RwSignal<Option<String>>,
    on_submit: Callback<UpsertLocalScriptRequest>,
    on_reload: Callback<ScriptDraft>,
) -> impl IntoView {
    let router = use_router();
    let read_only = !can_manage;
    let opened = StoredValue::new(draft.clone());
    let opened_state = Memo::new(move |_| {
        snapshot.with(|loaded| match loaded {
            Some(Ok(data)) => opened.with_value(|origin| OpenedScript::of(origin, &data.scripts)),
            _ => OpenedScript::Unchanged,
        })
    });

    let id = draft.id;
    let name = RwSignal::new(draft.name);
    let desc = RwSignal::new(draft.description);
    let interp = RwSignal::new(draft.interpreter);
    let filename = RwSignal::new(draft.filename);
    let code = RwSignal::new(draft.code);
    let category = RwSignal::new(draft.category);
    let tags = RwSignal::new(draft.tags);
    let variable_rows = RwSignal::new(rows_from(draft.variables));
    let requirement_rows = RwSignal::new(requirement_rows_from(draft.requires));
    let dependencies_unverified = RwSignal::new(draft.dependencies_unverified);
    let panel = RwSignal::new(SidePanel::Properties);
    let extension = Signal::derive(move || interp.get().default_extension());
    let declared = Signal::derive(move || variable_names(&variable_rows.get()));

    // Toute saisie après l'ouverture déclare des modifications (confirmation en quittant).
    let dirty = RwSignal::new(false);
    let opening = StoredValue::new(true);
    Effect::new(move |_| {
        name.track();
        desc.track();
        interp.track();
        filename.track();
        code.track();
        category.track();
        tags.track();
        variable_rows.track();
        requirement_rows.track();
        dependencies_unverified.track();
        if opening.get_value() {
            opening.set_value(false);
        } else {
            dirty.set(true);
            router.set_unsaved(true);
        }
    });

    let submit = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        if !can_manage {
            return;
        }
        let raw_filename = filename.get_untracked();
        let trimmed_filename = raw_filename.trim();
        let variables = match rows_to_variables(&variable_rows.get_untracked()) {
            Ok(variables) => variables,
            Err(message) => {
                Toasts::set_error(feedback, message);
                return;
            }
        };
        let requires = match rows_to_requirements(&requirement_rows.get_untracked()) {
            Ok(requires) => requires,
            Err(message) => {
                Toasts::set_error(feedback, message);
                return;
            }
        };
        on_submit.run(UpsertLocalScriptRequest {
            requires,
            dependencies_unverified: dependencies_unverified.get_untracked(),
            variables,
            id: id.clone(),
            name: name.get_untracked(),
            description: Some(desc.get_untracked()),
            interpreter: interp.get_untracked(),
            code: code.get_untracked(),
            filename: (!trimmed_filename.is_empty()).then(|| trimmed_filename.to_owned()),
            category: category.get_untracked(),
            tags: parse_tags(&tags.get_untracked()),
        });
    };

    let change_interpreter = move |raw: String| {
        let previous = interp.get_untracked();
        let next = ScriptInterpreter::from_executable(&raw).unwrap_or(previous);
        if is_starter_code(&code.get_untracked()) {
            code.set(starter_script_code(next).to_owned());
        }
        let current_filename = filename.get_untracked();
        let default_previous = format!("run.{}", previous.default_extension());
        if current_filename.trim().is_empty() || current_filename.trim() == default_previous {
            filename.set(format!("run.{}", next.default_extension()));
        }
        interp.set(next);
    };

    view! {
        <form class="flow-workbench script-workbench" on:submit=submit>
            <header class="script-workbench__bar">
                <button type="button" class="btn btn-secondary"
                    on:click=move |_| router.go(Route::Libraries)>"← Librairies"</button>
                <h1 class="script-workbench__title">{move || page_title(&name.get())}</h1>
                {move || dirty.get().then(|| view! { <Tag label="Modifié" tone=TagTone::Warning /> })}
                <span class="script-workbench__spacer"></span>
                {can_manage.then(|| view! {
                    <button type="submit" class="btn btn-primary" disabled=move || pending.get()>
                        {move || if pending.get() { "Enregistrement…" } else { "Enregistrer" }}
                    </button>
                })}
            </header>
            {read_only.then(|| view! {
                <p class="flow-workbench__readonly" role="status">
                    "Lecture seule : modifier un script demande le droit de gestion de la collection locale."
                </p>
            })}
            {move || (!pending.get())
                .then(|| opened_script_warning(opened_state.get(), on_reload))
                .flatten()}
            {move || feedback.get().map(|text| view! {
                <p class="flow-workbench__feedback" role="status">{text}</p>
            })}
            <div class="script-workbench__body">
                <section class="script-workbench__code">
                    <label class="form-label" for="script-code">
                        "Code (entrée stdin JSON → sortie stdout JSON) *"
                    </label>
                    <CodeEditor code=code extension=extension declared=declared read_only=read_only />
                    <p class="form-help">
                        "Le script reçoit workingFile, workDir, mediaInfo et config sur stdin."
                    </p>
                </section>
                <aside class="script-workbench__side">
                    <nav class="page-subnav" role="tablist" aria-label="Panneaux du script">
                        {SidePanel::ALL.into_iter().map(|entry| {
                            let is_active = move || panel.get() == entry;
                            view! {
                                <button type="button" role="tab" class="page-subnav__tab"
                                    class:is-active=is_active
                                    aria-selected=move || is_active().to_string()
                                    on:click=move |_| panel.set(entry)>
                                    {entry.label()}
                                </button>
                            }
                        }).collect_view()}
                    </nav>
                    <div class="script-workbench__panel"
                        hidden=move || panel.get() != SidePanel::Properties>
                        <div class="form-group">
                            <label class="form-label" for="script-name">"Nom du script *"</label>
                            <input id="script-name" class="form-input" required=true disabled=read_only
                                prop:value=move || name.get()
                                on:input=move |e| name.set(event_target_value(&e)) />
                        </div>
                        <div class="form-group">
                            <label class="form-label" for="script-interp">"Langage / Interpréteur"</label>
                            <select id="script-interp" class="form-input" disabled=read_only
                                prop:value=move || interp.get().executable()
                                on:change=move |e| change_interpreter(event_target_value(&e))>
                                {ScriptInterpreter::ALL.iter().map(|&variant| view! {
                                    <option value=variant.executable()>{variant.label()}</option>
                                }).collect_view()}
                            </select>
                        </div>
                        <div class="form-group">
                            <label class="form-label" for="script-filename">"Point d'entrée"</label>
                            <input id="script-filename" class="form-input text-mono" disabled=read_only
                                placeholder="run.sh"
                                prop:value=move || filename.get()
                                on:input=move |e| filename.set(event_target_value(&e)) />
                        </div>
                        <div class="form-group">
                            <label class="form-label" for="script-category">"Catégorie"</label>
                            <select id="script-category" class="form-input" disabled=read_only
                                prop:value=move || category_value(category.get())
                                on:change=move |e| category.set(parse_category(&event_target_value(&e)))>
                                {CATEGORIES.iter().map(|&(cat, label)| view! {
                                    <option value=category_value(cat)>{label}</option>
                                }).collect_view()}
                            </select>
                        </div>
                        <div class="form-group">
                            <label class="form-label" for="script-tags">
                                "Étiquettes (séparées par des virgules)"
                            </label>
                            <input id="script-tags" class="form-input" disabled=read_only
                                placeholder="mkv, sous-titres, audio"
                                prop:value=move || tags.get()
                                on:input=move |e| tags.set(event_target_value(&e)) />
                        </div>
                        <div class="form-group">
                            <label class="form-label" for="script-desc">"Description"</label>
                            <input id="script-desc" class="form-input" disabled=read_only
                                prop:value=move || desc.get()
                                on:input=move |e| desc.set(event_target_value(&e)) />
                        </div>
                    </div>
                    <div class="script-workbench__panel"
                        hidden=move || panel.get() != SidePanel::Variables>
                        <VariablesEditor rows=variable_rows read_only=read_only />
                    </div>
                    <div class="script-workbench__panel"
                        hidden=move || panel.get() != SidePanel::Dependencies>
                        <RequirementsEditor
                            rows=requirement_rows
                            unverified=dependencies_unverified
                            read_only=read_only
                        />
                    </div>
                </aside>
            </div>
        </form>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_title_is_the_script_name_or_a_new_script() {
        assert_eq!(page_title("  "), "Nouveau script");
        assert_eq!(page_title(" Nettoyer "), "Nettoyer");
    }

    #[test]
    fn every_side_panel_has_a_distinct_label() {
        let labels: Vec<&str> = SidePanel::ALL.iter().map(|panel| panel.label()).collect();
        assert_eq!(labels, ["Propriétés", "Variables", "Dépendances"]);
    }
}
