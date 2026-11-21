//! Inspecteur dédié au bloc unique « Script » (`scripts.run`) :
//! choix d'un script de la librairie, création rapide, code inline et routage conditionnel.

use super::state::Editor;
use crate::api;
use crate::libraries::starter_script_code;
use crate::ui::Toasts;
use forgearr_shared::flows::FlowNode;
use forgearr_shared::local::{
    LocalLibrarySnapshot, LocalScriptSummary, ScriptInterpreter, UpsertLocalScriptRequest,
};
use leptos::prelude::*;
use serde_json::{Map, Value, json};

pub const UNIFIED_SCRIPT_KIND: &str = "scripts.run";

fn update_node_config(
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

pub fn initial_script_routing(node: &FlowNode) -> &'static str {
    if node.config.get("routing").and_then(Value::as_str) == Some("conditional") {
        "conditional"
    } else {
        "single"
    }
}

#[component]
pub fn ScriptBlockInspector(
    editor: Editor,
    node_id: String,
    node: FlowNode,
    can_manage: bool,
) -> impl IntoView {
    let reload_local = RwSignal::new(0_u32);
    let snapshot = LocalResource::new(move || {
        reload_local.track();
        api::get::<LocalLibrarySnapshot>("/api/v1/local")
    });

    let mode = RwSignal::new(initial_script_mode(&node).to_owned());
    let script_id = RwSignal::new(config_str(&node, "scriptId"));
    let interp = RwSignal::new(
        ScriptInterpreter::from_executable(&config_str(&node, "interpreter")).unwrap_or_default(),
    );
    let initial_code = {
        let raw = config_str(&node, "code");
        if raw.trim().is_empty() {
            starter_script_code(interp.get_untracked()).to_owned()
        } else {
            raw
        }
    };
    let code = RwSignal::new(initial_code);
    let routing = RwSignal::new(initial_script_routing(&node).to_owned());

    let creating = RwSignal::new(false);
    let new_name = RwSignal::new(String::new());
    let new_interp = RwSignal::new(ScriptInterpreter::Sh);
    let new_code = RwSignal::new(starter_script_code(ScriptInterpreter::Sh).to_owned());
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
                creating.set(false);
                new_name.set(String::new());
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

    let nid_mode = node_id.clone();
    let nid_select = node_id.clone();
    let nid_interp = node_id.clone();
    let nid_code = node_id.clone();
    let nid_route = node_id;

    view! {
        <div class="form-group">
            <label for="script-block-mode" class="form-label">"Source du script"</label>
            <select
                id="script-block-mode"
                class="form-input"
                disabled=!can_manage
                prop:value=move || mode.get()
                on:change=move |ev| {
                    let next = event_target_value(&ev);
                    mode.set(next.clone());
                    let cur_interp = interp.get_untracked().executable().to_owned();
                    let cur_code = code.get_untracked();
                    update_node_config(editor, &nid_mode, move |map| {
                        map.insert("mode".into(), json!(next));
                        if next == "inline" {
                            map.entry("interpreter".to_owned()).or_insert_with(|| json!(cur_interp));
                            map.entry("code".to_owned()).or_insert_with(|| json!(cur_code));
                        }
                    });
                }
            >
                <option value="library">"Script de la librairie"</option>
                <option value="inline">"Code direct (inline)"</option>
            </select>
        </div>

        {move || {
            if mode.get() == "library" {
                let scripts = snapshot
                    .get()
                    .and_then(Result::ok)
                    .map(|s| s.scripts)
                    .unwrap_or_default();
                let nid_select = nid_select.clone();
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
                        <div class="form-group">
                            <button
                                type="button"
                                class="btn btn-secondary btn-sm"
                                on:click=move |_| creating.update(|v| *v = !*v)
                            >
                                {move || if creating.get() { "Fermer la création" } else { "+ Nouveau script" }}
                            </button>
                        </div>
                    })}
                    {move || (creating.get() && can_manage).then(|| view! {
                        <div class="form-group">
                            <label class="form-label" for="script-quick-name">"Nom du nouveau script *"</label>
                            <input
                                id="script-quick-name"
                                class="form-input"
                                prop:value=move || new_name.get()
                                on:input=move |e| new_name.set(event_target_value(&e))
                            />
                            <label class="form-label" for="script-quick-interp">"Langage"</label>
                            <select
                                id="script-quick-interp"
                                class="form-input"
                                prop:value=move || new_interp.get().executable()
                                on:change=move |e| {
                                    let next = ScriptInterpreter::from_executable(&event_target_value(&e))
                                        .unwrap_or_default();
                                    new_interp.set(next);
                                    new_code.set(starter_script_code(next).to_owned());
                                }
                            >
                                {ScriptInterpreter::ALL.iter().map(|&v| view! {
                                    <option value=v.executable()>{v.label()}</option>
                                }).collect_view()}
                            </select>
                            <label class="form-label" for="script-quick-code">"Code"</label>
                            <textarea
                                id="script-quick-code"
                                class="form-input text-mono"
                                rows="6"
                                prop:value=move || new_code.get()
                                on:input=move |e| new_code.set(event_target_value(&e))
                            />
                            <button
                                type="button"
                                class="btn btn-primary btn-sm"
                                disabled=move || save_script.pending().get()
                                on:click=move |_| {
                                    let name_val = new_name.get_untracked();
                                    if name_val.trim().is_empty() {
                                        Toasts::set_error(feedback, "Le nom du script est requis.");
                                        return;
                                    }
                                    save_script.dispatch(UpsertLocalScriptRequest {
                                        variables: Vec::new(),
                                        id: None,
                                        name: name_val,
                                        description: None,
                                        interpreter: new_interp.get_untracked(),
                                        code: new_code.get_untracked(),
                                        filename: None,
                                        category: None,
                                        tags: Vec::new(),
                                    });
                                }
                            >
                                "Enregistrer et utiliser"
                            </button>
                        </div>
                    })}
                }.into_any()
            } else {
                let nid_interp = nid_interp.clone();
                let nid_code = nid_code.clone();
                view! {
                    <div class="form-group">
                        <label for="script-inline-interp" class="form-label">"Interpréteur"</label>
                        <select
                            id="script-inline-interp"
                            class="form-input"
                            disabled=!can_manage
                            prop:value=move || interp.get().executable()
                            on:change=move |ev| {
                                let next = ScriptInterpreter::from_executable(&event_target_value(&ev))
                                    .unwrap_or_default();
                                interp.set(next);
                                update_node_config(editor, &nid_interp, move |map| {
                                    map.insert("mode".into(), json!("inline"));
                                    map.insert("interpreter".into(), json!(next.executable()));
                                });
                            }
                        >
                            {ScriptInterpreter::ALL.iter().map(|&v| view! {
                                <option value=v.executable()>{v.label()}</option>
                            }).collect_view()}
                        </select>
                    </div>
                    <div class="form-group">
                        <label for="script-inline-code" class="form-label">"Code (stdin JSON → stdout JSON)"</label>
                        <textarea
                            id="script-inline-code"
                            class="form-input text-mono"
                            rows="9"
                            disabled=!can_manage
                            prop:value=move || code.get()
                            on:input=move |ev| {
                                let text = event_target_value(&ev);
                                code.set(text.clone());
                                update_node_config(editor, &nid_code, move |map| {
                                    map.insert("mode".into(), json!("inline"));
                                    map.insert("code".into(), json!(text));
                                });
                            }
                        />
                        <p class="form-help">
                            "Renvoyez {\"status\":\"ok\",\"nextPort\":\"true\"} en mode conditionnel."
                        </p>
                    </div>
                    {can_manage.then(|| view! {
                        <div class="form-group">
                            <label class="form-label" for="script-promote-name">"Promouvoir en script de librairie"</label>
                            <input
                                id="script-promote-name"
                                class="form-input"
                                placeholder="Nom du script à enregistrer…"
                                prop:value=move || new_name.get()
                                on:input=move |e| new_name.set(event_target_value(&e))
                            />
                            <button
                                type="button"
                                class="btn btn-secondary btn-sm"
                                disabled=move || save_script.pending().get()
                                on:click=move |_| {
                                    let name_val = new_name.get_untracked();
                                    if name_val.trim().is_empty() {
                                        Toasts::set_error(feedback, "Saisissez un nom pour enregistrer le script.");
                                        return;
                                    }
                                    save_script.dispatch(UpsertLocalScriptRequest {
                                        variables: Vec::new(),
                                        id: None,
                                        name: name_val,
                                        description: None,
                                        interpreter: interp.get_untracked(),
                                        code: code.get_untracked(),
                                        filename: None,
                                        category: None,
                                        tags: Vec::new(),
                                    });
                                }
                            >
                                "Enregistrer dans la librairie"
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
                prop:value=move || routing.get()
                on:change=move |ev| {
                    let next = event_target_value(&ev);
                    routing.set(next.clone());
                    update_node_config(editor, &nid_route, move |map| {
                        map.insert("routing".into(), json!(next));
                    });
                }
            >
                <option value="single">"Simple (1 sortie)"</option>
                <option value="conditional">"Conditionnel (Vrai / Faux)"</option>
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
        assert_eq!(initial_script_routing(&node), "single");

        let cond_node = FlowNode {
            id: "n2".into(),
            kind: FlowBlockKind::parse(UNIFIED_SCRIPT_KIND),
            name: "Cond".into(),
            config: json!({"scriptId": "script.test", "routing": "conditional"}),
            extra: Map::new(),
        };
        assert_eq!(initial_script_mode(&cond_node), "library");
        assert_eq!(initial_script_routing(&cond_node), "conditional");
    }
}
