//! Widget à vues multiples des tâches dans la vue d'ensemble : en cours, terminées, en attente.

use crate::format;
use crate::jobs::model::{status_label, status_tone};
use crate::jobs::table::job_progress_cell;
use crate::route::{Link, Route, use_router};
use crate::ui::{Registre, RegistreMessage, SectionHeader, StatusBadge};
use flux_shared::jobs::{Job, JobStatus};
use flux_shared::workers::Worker;
use leptos::prelude::*;
use std::collections::HashMap;
use uuid::Uuid;

const WIDGET_ROWS_LIMIT: usize = 6;

/// Vues disponibles dans le widget des tâches de la vue d'ensemble.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum JobsWidgetTab {
    #[default]
    Running,
    Completed,
    Queued,
}

impl JobsWidgetTab {
    pub const ALL: [Self; 3] = [Self::Running, Self::Completed, Self::Queued];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Running => "En cours",
            Self::Completed => "Terminées",
            Self::Queued => "En attente",
        }
    }

    pub const fn empty_message(self) -> &'static str {
        match self {
            Self::Running => "Aucune tâche en cours.",
            Self::Completed => "Aucune tâche terminée.",
            Self::Queued => "Aucune tâche en attente.",
        }
    }

    pub fn matches(self, status: JobStatus) -> bool {
        match self {
            Self::Running => status == JobStatus::Running,
            Self::Completed => matches!(status, JobStatus::Succeeded | JobStatus::Failed),
            Self::Queued => status == JobStatus::Queued,
        }
    }
}

/// Compteurs `(en_cours, terminées, en_attente)` affichés sur les onglets du widget.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct JobsTabCounts {
    pub running: usize,
    pub completed: usize,
    pub queued: usize,
}

impl JobsTabCounts {
    pub fn for_tab(self, tab: JobsWidgetTab) -> usize {
        match tab {
            JobsWidgetTab::Running => self.running,
            JobsWidgetTab::Completed => self.completed,
            JobsWidgetTab::Queued => self.queued,
        }
    }
}

pub fn count_jobs_by_tab(jobs: &[Job]) -> JobsTabCounts {
    let mut counts = JobsTabCounts::default();
    for job in jobs {
        match job.status {
            JobStatus::Running => counts.running += 1,
            JobStatus::Succeeded | JobStatus::Failed => counts.completed += 1,
            JobStatus::Queued => counts.queued += 1,
        }
    }
    counts
}

pub fn filter_jobs_for_tab(jobs: &[Job], tab: JobsWidgetTab, limit: usize) -> Vec<Job> {
    jobs.iter()
        .filter(|job| tab.matches(job.status))
        .take(limit)
        .cloned()
        .collect()
}

fn worker_label(workers: &[Worker], worker_id: Option<Uuid>) -> String {
    let Some(id) = worker_id else {
        return "—".to_owned();
    };
    workers
        .iter()
        .find(|worker| worker.id == id)
        .map(|worker| worker.name.clone())
        .unwrap_or_else(|| "—".to_owned())
}

#[component]
pub fn JobsWidget(
    jobs: Vec<Job>,
    workers: Vec<Worker>,
    live_progress: Option<RwSignal<HashMap<Uuid, u32>>>,
) -> impl IntoView {
    let router = use_router();
    let active_tab = RwSignal::new(JobsWidgetTab::Running);
    let counts = count_jobs_by_tab(&jobs);
    let actions = view! {
        <Link to=Route::Jobs class="btn btn-secondary btn-sm">"Toutes les tâches →"</Link>
    }
    .into_any();

    view! {
        <section class="section-panel">
            <SectionHeader title="Tâches" actions=actions />
            <nav class="page-subnav" role="tablist" aria-label="Vues des tâches">
                {JobsWidgetTab::ALL.into_iter().map(|tab| {
                    let is_active = move || active_tab.get() == tab;
                    let count = counts.for_tab(tab);
                    view! {
                        <button
                            type="button"
                            role="tab"
                            class="page-subnav__tab"
                            class:is-active=is_active
                            aria-selected=move || is_active().to_string()
                            on:click=move |_| active_tab.set(tab)
                        >
                            {format!("{} ({count})", tab.label())}
                        </button>
                    }
                }).collect_view()}
            </nav>
            {move || {
                let tab = active_tab.get();
                let rows = filter_jobs_for_tab(&jobs, tab, WIDGET_ROWS_LIMIT);
                let workers = workers.clone();
                match tab {
                    JobsWidgetTab::Running => view! {
                        <Registre aria_label="Tâches en cours" headers=vec!["Fichier", "Worker", "Progression", "Mise à jour"]>
                            {if rows.is_empty() {
                                view! { <RegistreMessage span=4 text=tab.empty_message() /> }.into_any()
                            } else {
                                rows.into_iter().map(|job| {
                                    let id = job.id;
                                    let status = job.status;
                                    let worker = worker_label(&workers, job.worker_id);
                                    view! {
                                        <tr class="registre-row is-clickable" on:click=move |_| router.go(Route::Jobs)>
                                            <td class="text-strong truncate">{job.source_name}</td>
                                            <td class="text-dim truncate">{worker}</td>
                                            <td>{job_progress_cell(id, status, live_progress)}</td>
                                            <td class="text-dim">{format::datetime(job.updated_at)}</td>
                                        </tr>
                                    }
                                }).collect_view().into_any()
                            }}
                        </Registre>
                    }.into_any(),
                    JobsWidgetTab::Completed => view! {
                        <Registre aria_label="Tâches terminées" headers=vec!["Fichier", "État", "Worker", "Terminée"]>
                            {if rows.is_empty() {
                                view! { <RegistreMessage span=4 text=tab.empty_message() /> }.into_any()
                            } else {
                                rows.into_iter().map(|job| {
                                    let status = job.status;
                                    let worker = worker_label(&workers, job.worker_id);
                                    view! {
                                        <tr class="registre-row is-clickable" on:click=move |_| router.go(Route::Jobs)>
                                            <td class="text-strong truncate">{job.source_name}</td>
                                            <td><StatusBadge label=status_label(status) variant=status_tone(status) /></td>
                                            <td class="text-dim truncate">{worker}</td>
                                            <td class="text-dim">{format::datetime(job.updated_at)}</td>
                                        </tr>
                                    }
                                }).collect_view().into_any()
                            }}
                        </Registre>
                    }.into_any(),
                    JobsWidgetTab::Queued => view! {
                        <Registre aria_label="Tâches en attente" headers=vec!["Fichier", "État", "Tentatives", "Créée"]>
                            {if rows.is_empty() {
                                view! { <RegistreMessage span=4 text=tab.empty_message() /> }.into_any()
                            } else {
                                rows.into_iter().map(|job| {
                                    let status = job.status;
                                    view! {
                                        <tr class="registre-row is-clickable" on:click=move |_| router.go(Route::Jobs)>
                                            <td class="text-strong truncate">{job.source_name}</td>
                                            <td><StatusBadge label=status_label(status) variant=status_tone(status) /></td>
                                            <td class="text-dim">{job.attempts}</td>
                                            <td class="text-dim">{format::datetime(job.created_at)}</td>
                                        </tr>
                                    }
                                }).collect_view().into_any()
                            }}
                        </Registre>
                    }.into_any(),
                }
            }}
        </section>
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn job(name: &str, status: JobStatus) -> Job {
        let now = Utc::now();
        Job {
            id: Uuid::nil(),
            source_name: name.to_owned(),
            source_sha256: None,
            source_bytes: None,
            output_sha256: None,
            output_bytes: None,
            warning: false,
            target_worker_id: None,
            worker_id: None,
            status,
            attempts: 0,
            lease_epoch: 0,
            lease_token: None,
            leased_until: None,
            output_name: None,
            error_code: None,
            library_id: None,
            flow_id: None,
            flow_version: None,
            created_at: now,
            updated_at: now,
        }
    }

    #[test]
    fn count_and_filter_jobs_partition_the_three_views() {
        let jobs = vec![
            job("a.mkv", JobStatus::Running),
            job("b.mkv", JobStatus::Succeeded),
            job("c.mkv", JobStatus::Failed),
            job("d.mkv", JobStatus::Queued),
            job("e.mkv", JobStatus::Queued),
        ];
        assert_eq!(
            count_jobs_by_tab(&jobs),
            JobsTabCounts {
                running: 1,
                completed: 2,
                queued: 2,
            }
        );
        assert_eq!(
            filter_jobs_for_tab(&jobs, JobsWidgetTab::Running, 6).len(),
            1
        );
        assert_eq!(
            filter_jobs_for_tab(&jobs, JobsWidgetTab::Completed, 6).len(),
            2
        );
        assert_eq!(
            filter_jobs_for_tab(&jobs, JobsWidgetTab::Queued, 1).len(),
            1
        );
    }
}
