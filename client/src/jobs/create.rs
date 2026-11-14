//! Création d'une tâche à partir d'un fichier du dossier d'entrée.

use crate::api;
use flux_shared::format::format_bytes;
use flux_shared::jobs::{Job, MediaFile, NewJob};
use flux_shared::workers::Worker;
use leptos::prelude::*;
use uuid::Uuid;

const FILES_PATH: &str = "/api/v1/files";
const JOBS_PATH: &str = "/api/v1/jobs";
const WORKERS_PATH: &str = "/api/v1/workers";

/// Worker choisi dans la liste ; vide (ou illisible) : le premier worker disponible.
fn parse_target(value: &str) -> Option<Uuid> {
    Uuid::parse_str(value).ok()
}

#[component]
pub fn CreateJob(on_created: Callback<Job>) -> impl IntoView {
    let files = LocalResource::new(|| api::get::<Vec<MediaFile>>(FILES_PATH));
    let workers = LocalResource::new(|| api::get::<Vec<Worker>>(WORKERS_PATH));
    let selected = RwSignal::new(String::new());
    let target = RwSignal::new(String::new());
    let feedback = RwSignal::new(None::<String>);
    let create = Action::new_local(move |source_name: &String| {
        let request = NewJob {
            source_name: source_name.clone(),
            target_worker_id: parse_target(&target.get_untracked()),
        };
        async move { api::post::<NewJob, Job>(JOBS_PATH, Some(&request)).await }
    });

    Effect::new(move |_| {
        if let Some(result) = create.value().get() {
            match result {
                Ok(job) => {
                    feedback.set(Some(format!("Tâche créée pour « {} ».", job.source_name)));
                    on_created.run(job);
                }
                Err(error) => feedback.set(Some(error.message())),
            }
        }
    });

    let options = move || match files.get() {
        None => vec![view! { <option value="">"Chargement des fichiers…"</option> }.into_any()],
        Some(Err(error)) => vec![view! { <option value="">{error.message()}</option> }.into_any()],
        Some(Ok(list)) if list.is_empty() => {
            vec![
                view! { <option value="">"Aucun fichier dans le dossier d'entrée"</option> }
                    .into_any(),
            ]
        }
        Some(Ok(list)) => {
            std::iter::once(view! { <option value="">"Choisir un fichier"</option> }.into_any())
                .chain(list.into_iter().map(|file| {
                    let label = format!("{} ({})", file.name, format_bytes(file.bytes));
                    view! { <option value=file.name>{label}</option> }.into_any()
                }))
                .collect()
        }
    };

    view! {
        <div class="inspection-panel">
            <form
                on:submit=move |event| {
                    event.prevent_default();
                    let name = selected.get_untracked();
                    if name.is_empty() {
                        feedback.set(Some("Choisissez un fichier.".into()));
                    } else {
                        create.dispatch(name);
                    }
                }
            >
                <div class="form-group">
                    <label for="create-job-file" class="form-label">"Fichier source"</label>
                    <select
                        id="create-job-file"
                        class="form-select"
                        on:change=move |event| selected.set(event_target_value(&event))
                        prop:value=move || selected.get()
                    >
                        {options}
                    </select>
                    <p class="form-help">
                        "La tâche suit le flux par défaut, figé à sa création."
                    </p>
                </div>
                <div class="form-group">
                    <label for="create-job-worker" class="form-label">"Worker"</label>
                    <select
                        id="create-job-worker"
                        class="form-select"
                        on:change=move |event| target.set(event_target_value(&event))
                        prop:value=move || target.get()
                    >
                        <option value="">"Premier worker disponible"</option>
                        {move || workers.get().and_then(Result::ok).unwrap_or_default().into_iter().map(|worker| {
                            view! { <option value=worker.id.to_string()>{worker.name}</option> }
                        }).collect_view()}
                    </select>
                </div>
                <div class="form-group">
                    <button type="submit" class="btn btn-primary" disabled=move || create.pending().get()>
                        "Créer la tâche"
                    </button>
                    <p class="form-help" role="status">{move || feedback.get()}</p>
                </div>
            </form>
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::parse_target;

    #[test]
    fn an_empty_or_invalid_choice_leaves_the_worker_to_the_central() {
        assert_eq!(parse_target(""), None);
        assert_eq!(parse_target("pas-un-uuid"), None);
        let id = "67e55044-10b1-426f-9247-bb680e5fe0c8";
        assert_eq!(
            parse_target(id).map(|uuid| uuid.to_string()).as_deref(),
            Some(id)
        );
    }
}
