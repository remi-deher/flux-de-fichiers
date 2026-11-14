//! Formulaire d'une bibliothèque : nom, flux et chemins par worker.

use super::draft::{LibraryDraft, access_label};
use crate::api;
use flux_shared::flows::FlowRecord;
use flux_shared::libraries::{Library, LibraryAccess, LibraryWrite};
use flux_shared::workers::Worker;
use leptos::prelude::*;
use uuid::Uuid;

const LIBRARIES_PATH: &str = "/api/v1/libraries";

async fn save(id: Option<Uuid>, body: LibraryWrite) -> Result<Library, api::ApiError> {
    match id {
        Some(id) => api::put(&format!("{LIBRARIES_PATH}/{id}"), &body).await,
        None => api::post(LIBRARIES_PATH, Some(&body)).await,
    }
}

#[component]
pub fn LibraryForm(
    draft: LibraryDraft,
    flows: Vec<FlowRecord>,
    workers: Vec<Worker>,
    on_done: Callback<()>,
) -> impl IntoView {
    let draft = RwSignal::new(draft);
    let feedback = RwSignal::new(None::<String>);
    let new_worker = RwSignal::new(String::new());
    let new_path = RwSignal::new(String::new());
    let new_access = RwSignal::new(LibraryAccess::ReadWrite);
    let worker_names = StoredValue::new(
        workers
            .iter()
            .map(|worker| (worker.id, worker.name.clone()))
            .collect::<Vec<_>>(),
    );
    let worker_name = move |id: Uuid| {
        worker_names
            .read_value()
            .iter()
            .find(|(candidate, _)| *candidate == id)
            .map_or_else(|| id.to_string(), |(_, name)| name.clone())
    };

    let submit = Action::new_local(move |(id, body): &(Option<Uuid>, LibraryWrite)| {
        let (id, body) = (*id, body.clone());
        async move { save(id, body).await }
    });
    let remove = Action::new_local(move |id: &Uuid| {
        let path = format!("{LIBRARIES_PATH}/{id}");
        async move { api::delete(&path).await }
    });
    Effect::new(move |_| match submit.value().get() {
        Some(Ok(_)) => on_done.run(()),
        Some(Err(error)) => feedback.set(Some(error.message())),
        None => {}
    });
    Effect::new(move |_| match remove.value().get() {
        Some(Ok(())) => on_done.run(()),
        Some(Err(error)) => feedback.set(Some(error.message())),
        None => {}
    });

    let add_worker = move |_| {
        let Ok(worker_id) = new_worker.get_untracked().parse::<Uuid>() else {
            feedback.set(Some("Choisissez un worker.".into()));
            return;
        };
        let mut result = Ok(());
        draft.update(|value| {
            result = value.add_worker(
                worker_id,
                &new_path.get_untracked(),
                new_access.get_untracked(),
            )
        });
        match result {
            Ok(()) => {
                new_path.set(String::new());
                feedback.set(None);
            }
            Err(message) => feedback.set(Some(message.into())),
        }
    };
    let on_submit = move |event: leptos::ev::SubmitEvent| {
        event.prevent_default();
        let current = draft.get_untracked();
        match current.to_write() {
            Ok(body) => {
                submit.dispatch((current.id, body));
            }
            Err(message) => feedback.set(Some(message.into())),
        }
    };
    let existing_id = draft.get_untracked().id;

    view! {
        <form class="inspection-panel" on:submit=on_submit>
            <div>
                <div class="form-group">
                    <label for="library-name" class="form-label">"Nom"</label>
                    <input id="library-name" class="form-input" maxlength="80"
                        prop:value=move || draft.get().name
                        on:input=move |event| draft.update(|value| value.name = event_target_value(&event)) />
                </div>
                <div class="form-group">
                    <label for="library-flow" class="form-label">"Flux"</label>
                    <select id="library-flow" class="form-select"
                        prop:value=move || draft.get().flow_id.unwrap_or_default()
                        on:change=move |event| draft.update(|value| {
                            let id = event_target_value(&event);
                            value.flow_id = (!id.is_empty()).then_some(id);
                        })>
                        <option value="">"Aucun (pas de traitement automatique)"</option>
                        {flows.into_iter().map(|flow| view! {
                            <option value=flow.id.clone()>{format!("{} (v{})", flow.name, flow.version)}</option>
                        }).collect_view()}
                    </select>
                </div>
            </div>

            <p class="inspection-panel__title">"Workers"</p>
            <ul class="library-worker-list">
                {move || {
                    let entries = draft.get().workers;
                    if entries.is_empty() {
                        return view! { <li class="text-dim">"Aucun worker : la bibliothèque n'est pas traitée."</li> }.into_any();
                    }
                    entries.into_iter().map(|entry| {
                        let id = entry.worker_id;
                        view! {
                            <li class="library-worker-row">
                                <span class="text-strong">{worker_name(id)}</span>
                                <span class="library-worker-path">{entry.local_path}</span>
                                <span class="status-pill info">{access_label(entry.access)}</span>
                                <button type="button" class="btn btn-secondary btn-sm"
                                    on:click=move |_| draft.update(|value| value.remove_worker(id))>"Retirer"</button>
                            </li>
                        }
                    }).collect_view().into_any()
                }}
            </ul>
            <div>
                <div class="form-group">
                    <label for="library-worker" class="form-label">"Worker"</label>
                    <select id="library-worker" class="form-select"
                        on:change=move |event| new_worker.set(event_target_value(&event))>
                        <option value="">"Choisir un worker"</option>
                        {workers.into_iter().map(|worker| view! {
                            <option value=worker.id.to_string()>{worker.name}</option>
                        }).collect_view()}
                    </select>
                </div>
                <div class="form-group">
                    <label for="library-path" class="form-label">"Chemin sur ce worker"</label>
                    <input id="library-path" class="form-input" placeholder="/mnt/medias/films"
                        prop:value=move || new_path.get()
                        on:input=move |event| new_path.set(event_target_value(&event)) />
                    <p class="form-help">"Chemin absolu tel que ce worker le voit."</p>
                </div>
                <div class="form-group">
                    <label for="library-access" class="form-label">"Accès"</label>
                    <select id="library-access" class="form-select"
                        on:change=move |event| new_access.set(if event_target_value(&event) == "read" {
                            LibraryAccess::Read
                        } else {
                            LibraryAccess::ReadWrite
                        })>
                        <option value="read_write">{access_label(LibraryAccess::ReadWrite)}</option>
                        <option value="read">{access_label(LibraryAccess::Read)}</option>
                    </select>
                </div>
                <div class="form-group">
                    <button type="button" class="btn btn-secondary" on:click=add_worker>"Ajouter le worker"</button>
                </div>
            </div>

            <p class="form-help" role="status">{move || feedback.get()}</p>
            <div class="inspection-drawer__actions">
                <button type="submit" class="btn btn-primary" disabled=move || submit.pending().get()>
                    "Enregistrer"
                </button>
                <button type="button" class="btn btn-secondary" on:click=move |_| on_done.run(())>"Annuler"</button>
                {existing_id.map(|id| view! {
                    <button type="button" class="btn btn-danger-quiet" disabled=move || remove.pending().get()
                        on:click=move |_| { remove.dispatch(id); }>
                        "Supprimer la bibliothèque"
                    </button>
                })}
            </div>
        </form>
    }
}
