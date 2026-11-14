//! Plan du graphe : cartes disposées en couches et liaisons SVG, recalculées depuis le modèle.

use super::dnd::DragPayload;
use super::drag::DragItem;
use super::geometry::{
    CARD_HEIGHT, CARD_WIDTH, board_size, card_origin, input_anchor, link_path, output_anchor,
};
use super::state::Editor;
use flux_shared::flows::{block_spec, layout};
use leptos::prelude::*;

fn link_class(port: Option<&str>) -> &'static str {
    match port {
        Some("true") => "flow-graph__link flow-graph__link--true",
        Some("false") => "flow-graph__link flow-graph__link--false",
        _ => "flow-graph__link",
    }
}

#[component]
pub fn GraphCanvas(editor: Editor) -> impl IntoView {
    move || {
        let graph = editor.graph.get();
        if graph.nodes.is_empty() {
            return view! {
                <div class="flow-graph-board empty-state-panel">
                    <p class="empty-state-title">"Aucun bloc : ajoutez-en depuis la palette."</p>
                </div>
            }
            .into_any();
        }
        let places = layout(&graph);
        let size = board_size(places.values());
        let links = graph
            .connections
            .iter()
            .filter_map(|edge| {
                let from = graph.node(&edge.from)?;
                let ports = graph.output_ports(from);
                let index = ports.iter().position(|port| *port == edge.port)?;
                let start = output_anchor(*places.get(&edge.from)?, index, ports.len());
                let end = input_anchor(*places.get(&edge.to)?);
                let label = edge.port.clone().map(|port| {
                    view! {
                        <text class="flow-graph__port-label" x=start.x + 4.0 y=start.y + 14.0>{port}</text>
                    }
                });
                Some(view! {
                    <path class=link_class(edge.port.as_deref()) d=link_path(start, end) />
                    {label}
                })
            })
            .collect_view();
        let cards = graph
            .nodes
            .iter()
            .filter_map(|node| {
                let origin = card_origin(*places.get(&node.id)?);
                let spec = block_spec(&node.kind);
                let kind = spec.map_or_else(|| node.kind.as_str().to_owned(), |spec| spec.label.to_owned());
                let incomplete = spec.is_some_and(|spec| !spec.missing_fields(&node.config).is_empty());
                let id = node.id.clone();
                let name = node.name.clone();
                let selected_id = id.clone();
                let style = format!(
                    "left:{}px;top:{}px;width:{CARD_WIDTH}px;height:{CARD_HEIGHT}px",
                    origin.x, origin.y
                );
                Some(view! {
                    <DragItem editor=editor payload=DragPayload::Card(id.clone()) target=id.clone() class="flow-graph__slot" style=style>
                        <button
                            type="button"
                            class="flow-graph__card"
                            class:is-selected=move || editor.selected.get().as_deref() == Some(selected_id.as_str())
                            class:is-incomplete=incomplete
                            on:click=move |_| editor.selected.set(Some(id.clone()))
                        >
                            <div class="flow-graph__kind">{kind}</div>
                            <div class="flow-graph__name">{name}</div>
                        </button>
                    </DragItem>
                })
            })
            .collect_view();
        view! {
            <div class="flow-graph-board">
                <div class="flow-graph" style=format!("width:{}px;height:{}px", size.x, size.y)>
                    <svg class="flow-graph__links" width=size.x height=size.y aria-hidden="true">{links}</svg>
                    {cards}
                </div>
                <p class="form-help text-danger" role="status">{move || editor.drop_notice.get()}</p>
            </div>
        }
        .into_any()
    }
}
