//! Écran Bibliothèques : liste, état et édition (nom, flux, chemins par worker) (ADR 0014).

mod draft;
mod form;

use crate::api;
use crate::ui::search::{matches, query};
use crate::ui::{Drawer, Registre, RegistreMessage, StatusBadge, SubNav, SubTab};
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
    let drawer = RwSignal::new(false);
    let on_done = Callback::new(move |()| {
        editing.set(None);
        drawer.set(false);
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

    Effect::new(move |_| {
        if !drawer.get() {
            editing.set(None);
        }
    });
    const HEADERS: [&str; 5] = ["Bibliothèque", "Flux", "Workers", "État", "Action"];
    let rows = move || {
        let text = query();
        match libraries.get() {
            None => view! { <RegistreMessage span=HEADERS.len() text="Chargement des bibliothèques…" /> }.into_any(),
            Some(Err(error)) => view! { <RegistreMessage span=HEADERS.len() text=error.message() /> }.into_any(),
            Some(Ok(list)) => {
                let shown: Vec<Library> = list.into_iter().filter(|library| matches(&[&library.name], &text)).collect();
                if shown.is_empty() {
                    return view! { <RegistreMessage span=HEADERS.len() text="Aucune bibliothèque déclarée." /> }.into_any();
                }
                shown.into_iter().map(|library| {
                    let ready = is_ready(&library);
                    let draft = LibraryDraft::editing(&library);
                    let id = library.id;
                    view! {
                        <tr class="registre-row" class:is-open=move || editing.get().and_then(|draft| draft.id) == Some(id)>
                            <td class="text-strong">{library.name.clone()}</td>
                            <td>{flow_name(&library.flow_id)}</td>
                            <td>{library.workers.len()}</td>
                            <td><StatusBadge label=if ready { "Prête" } else { "À configurer" } variant=if ready { "success" } else { "warning" } /></td>
                            <td class="cell-end">
                                {can_manage.then(|| view! {
                                    <button type="button" class="btn btn-secondary btn-sm registre-open-btn"
                                        on:click=move |_| { editing.set(Some(draft.clone())); drawer.set(true); }>"Modifier"</button>
                                })}
                            </td>
                        </tr>
                    }
                }).collect_view().into_any()
            }
        }
    };
    let form = move || {
        let draft = editing.get()?;
        let flows = flows.get().and_then(Result::ok).unwrap_or_default();
        let workers = workers.get().and_then(Result::ok).unwrap_or_default();
        Some(view! { <LibraryForm draft=draft flows=flows workers=workers on_done=on_done /> })
    };
    let title = Signal::derive(move || match editing.get() {
        Some(draft) if draft.id.is_some() => draft.name,
        _ => "Nouvelle bibliothèque".to_owned(),
    });
    let tab = RwSignal::new(0_usize);
    let actions = can_manage.then(|| view! {
        <button type="button" class="btn btn-primary btn-sm"
            on:click=move |_| { editing.set(Some(LibraryDraft::default())); drawer.set(true); }>"Nouvelle bibliothèque"</button>
    }.into_any());

    view! {
        <div class="tab-panel" role="tabpanel">
            <SubNav aria_label="Sous-navigation des bibliothèques" tabs=vec![SubTab::new("Bibliothèques", "📚")] selected=tab
                actions=actions.unwrap_or_else(|| ().into_any()) />
            <div class="registre">
                <Registre aria_label="Bibliothèques déclarées" headers=HEADERS.to_vec()>{rows}</Registre>
            </div>
            <Drawer open=drawer eyebrow="Bibliothèque" title=title>{form}</Drawer>
        </div>
    }
}
