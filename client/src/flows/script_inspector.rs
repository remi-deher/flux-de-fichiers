//! Inspecteur dédié au bloc unique « Script » (`scripts.run`) :
//! choix d'un script de la librairie, création rapide, code inline et routage conditionnel.

use super::state::Editor;
use super::template_input::ScriptProtocolHelp;
use super::variables_form::ScriptVariablesPanel;
use crate::api;
use crate::libraries::script_editor_location;
use crate::route::{Route, use_router};
use crate::ui::Toasts;
use forgearr_shared::flows::{BranchPort, FlowNode, SCRIPT_ROUTING_KEY, ScriptRouting};
use forgearr_shared::local::{
    LocalLibrarySnapshot, LocalScriptSummary, ScriptInterpreter, UpsertLocalScriptRequest,
};
use leptos::prelude::*;
use serde_json::{Map, Value, json};

/// Flux d'où l'on ouvre l'éditeur de script : l'éditeur y revient à sa fermeture.
fn flow_origin(route: &Route) -> Option<String> {
    match route {
        Route::FlowEditor(id) => Some(id.clone()),
        _ => None,
    }
}

pub const UNIFIED_SCRIPT_KIND: &str = "scripts.run";

pub(super) fn update_node_config(
    editor: Editor,
    node_id: &str,
    apply: impl FnOnce(&mut Map<String, Value>) + 'static,
) {
    let target_id = node_id.to_owned();
    editor.edit(move |graph| {
        if let Some(node) = graph.node_mut(&target_id) {
            if !node.config.is_object() {
                node.config = Value::Object(Map::new());
            }
            if let Some(map) = node.config.as_object_mut() {
                apply(map);
            }
        }
    });
}

fn config_str(node: &FlowNode, key: &str) -> String {
    node.config
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

pub fn initial_script_mode(node: &FlowNode) -> &'static str {
    match node.config.get("mode").and_then(Value::as_str) {
        Some("inline") => "inline",
        Some("library") => "library",
        _ if !config_str(node, "code").trim().is_empty()
            && config_str(node, "scriptId").is_empty() =>
        {
            "inline"
        }
        _ => "library",
    }
}

pub fn initial_script_routing(node: &FlowNode) -> ScriptRouting {
    ScriptRouting::from_config(&node.config)
}

/// Libellé d'un mode de sortie dans le sélecteur.
fn routing_label(routing: ScriptRouting) -> &'static str {
    match routing {
        ScriptRouting::Single => "Simple (1 sortie)",
        ScriptRouting::Conditional => "Conditionnel (Vrai / Faux)",
    }
}

/// Aide affichée sous l'éditeur de code : réponse attendue en mode conditionnel.
fn conditional_reply_help() -> String {
    format!(
        "Renvoyez {{\"status\":\"ok\",\"nextPort\":\"{}\"}} en mode conditionnel (ok → {}, skip → {} par défaut).",
        BranchPort::True.as_str(),
        BranchPort::True.as_str(),
        BranchPort::False.as_str(),
    )
}

#[component]
pub fn ScriptBlockInspector(
    editor: Editor,
    node_id: String,
    node: FlowNode,
    can_manage: bool,
) -> impl IntoView {
    let router = use_router();
    let from = flow_origin(&router.current());
    let reload_local = RwSignal::new(0_u32);
    let snapshot = LocalResource::new(move || {
        reload_local.track();
        api::get::<LocalLibrarySnapshot>("/api/v1/local")
    });

    // Un bloc ancien peut encore porter du code direct : il reste lisible et s'enregistre en script.
    let mode = RwSignal::new(initial_script_mode(&node).to_owned());
    let script_id = RwSignal::new(config_str(&node, "scriptId"));
    let interpreter =
        ScriptInterpreter::from_executable(&config_str(&node, "interpreter")).unwrap_or_default();
    let inline_code = config_str(&node, "code");
    let routing = RwSignal::new(initial_script_routing(&node));
    let promote_name = RwSignal::new(String::new());
    let feedback = RwSignal::new(None::<String>);

    let save_script = Action::new_local(move |req: &UpsertLocalScriptRequest| {
        let req = req.clone();
        async move {
            api::post::<UpsertLocalScriptRequest, LocalScriptSummary>(
                "/api/v1/local/scripts",
                Some(&req),
            )
            .await
        }
    });

    Effect::new({
        let nid = node_id.clone();
        move |_| match save_script.value().get() {
            Some(Ok(saved)) => {
                promote_name.set(String::new());
                feedback.set(None);
                mode.set("library".to_owned());
                script_id.set(saved.id.clone());
                let saved_id = saved.id.clone();
                update_node_config(editor, &nid, move |map| {
                    map.insert("mode".into(), json!("library"));
                    map.insert("scriptId".into(), json!(saved_id));
                });
                reload_local.update(|c| *c = c.wrapping_add(1));
                Toasts::notify_success("Script enregistré et sélectionné.");
            }
            Some(Err(err)) => Toasts::set_error(feedback, err.message()),
            None => {}
        }
    });

    let nid_select = node_id.clone();
    let nid_vars = node_id.clone();
    let nid_route = node_id;
    let edit_from = from.clone();
    let new_from = from;

    view! {
        {move || {
            if mode.get() == "library" {
                let scripts = snapshot
                    .get()
                    .and_then(Result::ok)
                    .map(|s| s.scripts)
                    .unwrap_or_default();
                let nid_select = nid_select.clone();
                let nid_vars = nid_vars.clone();
                let edit_from = edit_from.clone();
                let new_from = new_from.clone();
                view! {
                    <div class="form-group">
                        <label for="script-block-ref" class="form-label">"Script sélectionné"</label>
                        <select
                            id="script-block-ref"
                            class="form-input"
                            disabled=!can_manage
                            prop:value=move || script_id.get()
                            on:change=move |ev| {
                                let chosen = event_target_value(&ev);
                                script_id.set(chosen.clone());
                                update_node_config(editor, &nid_select, move |map| {
                                    map.insert("mode".into(), json!("library"));
                                    map.insert("scriptId".into(), json!(chosen));
                                });
                            }
                        >
                            <option value="">"— Choisir un script —"</option>
                            {scripts.into_iter().map(|s| view! {
                                <option value=s.id.clone()>{format!("{} ({})", s.name, s.interpreter.executable())}</option>
                            }).collect_view()}
                        </select>
                    </div>
                    {can_manage.then(|| view! {
                        <div class="form-group cluster">
                            <button
                                type="button"
                                class="btn btn-secondary btn-sm"
                                disabled=move || script_id.get().is_empty()
                                on:click=move |_| router.go_to(script_editor_location(
                                    Route::ScriptEditor(script_id.get_untracked()),
                                    edit_from.as_deref(),
                                ))
                            >
                                "Modifier le script"
                            </button>
                            <button
                                type="button"
                                class="btn btn-secondary btn-sm"
                                on:click=move |_| router.go_to(script_editor_location(
                                    Route::NewScript,
                                    new_from.as_deref(),
                                ))
                            >
                                "+ Nouveau script"
                            </button>
                        </div>
                    })}
                    {move || (!script_id.get().is_empty()).then(|| view! {
                        <ScriptVariablesPanel editor=editor node_id=nid_vars.clone() can_manage=can_manage />
                    })}
                }.into_any()
            } else {
                let inline_code = inline_code.clone();
                let promote_code = inline_code.clone();
                view! {
                    <div class="form-group">
                        <span class="form-label">"Code direct (ancien bloc)"</span>
                        <p class="form-help">
                            {format!("Interpréteur : {}. ", interpreter.label())}
                            "Enregistrez-le en script pour le modifier dans l'éditeur."
                        </p>
                        <pre class="text-mono text-sm">{inline_code}</pre>
                        <p class="form-help">{conditional_reply_help()}</p>
                        <ScriptProtocolHelp />
                    </div>
                    {can_manage.then(|| view! {
                        <div class="form-group">
                            <label class="form-label" for="script-promote-name">"Nom du script à enregistrer"</label>
                            <input
                                id="script-promote-name"
                                class="form-input"
                                placeholder="Nom du script à enregistrer…"
                                prop:value=move || promote_name.get()
                                on:input=move |e| promote_name.set(event_target_value(&e))
                            />
                            <button
                                type="button"
                                class="btn btn-secondary btn-sm"
                                disabled=move || save_script.pending().get()
                                on:click=move |_| {
                                    let name_val = promote_name.get_untracked();
                                    if name_val.trim().is_empty() {
                                        Toasts::set_error(feedback, "Saisissez un nom pour enregistrer le script.");
                                        return;
                                    }
                                    save_script.dispatch(UpsertLocalScriptRequest {
                                        requires: Vec::new(),
                                        dependencies_unverified: false,
                                        variables: Vec::new(),
                                        id: None,
                                        name: name_val,
                                        description: None,
                                        interpreter,
                                        code: promote_code.clone(),
                                        filename: None,
                                        category: None,
                                        tags: Vec::new(),
                                    });
                                }
                            >
                                "Enregistrer comme script"
                            </button>
                        </div>
                    })}
                }.into_any()
            }
        }}

        <div class="form-group">
            <label for="script-block-routing" class="form-label">"Sorties / Routage"</label>
            <select
                id="script-block-routing"
                class="form-input"
                disabled=!can_manage
                prop:value=move || routing.get().as_str()
                on:change=move |ev| {
                    let next = ScriptRouting::parse(&event_target_value(&ev));
                    routing.set(next);
                    update_node_config(editor, &nid_route, move |map| {
                        map.insert(SCRIPT_ROUTING_KEY.into(), json!(next.as_str()));
                    });
                }
            >
                {ScriptRouting::ALL.into_iter().map(|mode| view! {
                    <option value=mode.as_str()>{routing_label(mode)}</option>
                }).collect_view()}
            </select>
        </div>

        {move || feedback.get().map(|err| view! {
            <p class="form-error text-sm" role="status">{err}</p>
        })}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use forgearr_shared::flows::FlowBlockKind;

    #[test]
    fn script_block_inspector_defaults_follow_node_config() {
        let node = FlowNode {
            id: "n1".into(),
            kind: FlowBlockKind::parse(UNIFIED_SCRIPT_KIND),
            name: "Script".into(),
            config: json!({"code": "echo ok"}),
            extra: Map::new(),
        };
        assert_eq!(initial_script_mode(&node), "inline");
        assert_eq!(initial_script_routing(&node), ScriptRouting::Single);

        let cond_node = FlowNode {
            id: "n2".into(),
            kind: FlowBlockKind::parse(UNIFIED_SCRIPT_KIND),
            name: "Cond".into(),
            config: json!({"scriptId": "script.test", "routing": "conditional"}),
            extra: Map::new(),
        };
        assert_eq!(initial_script_mode(&cond_node), "library");
        assert_eq!(
            initial_script_routing(&cond_node),
            ScriptRouting::Conditional
        );
        assert!(conditional_reply_help().contains("\"nextPort\":\"true\""));
    }

    #[test]
    fn the_script_editor_returns_to_the_flow_that_opened_it() {
        let route = Route::FlowEditor("flux.a".into());
        assert_eq!(flow_origin(&route).as_deref(), Some("flux.a"));
        assert_eq!(flow_origin(&Route::NewFlow), None);
        assert_eq!(flow_origin(&Route::Flows), None);
    }
}
