//! Éditeur d'un flux : métadonnées, palette, plan du graphe, inspecteur et enregistrement.

use super::canvas::GraphCanvas;
use super::inspector::NodeInspector;
use super::palette::Palette;
use super::state::{Editor, incomplete_blocks};
use crate::api;
use crate::route::{Link, Route, encode_segment, use_router};
use chrono::{DateTime, Utc};
use flux_shared::Permission;
use flux_shared::flows::{
    FlowBlockKind, FlowGraph, FlowRecord, UpdateFlowRequest, block_spec, new_flow_id,
};
use flux_shared::session::Me;
use leptos::prelude::*;

const FLOWS_PATH: &str = "/api/v1/flows";
const DEFAULT_FILE_MASK: &str = "*.mkv";

/// Flux encore jamais enregistré : une entrée, pas d'identifiant.
fn blank_flow() -> (FlowRecord, FlowGraph) {
    let mut graph = FlowGraph::default();
    let input = FlowBlockKind::FileInput;
    let label = block_spec(&input).map_or("Fichier source", |spec| spec.label);
    graph.append(input, label, serde_json::json!({}));
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
pub fn save_blocker(graph: &FlowGraph) -> Option<String> {
    let incomplete = incomplete_blocks(graph);
    (!incomplete.is_empty()).then(|| {
        let list = incomplete
            .into_iter()
            .map(|(block, fields)| format!("{block} ({})", fields.join(", ")))
            .collect::<Vec<_>>()
            .join(" ; ");
        format!("Réglages obligatoires manquants : {list}.")
    })
}

#[component]
fn Workspace(me: Me, record: FlowRecord, graph: FlowGraph) -> impl IntoView {
    let can_manage = me.allows(Permission::ManageFlows);
    let router = use_router();
    let editor = Editor::new(graph);
    let meta = RwSignal::new(record);
    let feedback = RwSignal::new(None::<String>);
    let save = Action::new_local(move |(id, body): &(String, UpdateFlowRequest)| {
        let path = format!("{FLOWS_PATH}/{}", encode_segment(id));
        let body = body.clone();
        async move { api::put::<UpdateFlowRequest, FlowRecord>(&path, &body).await }
    });
    Effect::new(move |_| match save.value().get() {
        Some(Ok(saved)) => {
            let created = meta.get_untracked().id.is_empty();
            feedback.set(Some(format!("Enregistré : version {}.", saved.version)));
            editor.dirty.set(false);
            if created {
                router.go(Route::FlowEditor(saved.id.clone()));
            }
            meta.set(saved);
        }
        Some(Err(error)) => feedback.set(Some(error.message())),
        None => {}
    });
    let on_save = move |_| {
        let graph = editor.graph.get_untracked();
        if let Some(message) = save_blocker(&graph) {
            feedback.set(Some(message));
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
                feedback.set(Some("Donnez un nom au flux.".into()))
            }
            Ok(body) => {
                save.dispatch((id, body));
            }
            Err(_) => feedback.set(Some("Graphe illisible : rien n'a été enregistré.".into())),
        }
    };

    view! {
        <div class="registre">
            <div class="cluster-spread">
                <Link to=Route::Flows class="btn btn-secondary btn-sm">"← Flux"</Link>
                <span class="text-dim">
                    {move || match meta.get().version {
                        0 => "Jamais enregistré".to_owned(),
                        version => format!("Version {version}"),
                    }}
                    {move || editor.dirty.get().then_some(" • modifications non enregistrées")}
                </span>
                {can_manage.then(|| view! {
                    <button type="button" class="btn btn-primary" disabled=move || save.pending().get() on:click=on_save>
                        "Enregistrer"
                    </button>
                })}
            </div>
            <div class="form-grid-2">
                <div class="form-group">
                    <label for="flow-name" class="form-label">"Nom"</label>
                    <input id="flow-name" class="form-input" prop:value=move || meta.get().name
                        on:input=move |event| { meta.update(|record| record.name = event_target_value(&event)); editor.dirty.set(true); } />
                </div>
                <div class="form-group">
                    <label for="flow-mask" class="form-label">"Masque de fichiers"</label>
                    <input id="flow-mask" class="form-input" prop:value=move || meta.get().file_mask
                        on:input=move |event| { meta.update(|record| record.file_mask = event_target_value(&event)); editor.dirty.set(true); } />
                </div>
                <div class="form-group">
                    <label for="flow-description" class="form-label">"Description"</label>
                    <input id="flow-description" class="form-input" prop:value=move || meta.get().description.unwrap_or_default()
                        on:input=move |event| {
                            let text = event_target_value(&event);
                            meta.update(|record| record.description = (!text.trim().is_empty()).then_some(text));
                            editor.dirty.set(true);
                        } />
                </div>
                <label class="cluster">
                    <input type="checkbox" prop:checked=move || meta.get().is_default
                        on:change=move |event| { meta.update(|record| record.is_default = event_target_checked(&event)); editor.dirty.set(true); } />
                    <span>"Flux par défaut des nouvelles tâches"</span>
                </label>
            </div>
            <p class="form-help" role="status">{move || feedback.get()}</p>
            {(!can_manage).then(|| view! { <p class="form-help">"Lecture seule : modifier un flux demande le droit de gestion des flux."</p> })}
            <div class="flow-editor">
                {can_manage.then(|| view! { <Palette editor=editor /> })}
                <GraphCanvas editor=editor />
                <aside class="flow-inspector" aria-label="Bloc sélectionné">
                    <NodeInspector editor=editor />
                </aside>
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
    use super::*;
    use serde_json::json;

    #[test]
    fn a_blank_flow_starts_with_its_input_and_no_id() {
        let (record, graph) = blank_flow();
        assert!(record.id.is_empty());
        assert_eq!(graph.nodes.len(), 1);
        assert_eq!(graph.nodes[0].kind, FlowBlockKind::FileInput);
    }

    #[test]
    fn saving_is_blocked_by_an_empty_required_field() {
        let mut graph = FlowGraph::default();
        graph.append(FlowBlockKind::Webhook, "Hook", json!({}));
        assert!(save_blocker(&graph).is_some_and(|message| message.contains("Hook (URL)")));
        graph.nodes[0].config = json!({"url": "https://hook.example"});
        assert_eq!(save_blocker(&graph), None);
        assert_eq!(encode_segment("flux à"), "flux%20%C3%A0");
    }
}
