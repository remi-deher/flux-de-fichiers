//! Onglet Scripts de la Librairie : écrire, éditer et supprimer des scripts locaux (`stdio` JSON).

use super::requirements_editor::{RequirementsEditor, requirement_rows_from, rows_to_requirements};
use super::variables_editor::{VariablesEditor, rows_from, rows_to_variables};
use super::{LOCAL_SCRIPTS_PATH, LOCAL_TOPICS, LocalDeleteButton, use_local_delete};
use crate::api;
use crate::live::{ChangedElsewhere, Loaded, use_live};
use crate::ui::search::{matches, query};
use crate::ui::topbar_action::{ActionSpec, provide_topbar_action};
use crate::ui::{
    Drawer, EmptyState, IconName, MetricTone, ResourceCard, StatItem, StatStrip, Tag, TagTone,
    Toasts,
};
use forgearr_shared::exposed_variables::ScriptRequirement;
use forgearr_shared::local::{
    LocalLibrarySnapshot, LocalScriptSummary, ScriptInterpreter, UpsertLocalScriptRequest,
};
use forgearr_shared::plugins::vocab::PluginCategory;
use forgearr_shared::script_variables::ScriptVariable;
use leptos::prelude::*;

const CATEGORIES: &[(Option<PluginCategory>, &str)] = &[
    (None, "Standard (aucun badge)"),
    (Some(PluginCategory::Media), "Média"),
    (Some(PluginCategory::Data), "Données"),
    (Some(PluginCategory::Io), "Entrées / Sorties"),
];

pub fn starter_script_code(interpreter: ScriptInterpreter) -> &'static str {
    match interpreter {
        ScriptInterpreter::Python3 => {
            "import json, sys\n\npayload = json.load(sys.stdin)\nprint(json.dumps({\"status\": \"ok\", \"message\": \"Script Python exécuté\"}))\n"
        }
        ScriptInterpreter::Node | ScriptInterpreter::Bun => {
            "const fs = require('fs');\nconst input = JSON.parse(fs.readFileSync(0, 'utf8'));\nconsole.log(JSON.stringify({ status: 'ok', message: 'Script JS exécuté' }));\n"
        }
        ScriptInterpreter::Deno => {
            "const text = await new Response(Deno.stdin.readable).text();\nconst input = text.trim() ? JSON.parse(text) : {};\nconsole.log(JSON.stringify({ status: 'ok', message: 'Script Deno exécuté' }));\n"
        }
        ScriptInterpreter::Pwsh => {
            "$payload = [Console]::In.ReadToEnd() | ConvertFrom-Json\n@{ status = 'ok'; message = 'Script PowerShell exécuté' } | ConvertTo-Json -Compress\n"
        }
        ScriptInterpreter::Sh | ScriptInterpreter::Bash => {
            "cat >/dev/null\nprintf '{\"status\":\"ok\",\"message\":\"Script exécuté\"}\\n'\n"
        }
    }
}

pub fn is_starter_code(code: &str) -> bool {
    let trimmed = code.trim();
    trimmed.is_empty()
        || ScriptInterpreter::ALL
            .iter()
            .any(|&interp| starter_script_code(interp).trim() == trimmed)
}

pub fn parse_tags(raw: &str) -> Vec<String> {
    let mut out = Vec::new();
    for tag in raw.split(',').map(str::trim).filter(|t| !t.is_empty()) {
        if !out.iter().any(|existing: &String| existing == tag) {
            out.push(tag.to_owned());
        }
    }
    out
}

fn category_value(cat: Option<PluginCategory>) -> &'static str {
    match cat {
        Some(c) if c.shows_badge() => c.as_str(),
        _ => "",
    }
}

fn parse_category(raw: &str) -> Option<PluginCategory> {
    match raw.trim() {
        "media" => Some(PluginCategory::Media),
        "data" => Some(PluginCategory::Data),
        "io" => Some(PluginCategory::Io),
        _ => None,
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct ScriptDraft {
    id: Option<String>,
    name: String,
    description: String,
    interpreter: ScriptInterpreter,
    filename: String,
    code: String,
    category: Option<PluginCategory>,
    tags: String,
    variables: Vec<ScriptVariable>,
    requires: Vec<ScriptRequirement>,
    dependencies_unverified: bool,
}

impl ScriptDraft {
    fn blank() -> Self {
        let interp = ScriptInterpreter::Sh;
        Self {
            id: None,
            name: String::new(),
            description: String::new(),
            interpreter: interp,
            filename: format!("run.{}", interp.default_extension()),
            code: starter_script_code(interp).to_owned(),
            category: None,
            tags: String::new(),
            variables: Vec::new(),
            requires: Vec::new(),
            dependencies_unverified: false,
        }
    }

    fn from_summary(s: &LocalScriptSummary) -> Self {
        Self {
            id: Some(s.id.clone()),
            name: s.name.clone(),
            description: s.description.clone().unwrap_or_default(),
            interpreter: s.interpreter,
            filename: s.entrypoint.clone(),
            code: s.code.clone(),
            category: s.category.filter(|c| c.shows_badge()),
            tags: s.tags.join(", "),
            variables: s.variables.clone(),
            requires: s.requires.clone(),
            dependencies_unverified: s.dependencies_unverified,
        }
    }
}

/// Ce que le script ouvert est devenu dans la librairie relue.
#[derive(Clone, Debug, PartialEq, Eq)]
enum OpenedScript {
    Unchanged,
    Changed(Box<ScriptDraft>),
    Removed,
}

impl OpenedScript {
    /// `opened` est l'état à l'ouverture, sans la saisie ; un nouveau script n'a rien à comparer.
    fn of(opened: &ScriptDraft, scripts: &[LocalScriptSummary]) -> Self {
        let Some(id) = opened.id.as_deref() else {
            return Self::Unchanged;
        };
        match scripts.iter().find(|script| script.id == id) {
            None => Self::Removed,
            Some(script) => {
                let current = ScriptDraft::from_summary(script);
                if current == *opened {
                    Self::Unchanged
                } else {
                    Self::Changed(Box::new(current))
                }
            }
        }
    }
}

/// Avertissement affiché au-dessus du script ouvert ; `on_reload` remplace la saisie.
fn opened_script_warning(state: OpenedScript, on_reload: Callback<ScriptDraft>) -> Option<AnyView> {
    match state {
        OpenedScript::Unchanged => None,
        OpenedScript::Changed(current) => {
            let reload = Callback::new(move |()| on_reload.run((*current).clone()));
            Some(
                view! {
                    <ChangedElsewhere
                        text="Ce script a changé depuis son ouverture. Recharger remplace votre saisie ; enregistrer remplace ces changements."
                        on_reload=Some(reload)
                    />
                }
                .into_any(),
            )
        }
        OpenedScript::Removed => Some(
            view! { <ChangedElsewhere text="Ce script a été supprimé de la librairie." /> }
                .into_any(),
        ),
    }
}

#[component]
pub fn ScriptsSection(snapshot: Loaded<LocalLibrarySnapshot>, can_manage: bool) -> impl IntoView {
    let live = use_live();
    let editing = RwSignal::new(None::<ScriptDraft>);
    let drawer_open = RwSignal::new(false);
    let feedback = RwSignal::new(None::<String>);

    Effect::new(move |_| {
        if !drawer_open.get() {
            editing.set(None);
            feedback.set(None);
        }
    });

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
        Some(Ok(_)) => {
            drawer_open.set(false);
            Toasts::notify_success("Script enregistré dans la librairie locale.");
            live.touch(LOCAL_TOPICS);
        }
        Some(Err(err)) => Toasts::set_error(feedback, err.message()),
        None => {}
    });

    let remove = use_local_delete("Script supprimé.");

    if can_manage {
        provide_topbar_action(ActionSpec {
            label: "Écrire un script",
            on_click: Callback::new(move |()| {
                feedback.set(None);
                editing.set(Some(ScriptDraft::blank()));
                drawer_open.set(true);
            }),
        });
    }

    let stats = Signal::derive(move || {
        let count = snapshot
            .get()
            .and_then(Result::ok)
            .map_or(0, |s| s.scripts.len());
        vec![StatItem::new(
            "Scripts locaux",
            count.to_string(),
            if count > 0 {
                MetricTone::Success
            } else {
                MetricTone::Neutral
            },
        )]
    });

    let opened_state = Memo::new(move |_| {
        let (editing, snapshot) = (editing.read(), snapshot.read());
        match (editing.as_ref(), snapshot.as_ref()) {
            (Some(opened), Some(Ok(data))) => OpenedScript::of(opened, &data.scripts),
            _ => OpenedScript::Unchanged,
        }
    });
    let on_reload = Callback::new(move |current: ScriptDraft| editing.set(Some(current)));

    let drawer_title = Signal::derive(move || match editing.get() {
        Some(d) if d.id.is_some() => d.name,
        _ => "Nouveau script".to_owned(),
    });

    view! {
        <StatStrip aria_label="Synthèse des scripts locaux" items=stats />
        {move || {
            let text = query();
            match snapshot.get() {
                None => view! {
                    <div class="empty-state-panel"><p class="text-dim">"Chargement des scripts…"</p></div>
                }.into_any(),
                Some(Err(err)) => view! {
                    <div class="empty-state-panel"><p class="form-error">{err.message()}</p></div>
                }.into_any(),
                Some(Ok(data)) => {
                    let shown: Vec<LocalScriptSummary> = data
                        .scripts
                        .into_iter()
                        .filter(|s| matches(&[&s.name, &s.id, s.interpreter.executable()], &text))
                        .collect();
                    if shown.is_empty() {
                        return view! {
                            <div class="empty-state-panel">
                                <EmptyState
                                    title="Aucun script local"
                                    message="Écrivez un script Bash, Python ou Node.js directement ici : il apparaîtra aussitôt dans la palette de vos flux."
                                    icon_name=IconName::Command
                                />
                            </div>
                        }.into_any();
                    }
                    view! {
                        <div class="resource-grid resource-grid--list">
                            {shown.into_iter().map(|script| {
                                let card_id = script.id.clone();
                                let edit_draft = ScriptDraft::from_summary(&script);
                                let open_edit = Callback::new({
                                    let edit_draft = edit_draft.clone();
                                    move |()| {
                                        feedback.set(None);
                                        editing.set(Some(edit_draft.clone()));
                                        drawer_open.set(true);
                                    }
                                });
                                let active = Signal::derive(move || {
                                    editing.get().is_some_and(|d| d.id.as_deref() == Some(card_id.as_str()))
                                });
                                let badge = view! {
                                    <Tag label=script.interpreter.executable() tone=TagTone::Primary />
                                }.into_any();
                                let actions = can_manage.then(|| {
                                    let del_id = script.id.clone();
                                    view! {
                                        <button type="button" class="btn btn-secondary btn-sm"
                                            on:click=move |ev| {
                                                ev.stop_propagation();
                                                open_edit.run(());
                                            }>"Éditer"</button>
                                        <LocalDeleteButton id=del_id remove=remove />
                                    }.into_any()
                                });
                                view! {
                                    <ResourceCard
                                        focus_key=script.id.clone()
                                        title=script.name
                                        subtitle=script.description
                                        icon=IconName::Command
                                        tone=MetricTone::Success
                                        badge=Some(badge)
                                        active=Some(active)
                                        on_click=Some(open_edit)
                                        footer_meta=format!("{} · {}", script.id, script.entrypoint)
                                        footer_actions=actions
                                    >
                                        <div class="cluster">
                                            <Tag label=format!("block.{}", script.id) tone=TagTone::Mono />
                                            {script.category.filter(|c| c.shows_badge()).map(|c| view! {
                                                <Tag label=c.as_str() tone=TagTone::Primary />
                                            })}
                                            {script.tags.into_iter().map(|t| view! {
                                                <Tag label=t tone=TagTone::Neutral />
                                            }).collect_view()}
                                        </div>
                                    </ResourceCard>
                                }
                            }).collect_view()}
                        </div>
                    }.into_any()
                }
            }
        }}
        <Drawer open=drawer_open eyebrow="Script local" title=drawer_title>
            {move || (!save.pending().get())
                .then(|| opened_script_warning(opened_state.get(), on_reload))
                .flatten()}
            {move || editing.get().map(|draft| {
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
                let read_only = !can_manage;
                view! {
                    <form class="inspection-panel" on:submit=move |ev| {
                        ev.prevent_default();
                        if !can_manage {
                            return;
                        }
                        let raw_fn = filename.get_untracked();
                        let trimmed_fn = raw_fn.trim();
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
                        save.dispatch(UpsertLocalScriptRequest {
                            requires,
                            dependencies_unverified: dependencies_unverified.get_untracked(),
                            variables,
                            id: id.clone(),
                            name: name.get_untracked(),
                            description: Some(desc.get_untracked()),
                            interpreter: interp.get_untracked(),
                            code: code.get_untracked(),
                            filename: (!trimmed_fn.is_empty()).then(|| trimmed_fn.to_owned()),
                            category: category.get_untracked(),
                            tags: parse_tags(&tags.get_untracked()),
                        });
                    }>
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
                                on:change=move |e| {
                                    let prev = interp.get_untracked();
                                    let next = ScriptInterpreter::from_executable(&event_target_value(&e))
                                        .unwrap_or(prev);
                                    if is_starter_code(&code.get_untracked()) {
                                        code.set(starter_script_code(next).to_owned());
                                    }
                                    let current_fn = filename.get_untracked();
                                    let default_prev = format!("run.{}", prev.default_extension());
                                    if current_fn.trim().is_empty() || current_fn.trim() == default_prev {
                                        filename.set(format!("run.{}", next.default_extension()));
                                    }
                                    interp.set(next);
                                }>
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
                            <label class="form-label" for="script-tags">"Étiquettes (séparées par des virgules)"</label>
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
                        <div class="form-group">
                            <label class="form-label" for="script-code">"Code (entrée stdin JSON → sortie stdout JSON) *"</label>
                            <textarea id="script-code" class="form-input text-mono" rows="12" required=true disabled=read_only
                                prop:value=move || code.get()
                                on:input=move |e| code.set(event_target_value(&e)) />
                            <p class="form-help">"Le script reçoit workingFile, workDir, mediaInfo et config sur stdin."</p>
                        </div>
                        <VariablesEditor rows=variable_rows read_only=read_only />
                        <RequirementsEditor
                            rows=requirement_rows
                            unverified=dependencies_unverified
                            read_only=read_only
                        />
                        {can_manage.then(|| view! {
                            <div class="inspection-drawer__actions">
                                <button type="submit" class="btn btn-primary" disabled=move || save.pending().get()>
                                    {move || if save.pending().get() { "Enregistrement…" } else { "Enregistrer" }}
                                </button>
                                <button type="button" class="btn btn-secondary" on:click=move |_| drawer_open.set(false)>
                                    "Annuler"
                                </button>
                            </div>
                        })}
                        <p class="form-error text-sm" role="status">{move || feedback.get()}</p>
                    </form>
                }
            })}
        </Drawer>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starter_script_code_provides_valid_templates_per_runtime() {
        for &interp in ScriptInterpreter::ALL {
            let starter = starter_script_code(interp);
            assert!(starter.contains("status"));
            assert!(is_starter_code(starter));
        }
        assert!(starter_script_code(ScriptInterpreter::Python3).contains("sys.stdin"));
        assert!(starter_script_code(ScriptInterpreter::Node).contains("readFileSync"));
        assert!(starter_script_code(ScriptInterpreter::Deno).contains("Deno.stdin"));
        assert!(starter_script_code(ScriptInterpreter::Pwsh).contains("ConvertFrom-Json"));
        assert!(!is_starter_code("echo custom script"));
    }

    #[test]
    fn an_opened_script_notices_changes_and_removal() -> Result<(), serde_json::Error> {
        let script: LocalScriptSummary = serde_json::from_value(serde_json::json!({
            "id": "script.x", "slug": "x", "name": "X", "interpreter": ScriptInterpreter::Sh,
            "entrypoint": "run.sh", "code": "true"
        }))?;
        let opened = ScriptDraft::from_summary(&script);
        let edited = LocalScriptSummary {
            code: "false".into(),
            ..script.clone()
        };
        assert_eq!(
            OpenedScript::of(&opened, std::slice::from_ref(&script)),
            OpenedScript::Unchanged
        );
        assert_eq!(
            OpenedScript::of(&opened, std::slice::from_ref(&edited)),
            OpenedScript::Changed(Box::new(ScriptDraft::from_summary(&edited)))
        );
        assert_eq!(OpenedScript::of(&opened, &[]), OpenedScript::Removed);
        assert_eq!(
            OpenedScript::of(&ScriptDraft::blank(), &[]),
            OpenedScript::Unchanged
        );
        Ok(())
    }

    #[test]
    fn parse_tags_trims_and_deduplicates() {
        assert_eq!(
            parse_tags(" mkv, audio , , mkv , hdr "),
            vec!["mkv", "audio", "hdr"]
        );
        assert!(parse_tags("   ").is_empty());
    }
}
