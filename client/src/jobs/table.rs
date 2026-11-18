//! Registre des tâches filtrées, avec la progression reçue en direct.

use super::model::{error_label, priority_label, resolve_library_label, status_label, status_tone};
use crate::format;
use crate::ui::{ProgressBar, Registre, RegistreMessage, StatusBadge, Tag, TagTone, on_activate};
use flux_shared::jobs::{Job, JobStatus};
use leptos::prelude::*;
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

const HEADERS: [&str; 7] = [
    "",
    "Fichier",
    "Flux",
    "Worker",
    "État",
    "Progression",
    "Créée",
];

fn resolve_flow_label(flow_id: Option<&str>, flows: &HashMap<String, String>) -> Option<String> {
    let id = flow_id?;
    Some(flows.get(id).cloned().unwrap_or_else(|| id.to_owned()))
}

fn resolve_worker_label(
    worker_id: Option<Uuid>,
    workers: &HashMap<Uuid, String>,
) -> Option<String> {
    let id = worker_id?;
    Some(
        workers
            .get(&id)
            .cloned()
            .unwrap_or_else(|| id.to_string().chars().take(8).collect()),
    )
}

/// Cellule de progression partagée entre l'écran Activité et les tâches récentes de la Vue d'ensemble.
pub fn job_progress_cell(
    id: Uuid,
    status: JobStatus,
    progress: Option<RwSignal<HashMap<Uuid, u32>>>,
) -> AnyView {
    (move || match status {
        JobStatus::Running => match progress.and_then(|map| map.get().get(&id).copied()) {
            Some(pct) => view! { <ProgressBar percent=pct label=format!("{pct} %") /> }.into_any(),
            None => view! { <ProgressBar label="En cours" /> }.into_any(),
        },
        JobStatus::Succeeded => {
            view! { <span class="text-mono text-xs text-dim">"100 %"</span> }.into_any()
        }
        _ => view! { <span class="text-dim">"—"</span> }.into_any(),
    })
    .into_any()
}

#[component]
pub fn JobsTable(
    jobs: Signal<Option<Result<Vec<Job>, String>>>,
    progress: RwSignal<HashMap<Uuid, u32>>,
    workers: Signal<HashMap<Uuid, String>>,
    flows: Signal<HashMap<String, String>>,
    libraries: Signal<HashMap<Uuid, String>>,
    selected: RwSignal<Option<Uuid>>,
    checked_ids: RwSignal<HashSet<Uuid>>,
    can_manage: bool,
    on_open: Callback<Uuid>,
) -> impl IntoView {
    let headers: Vec<&'static str> = if can_manage {
        HEADERS.to_vec()
    } else {
        HEADERS[1..].to_vec()
    };
    let span = headers.len();
    view! {
        <Registre aria_label="Tâches" headers=headers>
            {move || match jobs.get() {
                None => view! { <RegistreMessage span=span text="Chargement des tâches…" /> }.into_any(),
                Some(Err(message)) => view! { <RegistreMessage span=span text=message /> }.into_any(),
                Some(Ok(list)) if list.is_empty() => view! { <RegistreMessage span=span text="Aucune tâche dans cette vue." /> }.into_any(),
                Some(Ok(list)) => {
                    let workers_map = workers.get();
                    let flows_map = flows.get();
                    let libs_map = libraries.get();
                    list.into_iter().map(|job| {
                        let id = job.id;
                        let status = job.status;
                        let priority = job.priority;
                        let flow_label = resolve_flow_label(job.flow_id.as_deref(), &flows_map);
                        let worker_label = resolve_worker_label(job.worker_id, &workers_map);
                        let library_label = resolve_library_label(job.library_id, &libs_map);
                        view! {
                            <tr class="registre-row is-clickable" class:is-open=move || selected.get() == Some(id)
                                tabindex="0"
                                on:click=move |_| on_open.run(id)
                                on:keydown=on_activate(move || on_open.run(id))>
                                {can_manage.then(|| view! {
                                    <td on:click=move |ev| ev.stop_propagation()>
                                        <input
                                            type="checkbox"
                                            aria-label="Sélectionner la tâche"
                                            prop:checked=move || checked_ids.with(|set| set.contains(&id))
                                            on:change=move |ev| {
                                                let checked = event_target_checked(&ev);
                                                checked_ids.update(|set| {
                                                    if checked {
                                                        set.insert(id);
                                                    } else {
                                                        set.remove(&id);
                                                    }
                                                });
                                            }
                                        />
                                    </td>
                                })}
                                <td class="truncate">
                                    <div class="cluster">
                                        <span class="text-strong">{job.source_name.clone()}</span>
                                        {library_label.map(|name| view! {
                                            <Tag label=name tone=TagTone::Primary />
                                        })}
                                        {(priority != 0).then(|| view! {
                                            <Tag
                                                label=priority_label(priority)
                                                tone={if priority > 0 { TagTone::Warning } else { TagTone::Neutral }}
                                            />
                                        })}
                                    </div>
                                </td>
                                <td>
                                    {match flow_label {
                                        Some(label) => view! { <Tag label=label tone=TagTone::Mono /> }.into_any(),
                                        None => view! { <span class="text-dim">"—"</span> }.into_any(),
                                    }}
                                </td>
                                <td>
                                    {match worker_label {
                                        Some(label) => view! { <Tag label=label tone=TagTone::Neutral /> }.into_any(),
                                        None => view! { <span class="text-dim">"—"</span> }.into_any(),
                                    }}
                                </td>
                                <td>
                                    <div class="cluster">
                                        <StatusBadge label=status_label(job.status) variant=status_tone(job.status) />
                                        {job.warning.then(|| view! {
                                            <Tag label="Avertissement" tone=TagTone::Warning />
                                        })}
                                        {job.error_code.map(|code| view! {
                                            <Tag label=error_label(code) tone=TagTone::Danger />
                                        })}
                                    </div>
                                </td>
                                <td>{job_progress_cell(id, status, Some(progress))}</td>
                                <td class="text-dim">{format::datetime(job.created_at)}</td>
                            </tr>
                        }
                    }).collect_view().into_any()
                }
            }}
        </Registre>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_flow_and_worker_labels_fall_back_gracefully() {
        let mut flows = HashMap::new();
        flows.insert("remux".into(), "Remux MKV".into());
        assert_eq!(
            resolve_flow_label(Some("remux"), &flows).as_deref(),
            Some("Remux MKV")
        );
        assert_eq!(
            resolve_flow_label(Some("custom"), &flows).as_deref(),
            Some("custom")
        );
        assert_eq!(resolve_flow_label(None, &flows), None);

        let worker_id = Uuid::from_u128(0x1234_5678_9abc_def0);
        let mut workers = HashMap::new();
        assert_eq!(
            resolve_worker_label(Some(worker_id), &workers).as_deref(),
            Some("00000000")
        );
        workers.insert(worker_id, "truenas-worker".into());
        assert_eq!(
            resolve_worker_label(Some(worker_id), &workers).as_deref(),
            Some("truenas-worker")
        );
        assert_eq!(resolve_worker_label(None, &workers), None);
    }
}
