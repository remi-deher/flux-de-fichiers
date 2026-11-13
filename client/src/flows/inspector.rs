//! Inspecteur du bloc sélectionné : nom, réglages générés, sorties et retrait.

use super::fields::FieldControl;
use super::state::Editor;
use flux_shared::flows::{FlowGraph, block_spec};
use leptos::prelude::*;

/// Libellé d'une sortie : celui de la fiche, sinon le nom du port.
fn port_label(graph: &FlowGraph, node_id: &str, port: &Option<String>) -> String {
    let Some(port) = port else {
        return "Suite".into();
    };
    graph
        .node(node_id)
        .and_then(|node| block_spec(&node.kind))
        .and_then(|spec| spec.ports.iter().find(|(name, _)| name == port))
        .map_or_else(|| port.clone(), |(_, label)| (*label).to_owned())
}

/// Relie (ou délie, `target` vide) la sortie `port` ; le graphe ne change pas si c'est refusé.
pub fn relink(
    graph: &FlowGraph,
    from: &str,
    port: &Option<String>,
    target: &str,
) -> Result<FlowGraph, &'static str> {
    let mut next = graph.clone();
    next.disconnect(from, port);
    if !target.is_empty() {
        next.connect(from, port.clone(), target)
            .map_err(|error| error.message())?;
    }
    Ok(next)
}

#[component]
fn Outputs(editor: Editor, node_id: String) -> impl IntoView {
    let feedback = RwSignal::new(None::<&'static str>);
    let id = StoredValue::new(node_id);
    move || {
        let graph = editor.graph.get();
        let node_id = id.get_value();
        let Some(node) = graph.node(&node_id) else {
            return ().into_any();
        };
        let ports = graph.output_ports(node);
        if ports.is_empty() {
            return view! { <p class="form-help">"Dernier bloc : le flux s'arrête ici."</p> }
                .into_any();
        }
        let targets: Vec<(String, String)> = graph
            .nodes
            .iter()
            .filter(|candidate| candidate.id != node_id)
            .map(|candidate| (candidate.id.clone(), candidate.name.clone()))
            .collect();
        view! {
            <h3 class="section-title">"Sorties"</h3>
            {ports.into_iter().map(|port| {
                let current = graph.outgoing(&node_id).find(|edge| edge.port == port).map(|edge| edge.to.clone()).unwrap_or_default();
                let label = port_label(&graph, &node_id, &port);
                let select_id = format!("output-{node_id}-{}", port.clone().unwrap_or_default());
                let from = node_id.clone();
                view! {
                    <div class="form-group">
                        <label for=select_id.clone() class="form-label">{label}</label>
                        <select id=select_id class="form-select" prop:value=current.clone()
                            on:change=move |event| {
                                let target = event_target_value(&event);
                                match editor.graph.with(|graph| relink(graph, &from, &port, &target)) {
                                    Ok(next) => {
                                        editor.edit(|graph| *graph = next);
                                        feedback.set(None);
                                    }
                                    Err(message) => feedback.set(Some(message)),
                                }
                            }>
                            <option value="">"Non reliée"</option>
                            {targets.iter().map(|(id, name)| view! {
                                <option value=id.clone() selected=*id == current>{name.clone()}</option>
                            }).collect_view()}
                        </select>
                    </div>
                }
            }).collect_view()}
            <p class="form-help text-danger" role="status">{move || feedback.get()}</p>
        }
        .into_any()
    }
}

#[component]
pub fn NodeInspector(editor: Editor) -> impl IntoView {
    move || {
        let Some(node_id) = editor.selected.get() else {
            return view! { <p class="form-help">"Sélectionnez un bloc pour voir ses réglages."</p> }.into_any();
        };
        let Some(node) = editor
            .graph
            .with_untracked(|graph| graph.node(&node_id).cloned())
        else {
            return view! { <p class="form-help">"Sélectionnez un bloc pour voir ses réglages."</p> }.into_any();
        };
        let spec = block_spec(&node.kind);
        let name_id = node_id.clone();
        let remove_id = node_id.clone();
        let fields = spec.map(|spec| spec.fields).unwrap_or_default();
        view! {
            <div class="form-group">
                <span class="status-pill info">{spec.map_or_else(|| node.kind.as_str().to_owned(), |spec| spec.label.to_owned())}</span>
                {spec.map(|spec| view! { <p class="form-help">{spec.description}</p> })}
                {spec.is_none().then(|| view! {
                    <p class="form-help">"Bloc sans exécution native : il est traversé sans effet, ses réglages sont conservés tels quels."</p>
                })}
            </div>
            <div class="form-group">
                <label for="inspector-name" class="form-label">"Nom du bloc"</label>
                <input id="inspector-name" class="form-input" prop:value=node.name.clone()
                    on:input=move |event| {
                        let name = event_target_value(&event);
                        let id = name_id.clone();
                        editor.edit(move |graph| {
                            if let Some(node) = graph.node_mut(&id) {
                                node.name = name;
                            }
                        });
                    } />
            </div>
            {fields.iter().map(|field| view! {
                <FieldControl editor=editor node_id=node_id.clone() field=*field />
            }).collect_view()}
            <Outputs editor=editor node_id=node_id.clone() />
            <button type="button" class="btn btn-danger-quiet" on:click=move |_| {
                let id = remove_id.clone();
                editor.edit(move |graph| graph.remove(&id));
                editor.selected.set(None);
            }>"Retirer ce bloc"</button>
        }
        .into_any()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use flux_shared::flows::FlowBlockKind;
    use serde_json::json;

    #[test]
    fn a_refused_link_leaves_the_graph_unchanged() {
        let mut graph = FlowGraph::default();
        let input = graph.append(FlowBlockKind::FileInput, "Entrée", json!({}));
        let end = graph.append(FlowBlockKind::FileOutput, "Fin", json!({}));
        assert!(relink(&graph, &end, &None, &input).is_err());
        let unlinked = relink(&graph, &input, &None, "");
        assert_eq!(unlinked.map(|next| next.connections.len()), Ok(0));
        assert_eq!(port_label(&graph, &input, &None), "Suite");
    }
}
