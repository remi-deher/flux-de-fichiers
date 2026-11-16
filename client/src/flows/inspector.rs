//! Inspecteur latéral repliable du bloc sélectionné : réglages, liaisons, duplication et retrait.

use super::dnd::BranchTarget;
use super::fields::FieldControl;
use super::presentation::{NodeDomain, node_output_ports, output_presentation};
use super::state::Editor;
use crate::ui::{Icon, IconName};
use flux_shared::flows::{FlowGraph, OwnedBlockSpec, find_owned};
use leptos::prelude::*;

const ACTION_ICON_SIZE: u32 = 14;

/// Libellé d'une sortie : celui de la fiche, sinon la présentation sémantique du port.
fn port_label(
    catalog: &[OwnedBlockSpec],
    graph: &FlowGraph,
    node_id: &str,
    port: &Option<String>,
) -> String {
    let Some(port_name) = port else {
        return "Suite".into();
    };
    graph
        .node(node_id)
        .and_then(|node| find_owned(catalog, &node.kind))
        .and_then(|spec| spec.ports.iter().find(|(name, _)| name == port_name))
        .map_or_else(
            || output_presentation(port_name).label,
            |(_, label)| label.clone(),
        )
}

/// Relie (ou délie, `target` vide) la sortie `port` ; le graphe ne change pas si c'est refusé.
pub fn relink(
    graph: &FlowGraph,
    from: &str,
    port: &Option<String>,
    target: &str,
    catalog: &[OwnedBlockSpec],
) -> Result<FlowGraph, &'static str> {
    let mut next = graph.clone();
    next.disconnect(from, port);
    if !target.is_empty() {
        next.connect(from, port.clone(), target, catalog)
            .map_err(|error| error.message())?;
    }
    Ok(next)
}

#[component]
fn Outputs(editor: Editor, node_id: String, can_manage: bool) -> impl IntoView {
    let feedback = RwSignal::new(None::<&'static str>);
    let id = StoredValue::new(node_id);
    move || {
        let graph = editor.graph.get();
        let catalog = editor.catalog.get();
        let node_id = id.get_value();
        let Some(node) = graph.node(&node_id) else {
            return ().into_any();
        };
        let ports = node_output_ports(&graph, node, &catalog);
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
            <div class="flow-inspector__section">
                <h3 class="flow-inspector__section-title">"Sorties & liaisons"</h3>
                {ports.into_iter().map(|port| {
                    let current = graph
                        .outgoing(&node_id)
                        .find(|edge| edge.port == port)
                        .map(|edge| edge.to.clone())
                        .unwrap_or_default();
                    let label = port_label(&catalog, &graph, &node_id, &port);
                    let select_id = format!("output-{node_id}-{}", port.clone().unwrap_or_default());
                    let from = node_id.clone();
                    let branch_from = node_id.clone();
                    let branch_port = port.clone();
                    let existing_to = (!current.is_empty()).then(|| current.clone());
                    view! {
                        <div class="form-group">
                            <div class="flow-inspector__output-head">
                                <label for=select_id.clone() class="form-label">{label}</label>
                                {can_manage.then(|| view! {
                                    <button
                                        type="button"
                                        class="btn btn-secondary btn-sm"
                                        on:click=move |_| {
                                            editor.branch_target.set(Some(BranchTarget {
                                                source_id: branch_from.clone(),
                                                port: branch_port.clone(),
                                                replace_to: existing_to.clone(),
                                            }));
                                            editor.focus_palette_search();
                                        }
                                    >
                                        <Icon name=IconName::Plus size=12 />
                                        <span>"Insérer"</span>
                                    </button>
                                })}
                            </div>
                            <select
                                id=select_id
                                class="form-select"
                                disabled=!can_manage
                                prop:value=current.clone()
                                on:change=move |event| {
                                    let target = event_target_value(&event);
                                    let catalog = editor.catalog.get_untracked();
                                    match editor
                                        .graph
                                        .with_untracked(|graph| relink(graph, &from, &port, &target, &catalog))
                                    {
                                        Ok(next) => {
                                            editor.mutate_graph(|graph| *graph = next);
                                            feedback.set(None);
                                        }
                                        Err(message) => feedback.set(Some(message)),
                                    }
                                }
                            >
                                <option value="">"Non reliée"</option>
                                {targets.iter().map(|(id, name)| view! {
                                    <option value=id.clone() selected=*id == current>{name.clone()}</option>
                                }).collect_view()}
                            </select>
                        </div>
                    }
                }).collect_view()}
                <p class="form-help text-danger" role="status">{move || feedback.get()}</p>
            </div>
        }
        .into_any()
    }
}

#[component]
pub fn NodeInspector(editor: Editor, can_manage: bool) -> impl IntoView {
    view! {
        <div class="flow-dock-pane flow-inspector-pane" aria-label="Inspecteur du bloc">
            {move || {
                let node_id = editor.selected.get()?;
                if !editor.inspector_open.get() {
                    return None;
                }
                let catalog = editor.catalog.get();
                let node = editor
                    .graph
                    .with(|graph| graph.node(&node_id).cloned())?;
                let spec = find_owned(&catalog, &node.kind).cloned();
                let category = spec.as_ref().map_or("Bloc", |s| s.category.as_str());
                let kind_label = spec
                    .as_ref()
                    .map(|s| s.label.clone())
                    .unwrap_or_else(|| node.kind.as_str().to_owned());
                let domain = NodeDomain::from_spec(spec.as_ref(), category);
                let description = spec.as_ref().map(|s| s.description.clone());
                let from_plugin = spec.as_ref().is_some_and(|s| s.from_plugin);
                let is_script = spec
                    .as_ref()
                    .and_then(|s| s.owner.as_deref())
                    .is_some_and(|owner| owner.starts_with("script."));
                let fields = spec
                    .as_ref()
                    .map(|s| s.fields.clone())
                    .unwrap_or_default();
                let name_id = node_id.clone();
                let dup_id = node_id.clone();
                let remove_id = node_id.clone();

                Some(view! {
                    <div class="flow-inspector__shell">
                        <header class="flow-inspector__header">
                            <div class="flow-inspector__identity">
                                <span
                                    class="flow-inspector__badge"
                                    style=format!("--node-accent: {}", domain.accent_var())
                                >
                                    <Icon name=domain.icon() size=13 />
                                    <span>{format!("{category} · {kind_label}")}</span>
                                </span>
                                <h2 class="flow-inspector__title">{node.name.clone()}</h2>
                            </div>
                        </header>

                        <div class="flow-inspector__body">
                            {description.map(|text| view! { <p class="form-help">{text}</p> })}
                            {is_script.then(|| view! {
                                <p class="form-help">"Bloc script (stdin/stdout JSON) : configurez le code depuis Librairies → Scripts."</p>
                            })}
                            {(from_plugin && !is_script).then(|| view! {
                                <p class="form-help">"Bloc fourni par un dépôt de plugins."</p>
                            })}
                            {spec.is_none().then(|| view! {
                                <p class="form-help">
                                    "Bloc sans exécution native : il est traversé sans effet, ses réglages sont conservés tels quels."
                                </p>
                            })}

                            <div class="form-group">
                                <label for="inspector-name" class="form-label">"Nom de l'étape"</label>
                                <input
                                    id="inspector-name"
                                    class="form-input"
                                    disabled=!can_manage
                                    prop:value=node.name
                                    on:input=move |event| {
                                        let name = event_target_value(&event);
                                        let id = name_id.clone();
                                        editor.edit(move |graph| {
                                            if let Some(node) = graph.node_mut(&id) {
                                                node.name = name;
                                            }
                                        });
                                    }
                                />
                            </div>

                            {fields.into_iter().map(|field| view! {
                                <FieldControl editor=editor node_id=node_id.clone() field=field />
                            }).collect_view()}

                            <Outputs editor=editor node_id=node_id.clone() can_manage=can_manage />
                        </div>

                        {can_manage.then(|| view! {
                            <footer class="flow-inspector__footer">
                                <button
                                    type="button"
                                    class="btn btn-secondary btn-sm"
                                    on:click=move |_| {
                                        editor.duplicate_node(&dup_id);
                                    }
                                >
                                    <Icon name=IconName::Copy size=ACTION_ICON_SIZE />
                                    <span>"Dupliquer"</span>
                                </button>
                                <button
                                    type="button"
                                    class="btn btn-danger-quiet btn-sm"
                                    on:click=move |_| {
                                        editor.remove_node(&remove_id);
                                    }
                                >
                                    <Icon name=IconName::Trash size=ACTION_ICON_SIZE />
                                    <span>"Retirer ce bloc"</span>
                                </button>
                            </footer>
                        })}
                    </div>
                })
            }}
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_support::test_catalog;
    use super::*;
    use flux_shared::flows::FlowBlockKind;
    use serde_json::json;

    #[test]
    fn a_refused_link_leaves_the_graph_unchanged() {
        let catalog = test_catalog();
        let mut graph = FlowGraph::default();
        let input = graph.append(
            FlowBlockKind::parse("block.core.file-input"),
            "Entrée",
            json!({}),
            &catalog,
        );
        let end = graph.append(
            FlowBlockKind::parse("block.core.file-output"),
            "Fin",
            json!({}),
            &catalog,
        );
        assert!(relink(&graph, &end, &None, &input, &catalog).is_err());
        let unlinked = relink(&graph, &input, &None, "", &catalog);
        assert_eq!(unlinked.map(|next| next.connections.len()), Ok(0));
        assert_eq!(port_label(&[], &graph, &input, &None), "Suite");
    }
}
