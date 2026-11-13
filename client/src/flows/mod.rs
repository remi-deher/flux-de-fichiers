//! Flux : liste, création et éditeur (graphe, réglages générés, liaisons) (ADR 0014).

mod canvas;
mod editor;
mod fields;
mod geometry;
mod inspector;
mod palette;
mod state;

pub use editor::FlowEditor;

use crate::route::{Link, Route, encode_segment};
use crate::{api, format};
use flux_shared::Permission;
use flux_shared::flows::{FlowGraph, FlowRecord};
use flux_shared::session::Me;
use leptos::prelude::*;

const FLOWS_PATH: &str = "/api/v1/flows";

#[component]
fn FlowRow(flow: FlowRecord, can_manage: bool, on_removed: Callback<()>) -> impl IntoView {
    let blocks = FlowGraph::parse(&flow.definition).nodes.len();
    let feedback = RwSignal::new(None::<String>);
    let path = format!("{FLOWS_PATH}/{}", encode_segment(&flow.id));
    let remove = Action::new_local(move |_: &()| {
        let path = path.clone();
        async move { api::delete(&path).await }
    });
    Effect::new(move |_| match remove.value().get() {
        Some(Ok(())) => on_removed.run(()),
        Some(Err(error)) => feedback.set(Some(error.message())),
        None => {}
    });
    let removable = can_manage && !flow.is_default;
    view! {
        <tr>
            <td class="text-strong">
                {flow.name.clone()}
                {flow.is_default.then(|| view! { " " <span class="status-pill success">"Défaut"</span> })}
            </td>
            <td class="text-mono">{flow.file_mask.clone()}</td>
            <td>{format!("{blocks} bloc(s)")}</td>
            <td>{format!("v{}", flow.version)}</td>
            <td class="text-dim">{format::datetime(flow.updated_at)}</td>
            <td class="cell-end">
                <Link to=Route::FlowEditor(flow.id.clone()) class="btn btn-secondary btn-sm">"Ouvrir"</Link>
                {removable.then(|| view! {
                    <button type="button" class="btn btn-sm btn-danger-quiet" disabled=move || remove.pending().get()
                        on:click=move |_| { remove.dispatch(()); }>"Supprimer"</button>
                })}
                <span class="text-danger text-xs" role="status">{move || feedback.get()}</span>
            </td>
        </tr>
    }
}

#[component]
pub fn FlowsPage(me: Me) -> impl IntoView {
    let can_manage = me.allows(Permission::ManageFlows);
    let reload = RwSignal::new(0_u32);
    let flows = LocalResource::new(move || {
        reload.track();
        api::get::<Vec<FlowRecord>>(FLOWS_PATH)
    });
    let on_removed = Callback::new(move |()| reload.update(|count| *count = count.wrapping_add(1)));
    let rows = move || match flows.get() {
        None => view! { <tr><td colspan="6" class="text-dim">"Chargement…"</td></tr> }.into_any(),
        Some(Err(error)) => {
            view! { <tr><td colspan="6" class="text-danger">{error.message()}</td></tr> }.into_any()
        }
        Some(Ok(list)) if list.is_empty() => view! {
            <tr><td colspan="6" class="text-dim">"Aucun flux : créez-en un."</td></tr>
        }
        .into_any(),
        Some(Ok(list)) => list
            .into_iter()
            .map(|flow| view! { <FlowRow flow=flow can_manage=can_manage on_removed=on_removed /> })
            .collect_view()
            .into_any(),
    };
    view! {
        <div class="registre">
            <div class="cluster-spread">
                <h1 class="section-title">"Flux"</h1>
                {can_manage.then(|| view! { <Link to=Route::NewFlow class="btn btn-primary">"Nouveau flux"</Link> })}
            </div>
            <div class="table-wrapper">
                <table class="data-table" aria-label="Flux">
                    <thead>
                        <tr>
                            <th scope="col">"Flux"</th>
                            <th scope="col">"Masque"</th>
                            <th scope="col">"Composition"</th>
                            <th scope="col">"Version"</th>
                            <th scope="col">"Modifié"</th>
                            <th scope="col" class="cell-end">"Action"</th>
                        </tr>
                    </thead>
                    <tbody>{rows}</tbody>
                </table>
            </div>
        </div>
    }
}
