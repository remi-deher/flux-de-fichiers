//! Écran Bibliothèques : liste, état et édition (nom, flux, chemins par worker) (ADR 0014).

mod draft;
mod form;

use crate::api;
use draft::{LibraryDraft, is_ready};
use flux_shared::Permission;
use flux_shared::flows::FlowRecord;
use flux_shared::libraries::Library;
use flux_shared::session::Me;
use flux_shared::workers::Worker;
use form::LibraryForm;
use leptos::prelude::*;

const LIBRARIES_PATH: &str = "/api/v1/libraries";
const FLOWS_PATH: &str = "/api/v1/flows";
const WORKERS_PATH: &str = "/api/v1/workers";

#[component]
pub fn LibrariesPage(me: Me) -> impl IntoView {
    let can_manage = me.allows(Permission::ManageLibraries);
    let reload = RwSignal::new(0_u32);
    let libraries = LocalResource::new(move || {
        reload.track();
        api::get::<Vec<Library>>(LIBRARIES_PATH)
    });
    let flows = LocalResource::new(|| api::get::<Vec<FlowRecord>>(FLOWS_PATH));
    let workers = LocalResource::new(|| api::get::<Vec<Worker>>(WORKERS_PATH));
    let editing = RwSignal::new(None::<LibraryDraft>);
    let on_done = Callback::new(move |()| {
        editing.set(None);
        reload.update(|count| *count = count.wrapping_add(1));
    });
    let flow_name = move |id: &Option<String>| {
        let Some(id) = id else {
            return "Aucun".to_owned();
        };
        flows
            .get()
            .and_then(Result::ok)
            .and_then(|list| {
                list.into_iter()
                    .find(|flow| &flow.id == id)
                    .map(|flow| flow.name)
            })
            .unwrap_or_else(|| id.clone())
    };

    let rows = move || {
        match libraries.get() {
        None => view! { <tr><td colspan="5" class="text-dim">"Chargement…"</td></tr> }.into_any(),
        Some(Err(error)) => view! { <tr><td colspan="5" class="text-danger">{error.message()}</td></tr> }.into_any(),
        Some(Ok(list)) if list.is_empty() => view! {
            <tr><td colspan="5" class="text-dim">"Aucune bibliothèque déclarée."</td></tr>
        }.into_any(),
        Some(Ok(list)) => list.into_iter().map(|library| {
            let ready = is_ready(&library);
            let draft = LibraryDraft::editing(&library);
            view! {
                <tr>
                    <td class="text-strong">{library.name.clone()}</td>
                    <td>{flow_name(&library.flow_id)}</td>
                    <td>{library.workers.len()}</td>
                    <td>
                        <span class=if ready { "status-pill success" } else { "status-pill warning" }>
                            {if ready { "Prête" } else { "À configurer" }}
                        </span>
                    </td>
                    <td class="cell-end">
                        {can_manage.then(|| view! {
                            <button type="button" class="btn btn-secondary btn-sm"
                                on:click=move |_| editing.set(Some(draft.clone()))>"Modifier"</button>
                        })}
                    </td>
                </tr>
            }
        }).collect_view().into_any(),
    }
    };

    let form = move || {
        let draft = editing.get()?;
        let flows = flows.get().and_then(Result::ok).unwrap_or_default();
        let workers = workers.get().and_then(Result::ok).unwrap_or_default();
        Some(view! { <LibraryForm draft=draft flows=flows workers=workers on_done=on_done /> })
    };

    view! {
        <div class="registre">
            <div class="cluster-spread">
                <h1 class="section-title">"Bibliothèques"</h1>
                {can_manage.then(|| view! {
                    <button type="button" class="btn btn-primary"
                        on:click=move |_| editing.set(Some(LibraryDraft::default()))>"Nouvelle bibliothèque"</button>
                })}
            </div>
            {form}
            <div class="table-wrapper">
                <table class="data-table" aria-label="Bibliothèques">
                    <thead>
                        <tr>
                            <th scope="col">"Bibliothèque"</th>
                            <th scope="col">"Flux"</th>
                            <th scope="col">"Workers"</th>
                            <th scope="col">"État"</th>
                            <th scope="col" class="cell-end">"Action"</th>
                        </tr>
                    </thead>
                    <tbody>{rows}</tbody>
                </table>
            </div>
        </div>
    }
}
