//! Création d'une tâche à partir d'un fichier du dossier d'entrée.

use crate::api;
use flux_shared::format::format_bytes;
use flux_shared::jobs::{Job, MediaFile, NewJob};
use leptos::prelude::*;

const FILES_PATH: &str = "/api/v1/files";
const JOBS_PATH: &str = "/api/v1/jobs";

#[component]
pub fn CreateJob(on_created: Callback<Job>) -> impl IntoView {
    let files = LocalResource::new(|| api::get::<Vec<MediaFile>>(FILES_PATH));
    let selected = RwSignal::new(String::new());
    let feedback = RwSignal::new(None::<String>);
    let create = Action::new_local(move |source_name: &String| {
        let request = NewJob {
            source_name: source_name.clone(),
            target_worker_id: None,
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
        <section class="section-panel" aria-labelledby="create-job-title">
            <h2 id="create-job-title" class="section-title">"Nouvelle tâche"</h2>
            <form
                class="form-grid-2"
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
                    <button type="submit" class="btn btn-primary" disabled=move || create.pending().get()>
                        "Créer la tâche"
                    </button>
                    <p class="form-help" role="status">{move || feedback.get()}</p>
                </div>
            </form>
        </section>
    }
}
