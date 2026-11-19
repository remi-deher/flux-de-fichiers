use forgearr_shared::flow_analysis::SimulationReport;
use forgearr_shared::{
    flows::{FlowGraph, FlowNode},
    libraries::{Library, LibraryOutputPolicy},
    workers::Worker,
};
use leptos::prelude::*;
use serde_json::Value;
use uuid::Uuid;

fn config<'a>(node: &'a FlowNode, key: &str) -> Option<&'a str> {
    node.config
        .get(key)
        .and_then(Value::as_str)
        .filter(|text| !text.is_empty())
}

fn node_plan(node: &FlowNode) -> Vec<String> {
    let kind = node.kind.as_str();
    if kind == "video.encode" || kind == "video-encode" {
        let video = if config(node, "video_mode") == Some("copy") {
            "Vidéo copiée".to_owned()
        } else {
            format!(
                "Vidéo encodée en {} · encodeur {}",
                config(node, "codec").unwrap_or("h265"),
                config(node, "encoder").unwrap_or("auto")
            )
        };
        let audio = format!(
            "Audio {} · {}",
            config(node, "audio_language").unwrap_or("toutes langues"),
            config(node, "audio_codec").unwrap_or("copy")
        );
        let subtitles = if node.config.get("keep_subtitles").and_then(Value::as_bool) == Some(false)
        {
            "Sous-titres retirés".to_owned()
        } else {
            format!(
                "Sous-titres conservés · {}",
                config(node, "subtitle_language").unwrap_or("toutes langues")
            )
        };
        vec![
            video,
            audio,
            subtitles,
            "Conteneur de sortie MKV".to_owned(),
        ]
    } else if node
        .config
        .get("after_publication")
        .and_then(Value::as_bool)
        == Some(true)
    {
        vec![format!("{} après publication", node.name)]
    } else {
        Vec::new()
    }
}

#[component]
pub(super) fn ProcessingPlan(
    graph: Signal<FlowGraph>,
    report: Memo<SimulationReport>,
    flow_id: RwSignal<Option<String>>,
) -> impl IntoView {
    let libraries = LocalResource::new(|| crate::api::get::<Vec<Library>>("/api/v1/libraries"));
    let workers = LocalResource::new(|| crate::api::get::<Vec<Worker>>("/api/v1/workers"));
    let selected_library = RwSignal::new(None::<Uuid>);
    let options = move || {
        let current = flow_id.get();
        libraries
            .get()
            .and_then(Result::ok)
            .unwrap_or_default()
            .into_iter()
            .filter(|library| current.is_some() && library.flow_id == current)
            .collect::<Vec<_>>()
    };
    view! {
        <section class="flow-inspector__section">
            <h3 class="flow-inspector__section-title">"Plan de traitement simulé"</h3>
            {move || {
                let steps = report.get().steps;
                let graph = graph.get();
                let entries = steps.iter().filter_map(|step| graph.nodes.iter().find(|node| node.id == step.node_id))
                    .flat_map(node_plan).collect::<Vec<_>>();
                (!entries.is_empty()).then(|| view! { <ul>{entries.into_iter().map(|entry| view! { <li>{entry}</li> }).collect_view()}</ul> })
            }}
            {move || {
                let items = options();
                (!items.is_empty()).then(|| view! {
                    <label for="plan-library">"Bibliothèque de référence"</label>
                    <select id="plan-library" class="form-select"
                        prop:value=move || selected_library.get().map_or_else(String::new, |id| id.to_string())
                        on:change=move |event| selected_library.set(event_target_value(&event).parse().ok())>
                        <option value="">"Choisir une bibliothèque"</option>
                        {items.into_iter().map(|library| view! { <option value=library.id.to_string()>{library.name}</option> }).collect_view()}
                    </select>
                })
            }}
            {move || options().into_iter().find(|item| Some(item.id) == selected_library.get()).map(|library| {
                let hosts = workers.get().and_then(Result::ok).unwrap_or_default();
                let policy = match library.output_policy {
                    LibraryOutputPolicy::OutputDir => "Répertoire de sortie de la centrale",
                    LibraryOutputPolicy::ReplaceWithBackup => "Remplacement local avec sauvegarde",
                    LibraryOutputPolicy::ReplaceInPlace => "Remplacement local",
                };
                view! {
                    <p>{policy}</p>
                    <ul>{library.workers.into_iter().map(|folder| {
                        let worker = hosts.iter().find(|worker| worker.id == folder.worker_id);
                        let host = if folder.worker_id.is_nil() { "Centrale".to_owned() } else {
                            worker.map_or_else(|| folder.worker_id.to_string(), |worker| worker.name.clone())
                        };
                        let state = worker.map(|worker| if worker.is_online(chrono::Utc::now()) { "en ligne" } else { "hors ligne" });
                        let access = match folder.access { forgearr_shared::libraries::LibraryAccess::Read => "lecture", forgearr_shared::libraries::LibraryAccess::ReadWrite => "lecture et écriture" };
                        view! { <li>{format!("{host} · {} · {access}", folder.local_path)}{state.map(|state| format!(" · {state}"))}</li> }
                    }).collect_view()}</ul>
                    <p class="text-dim">"Les montages sont déclaratifs. Le placement et les outils sont vérifiés lors du traitement."</p>
                }
            })}
        </section>
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preview_distinguishes_copy_and_audio_selection() -> serde_json::Result<()> {
        let node = serde_json::from_str(
            r#"{"id":"n","name":"Remux","type":"video.encode","config":{"video_mode":"copy","audio_language":"fra","keep_subtitles":false}}"#,
        )?;
        let lines = node_plan(&node);
        assert_eq!(
            &lines[..3],
            ["Vidéo copiée", "Audio fra · copy", "Sous-titres retirés"]
        );
        Ok(())
    }
}
