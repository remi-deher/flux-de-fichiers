//! Onglet Scripts de la Librairie : écrire, éditer et supprimer des scripts locaux (`stdio` JSON).

use super::{LOCAL_ITEMS_PATH, LOCAL_SCRIPTS_PATH, bump};
use crate::api;
use crate::route::encode_segment;
use crate::ui::search::{matches, query};
use crate::ui::topbar_action::{ActionSpec, provide_topbar_action};
use crate::ui::{
    Drawer, EmptyState, IconName, MetricTone, ResourceCard, StatItem, StatStrip, Tag, TagTone,
    Toasts,
};
use flux_shared::local::{LocalLibrarySnapshot, LocalScriptSummary, UpsertLocalScriptRequest};
use leptos::prelude::*;

const INTERPRETERS: &[(&str, &str)] = &[
    ("sh", "Shell (sh)"),
    ("bash", "Bash"),
    ("python3", "Python 3"),
    ("node", "Node.js"),
    ("deno", "Deno"),
    ("bun", "Bun"),
    ("pwsh", "PowerShell"),
];

pub fn starter_script_code(interpreter: &str) -> &'static str {
    match interpreter {
        "python3" => {
            "import json, sys\n\npayload = json.load(sys.stdin)\nprint(json.dumps({\"status\": \"ok\", \"message\": \"Script Python exécuté\"}))\n"
        }
        "node" | "deno" | "bun" => {
            "const fs = require('fs');\nconst input = JSON.parse(fs.readFileSync(0, 'utf8'));\nconsole.log(JSON.stringify({ status: 'ok', message: 'Script JS exécuté' }));\n"
        }
        _ => "cat >/dev/null\nprintf '{\"status\":\"ok\",\"message\":\"Script exécuté\"}'\n",
    }
}

#[derive(Clone, Default, PartialEq, Eq)]
struct ScriptDraft {
    id: Option<String>,
    name: String,
    description: String,
    interpreter: String,
    filename: String,
    code: String,
}

impl ScriptDraft {
    fn blank() -> Self {
        Self {
            id: None,
            name: String::new(),
            description: String::new(),
            interpreter: "sh".to_owned(),
            filename: String::new(),
            code: starter_script_code("sh").to_owned(),
        }
    }

    fn from_summary(s: &LocalScriptSummary) -> Self {
        Self {
            id: Some(s.id.clone()),
            name: s.name.clone(),
            description: s.description.clone().unwrap_or_default(),
            interpreter: s.interpreter.clone(),
            filename: s.entrypoint.clone(),
            code: s.code.clone(),
        }
    }
}

#[component]
pub fn ScriptsSection(
    snapshot: LocalResource<Result<LocalLibrarySnapshot, api::ApiError>>,
    reload: RwSignal<u32>,
    can_manage: bool,
) -> impl IntoView {
    let editing = RwSignal::new(None::<ScriptDraft>);
    let drawer_open = RwSignal::new(false);
    let feedback = RwSignal::new(None::<String>);

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
            editing.set(None);
            Toasts::notify_success("Script enregistré dans la librairie locale.");
            bump(reload);
        }
        Some(Err(err)) => Toasts::set_error(feedback, err.message()),
        None => {}
    });

    let remove = Action::new_local(move |id: &String| {
        let path = format!("{LOCAL_ITEMS_PATH}/{}", encode_segment(id));
        async move { api::delete(&path).await }
    });
    Effect::new(move |_| match remove.value().get() {
        Some(Ok(())) => {
            Toasts::notify_success("Script supprimé.");
            bump(reload);
        }
        Some(Err(err)) => Toasts::notify_error(err.message()),
        None => {}
    });

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
                        .filter(|s| matches(&[&s.name, &s.id, &s.interpreter], &text))
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
                                let edit_draft = ScriptDraft::from_summary(&script);
                                let del_id = script.id.clone();
                                let badge = view! {
                                    <Tag label=script.interpreter.clone() tone=TagTone::Primary />
                                }.into_any();
                                let actions = can_manage.then(|| {
                                    let edit_draft = edit_draft.clone();
                                    let del_id = del_id.clone();
                                    view! {
                                        <button type="button" class="btn btn-secondary btn-sm"
                                            on:click=move |_| {
                                                feedback.set(None);
                                                editing.set(Some(edit_draft.clone()));
                                                drawer_open.set(true);
                                            }>"Éditer"</button>
                                        <button type="button" class="btn btn-sm btn-danger-quiet"
                                            disabled=move || remove.pending().get()
                                            on:click=move |_| { remove.dispatch(del_id.clone()); }>"Supprimer"</button>
                                    }.into_any()
                                });
                                view! {
                                    <ResourceCard
                                        title=script.name
                                        subtitle=script.description
                                        icon=IconName::Command
                                        tone=MetricTone::Success
                                        badge=Some(badge)
                                        footer_meta=format!("{} · {}", script.id, script.entrypoint)
                                        footer_actions=actions
                                    >
                                        <div class="cluster">
                                            <Tag label=format!("block.{}", script.id) tone=TagTone::Mono />
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
            {move || editing.get().map(|draft| {
                let id = draft.id;
                let name = RwSignal::new(draft.name);
                let desc = RwSignal::new(draft.description);
                let interp = RwSignal::new(draft.interpreter);
                let filename = RwSignal::new(draft.filename);
                let code = RwSignal::new(draft.code);
                let is_new = id.is_none();
                view! {
                    <form class="inspection-panel" on:submit=move |ev| {
                        ev.prevent_default();
                        save.dispatch(UpsertLocalScriptRequest {
                            id: id.clone(),
                            name: name.get_untracked(),
                            description: Some(desc.get_untracked()),
                            interpreter: interp.get_untracked(),
                            code: code.get_untracked(),
                            filename: Some(filename.get_untracked()),
                            category: None,
                            tags: Vec::new(),
                        });
                    }>
                        <div class="form-group">
                            <label class="form-label" for="script-name">"Nom du script *"</label>
                            <input id="script-name" class="form-input" required=true
                                prop:value=move || name.get()
                                on:input=move |e| name.set(event_target_value(&e)) />
                        </div>
                        <div class="form-group">
                            <label class="form-label" for="script-interp">"Langage / Interpréteur"</label>
                            <select id="script-interp" class="form-input"
                                prop:value=move || interp.get()
                                on:change=move |e| {
                                    let next = event_target_value(&e);
                                    if is_new && code.get_untracked().trim().is_empty() {
                                        code.set(starter_script_code(&next).to_owned());
                                    }
                                    interp.set(next);
                                }>
                                {INTERPRETERS.iter().map(|(val, label)| view! {
                                    <option value=*val>{*label}</option>
                                }).collect_view()}
                            </select>
                        </div>
                        <div class="form-group">
                            <label class="form-label" for="script-desc">"Description"</label>
                            <input id="script-desc" class="form-input"
                                prop:value=move || desc.get()
                                on:input=move |e| desc.set(event_target_value(&e)) />
                        </div>
                        <div class="form-group">
                            <label class="form-label" for="script-code">"Code (entrée stdin JSON → sortie stdout JSON) *"</label>
                            <textarea id="script-code" class="form-input text-mono" rows="12" required=true
                                prop:value=move || code.get()
                                on:input=move |e| code.set(event_target_value(&e)) />
                            <p class="form-help">"Le script reçoit workingFile, workDir, mediaInfo et config sur stdin."</p>
                        </div>
                        <div class="inspection-drawer__actions">
                            <button type="submit" class="btn btn-primary" disabled=move || save.pending().get()>
                                {move || if save.pending().get() { "Enregistrement…" } else { "Enregistrer" }}
                            </button>
                        </div>
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
        assert!(starter_script_code("sh").contains("status"));
        assert!(starter_script_code("python3").contains("sys.stdin"));
        assert!(starter_script_code("node").contains("readFileSync"));
    }
}
