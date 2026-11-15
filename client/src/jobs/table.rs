//! Registre des tâches filtrées, avec la progression reçue en direct.

use super::model::{status_label, status_tone};
use crate::format;
use crate::ui::{ProgressBar, Registre, RegistreMessage, StatusBadge};
use flux_shared::jobs::{Job, JobStatus};
use leptos::prelude::*;
use std::collections::HashMap;
use uuid::Uuid;

const HEADERS: [&str; 5] = ["Fichier", "État", "Progression", "Créée", "Action"];

#[component]
pub fn JobsTable(
    jobs: Signal<Option<Result<Vec<Job>, String>>>,
    progress: RwSignal<HashMap<Uuid, u32>>,
    selected: RwSignal<Option<Uuid>>,
    on_open: Callback<Uuid>,
) -> impl IntoView {
    view! {
        <Registre aria_label="Tâches" headers=HEADERS.to_vec()>
            {move || match jobs.get() {
                None => view! { <RegistreMessage span=HEADERS.len() text="Chargement des tâches…" /> }.into_any(),
                Some(Err(message)) => view! { <RegistreMessage span=HEADERS.len() text=message /> }.into_any(),
                Some(Ok(list)) if list.is_empty() => view! { <RegistreMessage span=HEADERS.len() text="Aucune tâche dans cette vue." /> }.into_any(),
                Some(Ok(list)) => list.into_iter().map(|job| {
                    let id = job.id;
                    let status = job.status;
                    let progress_cell = move || match status {
                        JobStatus::Running => match progress.get().get(&id).copied() {
                            Some(pct) => {
                                view! { <ProgressBar percent=pct label=format!("{pct} %") /> }.into_any()
                            }
                            None => view! { <ProgressBar label="En cours" /> }.into_any(),
                        },
                        JobStatus::Succeeded => {
                            view! { <span class="text-mono text-xs text-dim">"100 %"</span> }.into_any()
                        }
                        _ => view! { <span class="text-dim">"—"</span> }.into_any(),
                    };
                    view! {
                        <tr class="registre-row" class:is-open=move || selected.get() == Some(id)>
                            <td class="text-strong truncate">{job.source_name.clone()}</td>
                            <td><StatusBadge label=status_label(job.status) variant=status_tone(job.status) /></td>
                            <td>{progress_cell}</td>
                            <td class="text-dim">{format::datetime(job.created_at)}</td>
                            <td class="cell-end">
                                <button type="button" class="btn btn-secondary btn-sm registre-open-btn" on:click=move |_| on_open.run(id)>"Ouvrir"</button>
                            </td>
                        </tr>
                    }
                }).collect_view().into_any(),
            }}
        </Registre>
    }
}
