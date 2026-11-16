//! Cartes et tiroir d'édition / inspection des modèles de blocs réutilisables.

use super::scripts::parse_tags;
use super::{LOCAL_BLOCK_TEMPLATES_PATH, LocalDeleteButton, bump};
use crate::api;
use crate::ui::{Drawer, IconName, MetricTone, ResourceCard, Tag, TagTone, Toasts};
use flux_shared::flows::{FlowGraph, FlowRecord};
use flux_shared::local::{CreateBlockTemplateRequest, LocalBlockTemplate};
use leptos::prelude::*;
use serde_json::Value;
use std::collections::HashSet;

const DEFAULT_GRAPH_JSON: &str = "{\n  \"nodes\": [],\n  \"connections\": []\n}";

pub fn format_definition_pretty(value: &Value) -> String {
    serde_json::to_string_pretty(value).unwrap_or_else(|_| DEFAULT_GRAPH_JSON.to_owned())
}

pub fn validate_block_template_json(raw: &str) -> Result<Value, &'static str> {
    let value: Value = serde_json::from_str(raw).map_err(|_| "Définition JSON invalide.")?;
    let graph: FlowGraph = serde_json::from_value(value.clone())
        .map_err(|_| "Structure de graphe ({ nodes, connections }) invalide.")?;
    let node_ids: HashSet<&str> = graph.nodes.iter().map(|n| n.id.as_str()).collect();
    if graph
        .connections
        .iter()
        .any(|c| !node_ids.contains(c.from.as_str()) || !node_ids.contains(c.to.as_str()))
    {
        return Err("Une connexion référence un nœud absent du modèle.");
    }
    Ok(value)
}

#[derive(Clone, PartialEq, Eq)]
pub struct BlockTemplateDraft {
    pub id: Option<String>,
    pub name: String,
    pub description: String,
    pub json: String,
    pub tags: String,
    pub writable: bool,
}

impl BlockTemplateDraft {
    pub fn blank() -> Self {
        Self {
            id: None,
            name: String::new(),
            description: String::new(),
            json: DEFAULT_GRAPH_JSON.to_owned(),
            tags: String::new(),
            writable: true,
        }
    }

    pub fn from_template(tpl: &LocalBlockTemplate, can_manage: bool) -> Self {
        Self {
            id: Some(tpl.id.clone()),
            name: tpl.name.clone(),
            description: tpl.description.clone().unwrap_or_default(),
            json: format_definition_pretty(&tpl.definition),
            tags: tpl.tags.join(", "),
            writable: can_manage && tpl.is_local,
        }
    }
}

#[component]
pub fn BlockTemplateCard(
    tpl: LocalBlockTemplate,
    can_manage: bool,
    draft_sig: RwSignal<Option<BlockTemplateDraft>>,
    open_sig: RwSignal<bool>,
    feedback: RwSignal<Option<String>>,
    remove: Action<String, Result<(), api::ApiError>>,
) -> impl IntoView {
    let card_id = tpl.id.clone();
    let edit_draft = BlockTemplateDraft::from_template(&tpl, can_manage);
    let nodes_count = tpl
        .definition
        .get("nodes")
        .and_then(Value::as_array)
        .map_or(0, Vec::len);
    let badge = view! {
        <Tag
            label=if tpl.is_local { "Modèle local" } else { "Modèle catalogue" }
            tone=if tpl.is_local { TagTone::Success } else { TagTone::Primary }
        />
    }
    .into_any();
    let open_cb = Callback::new(move |()| {
        feedback.set(None);
        draft_sig.set(Some(edit_draft.clone()));
        open_sig.set(true);
    });
    let active = Signal::derive(move || {
        draft_sig
            .get()
            .is_some_and(|d| d.id.as_deref() == Some(card_id.as_str()))
    });
    let actions = (can_manage && tpl.is_local).then(|| {
        let del_id = tpl.id.clone();
        view! {
            <button
                type="button"
                class="btn btn-secondary btn-sm"
                on:click=move |ev| {
                    ev.stop_propagation();
                    open_cb.run(());
                }
            >
                "Éditer"
            </button>
            <LocalDeleteButton id=del_id remove=remove />
        }
        .into_any()
    });

    view! {
        <ResourceCard
            title=tpl.name
            subtitle=tpl.description
            icon=IconName::Layers
            tone=if tpl.is_local { MetricTone::Success } else { MetricTone::Neutral }
            badge=Some(badge)
            active=Some(active)
            on_click=Some(open_cb)
            footer_meta=format!("{nodes_count} bloc(s) · {}", tpl.id)
            footer_actions=actions
        >
            <div class="cluster">
                {tpl.tags.into_iter().map(|t| view! {
                    <Tag label=t tone=TagTone::Neutral />
                }).collect_view()}
            </div>
        </ResourceCard>
    }
}

#[component]
pub fn BlockTemplateDrawer(
    open: RwSignal<bool>,
    draft_sig: RwSignal<Option<BlockTemplateDraft>>,
    flows: LocalResource<Result<Vec<FlowRecord>, api::ApiError>>,
    reload: RwSignal<u32>,
    feedback: RwSignal<Option<String>>,
) -> impl IntoView {
    Effect::new(move |_| {
        if !open.get() {
            draft_sig.set(None);
            feedback.set(None);
        }
    });

    let save_template = Action::new_local(move |req: &CreateBlockTemplateRequest| {
        let req = req.clone();
        async move {
            api::post::<CreateBlockTemplateRequest, LocalBlockTemplate>(
                LOCAL_BLOCK_TEMPLATES_PATH,
                Some(&req),
            )
            .await
        }
    });
    Effect::new(move |_| match save_template.value().get() {
        Some(Ok(_)) => {
            open.set(false);
            Toasts::notify_success("Modèle de blocs enregistré.");
            bump(reload);
        }
        Some(Err(err)) => Toasts::set_error(feedback, err.message()),
        None => {}
    });

    let title = Signal::derive(move || match draft_sig.get() {
        Some(d) if d.id.is_some() => d.name,
        _ => "Nouveau modèle de blocs".to_owned(),
    });

    view! {
        <Drawer open=open eyebrow="Modèle de blocs" title=title>
            {move || draft_sig.get().map(|draft| {
                let id = draft.id;
                let writable = draft.writable;
                let name = RwSignal::new(draft.name);
                let desc = RwSignal::new(draft.description);
                let json_str = RwSignal::new(draft.json);
                let tags = RwSignal::new(draft.tags);
                view! {
                    <form class="inspection-panel" on:submit=move |ev| {
                        ev.prevent_default();
                        if !writable {
                            return;
                        }
                        let definition = match validate_block_template_json(&json_str.get_untracked()) {
                            Ok(val) => val,
                            Err(msg) => {
                                Toasts::set_error(feedback, msg);
                                return;
                            }
                        };
                        save_template.dispatch(CreateBlockTemplateRequest {
                            id: id.clone(),
                            name: name.get_untracked(),
                            description: Some(desc.get_untracked()),
                            definition,
                            tags: parse_tags(&tags.get_untracked()),
                        });
                    }>
                        <div class="form-group">
                            <label class="form-label" for="btpl-name">"Nom du modèle *"</label>
                            <input id="btpl-name" class="form-input" required=true disabled=!writable
                                prop:value=move || name.get()
                                on:input=move |e| name.set(event_target_value(&e)) />
                        </div>
                        <div class="form-group">
                            <label class="form-label" for="btpl-desc">"Description"</label>
                            <input id="btpl-desc" class="form-input" disabled=!writable
                                prop:value=move || desc.get()
                                on:input=move |e| desc.set(event_target_value(&e)) />
                        </div>
                        <div class="form-group">
                            <label class="form-label" for="btpl-tags">"Étiquettes (séparées par des virgules)"</label>
                            <input id="btpl-tags" class="form-input" disabled=!writable
                                placeholder="remux, mkv"
                                prop:value=move || tags.get()
                                on:input=move |e| tags.set(event_target_value(&e)) />
                        </div>
                        {writable.then(|| move || {
                            let available = flows.get().and_then(Result::ok).unwrap_or_default();
                            (!available.is_empty()).then(|| view! {
                                <div class="form-group">
                                    <label class="form-label" for="btpl-source">"Pré-remplir depuis un flow existant"</label>
                                    <select id="btpl-source" class="form-input" on:change=move |ev| {
                                        let chosen = event_target_value(&ev);
                                        if let Some(flow) = available.iter().find(|f| f.id == chosen) {
                                            if let Ok(parsed) = serde_json::from_str::<Value>(&flow.definition) {
                                                json_str.set(format_definition_pretty(&parsed));
                                            } else {
                                                json_str.set(flow.definition.clone());
                                            }
                                            if name.get_untracked().trim().is_empty() {
                                                name.set(format!("Blocs · {}", flow.name));
                                            }
                                        }
                                    }>
                                        <option value="">"— Choisir un flow —"</option>
                                        {available.iter().map(|f| view! {
                                            <option value=f.id.clone()>{f.name.clone()}</option>
                                        }).collect_view()}
                                    </select>
                                </div>
                            })
                        })}
                        <div class="form-group">
                            <label class="form-label" for="btpl-json">"Graphe JSON ({ nodes, connections }) *"</label>
                            <textarea id="btpl-json" class="form-input text-mono" rows="10" required=true disabled=!writable
                                prop:value=move || json_str.get()
                                on:input=move |e| json_str.set(event_target_value(&e)) />
                        </div>
                        {if writable {
                            view! {
                                <div class="inspection-drawer__actions">
                                    <button type="submit" class="btn btn-primary" disabled=move || save_template.pending().get()>
                                        "Enregistrer le modèle"
                                    </button>
                                    <button type="button" class="btn btn-secondary" on:click=move |_| open.set(false)>
                                        "Annuler"
                                    </button>
                                </div>
                            }.into_any()
                        } else {
                            view! {
                                <p class="form-help">"Modèle en lecture seule (catalogue ou droits lecteur)."</p>
                            }.into_any()
                        }}
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
    fn validate_block_template_json_checks_structure_and_edges() {
        assert!(validate_block_template_json("{\"nodes\":[],\"connections\":[]}").is_ok());
        assert!(validate_block_template_json("{invalid").is_err());
        assert!(
            validate_block_template_json(
                "{\"nodes\":[],\"connections\":[{\"from\":\"a\",\"to\":\"b\"}]}"
            )
            .is_err()
        );
    }
}
