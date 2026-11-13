//! Tableau des tâches filtrées, avec la progression reçue en direct.

use super::model::{status_label, status_tone};
use crate::format;
use flux_shared::jobs::{Job, JobStatus};
use leptos::prelude::*;
use std::collections::HashMap;
use uuid::Uuid;

#[component]
pub fn JobsTable(
    jobs: Signal<Vec<Job>>,
    progress: RwSignal<HashMap<Uuid, u32>>,
    selected: RwSignal<Option<Uuid>>,
) -> impl IntoView {
    let progress_label = move |job: &Job| match job.status {
        JobStatus::Running => progress
            .get()
            .get(&job.id)
            .map_or_else(|| "En cours".to_owned(), |pct| format!("{pct} %")),
        _ => String::new(),
    };
    view! {
        <div class="table-wrapper">
            <table class="data-table" aria-label="Tâches">
                <thead>
                    <tr>
                        <th scope="col">"Fichier"</th>
                        <th scope="col">"État"</th>
                        <th scope="col">"Progression"</th>
                        <th scope="col">"Créée"</th>
                        <th scope="col" class="cell-end">"Action"</th>
                    </tr>
                </thead>
                <tbody>
                    {move || {
                        let list = jobs.get();
                        if list.is_empty() {
                            return view! {
                                <tr><td colspan="5" class="text-dim">"Aucune tâche pour ce filtre."</td></tr>
                            }.into_any();
                        }
                        list.into_iter().map(|job| {
                            let id = job.id;
                            let label = progress_label(&job);
                            view! {
                                <tr class:selected=move || selected.get() == Some(id)>
                                    <td class="text-strong truncate">{job.source_name.clone()}</td>
                                    <td>
                                        <span class=format!("status-pill {}", status_tone(job.status))>
                                            {status_label(job.status)}
                                        </span>
                                    </td>
                                    <td class="text-mono">{label}</td>
                                    <td class="text-dim">{format::datetime(job.created_at)}</td>
                                    <td class="cell-end">
                                        <button type="button" class="btn btn-secondary btn-sm"
                                            on:click=move |_| selected.set(Some(id))>
                                            "Ouvrir"
                                        </button>
                                    </td>
                                </tr>
                            }
                        }).collect_view().into_any()
                    }}
                </tbody>
            </table>
        </div>
    }
}
