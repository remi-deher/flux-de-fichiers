//! Flux : liste, création et éditeur (graphe, réglages générés, liaisons) (ADR 0014).

mod canvas;
mod dnd;
mod drag;
mod editor;
mod fields;
mod geometry;
mod inspector;
mod palette;
mod state;
mod versions;

pub use editor::FlowEditor;

use crate::route::{Link, Route, encode_segment};
use crate::ui::search::{matches, query};
use crate::ui::{Registre, RegistreMessage, StatusBadge, SubNav, SubTab};
use crate::{api, format};
use flux_shared::Permission;
use flux_shared::flows::{FlowGraph, FlowRecord};
use flux_shared::session::Me;
use leptos::prelude::*;

const FLOWS_PATH: &str = "/api/v1/flows";

const HEADERS: [&str; 6] = [
    "Flux",
    "Masque",
    "Composition",
    "Version",
    "Modifié",
    "Action",
];

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
        <tr class="registre-row">
            <td class="text-strong">
                {flow.name.clone()}
                {flow.is_default.then(|| view! { " " <StatusBadge label="Défaut" variant="success" /> })}
            </td>
            <td class="text-mono">{flow.file_mask.clone()}</td>
            <td>{if blocks == 0 { "Pipeline vide".to_owned() } else { format!("{blocks} blocs") }}</td>
            <td>{format!("v{}", flow.version)}</td>
            <td class="text-dim">{format::datetime(flow.updated_at)}</td>
            <td class="cell-end">
                <Link to=Route::FlowEditor(flow.id.clone()) class="btn btn-secondary btn-sm registre-open-btn">"Ouvrir"</Link>
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
    let tab = RwSignal::new(0_usize);
    let rows = move || {
        let text = query();
        match flows.get() {
            None => view! { <RegistreMessage span=HEADERS.len() text="Chargement des flux…" /> }
                .into_any(),
            Some(Err(error)) => {
                view! { <RegistreMessage span=HEADERS.len() text=error.message() /> }.into_any()
            }
            Some(Ok(list)) => {
                let shown: Vec<FlowRecord> = list
                    .into_iter()
                    .filter(|flow| matches(&[&flow.name, &flow.id, &flow.file_mask], &text))
                    .collect();
                if shown.is_empty() {
                    return view! { <RegistreMessage span=HEADERS.len() text="Aucun flux installé. Créez-en un." /> }.into_any();
                }
                shown
                    .into_iter()
                    .map(|flow| view! { <FlowRow flow=flow can_manage=can_manage on_removed=on_removed /> })
                    .collect_view()
                    .into_any()
            }
        }
    };
    let actions = can_manage.then(|| {
        view! { <Link to=Route::NewFlow class="btn btn-primary btn-sm">"Nouveau flux"</Link> }
            .into_any()
    });
    view! {
        <div class="tab-panel" role="tabpanel">
            <SubNav aria_label="Sous-navigation des flux" tabs=vec![SubTab::new("Flux installés", "📦")] selected=tab
                actions=actions.unwrap_or_else(|| ().into_any()) />
            <div class="registre">
                <Registre aria_label="Flux" headers=HEADERS.to_vec()>{rows}</Registre>
            </div>
        </div>
    }
}
