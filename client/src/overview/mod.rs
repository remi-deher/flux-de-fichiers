//! Vue d'ensemble : chiffres clés, points d'attention et tâches récentes ; sur un worker, ses tâches.

mod summary;

use crate::jobs::model::{status_label, status_tone};
use crate::route::{Link, Route};
use crate::{api, format};
use chrono::Utc;
use flux_shared::jobs::Job;
use flux_shared::libraries::Library;
use flux_shared::session::Me;
use flux_shared::workers::{Worker, WorkerOverview};
use leptos::prelude::*;
use summary::{Summary, alerts};

const JOBS_PATH: &str = "/api/v1/jobs?limit=200";
const WORKERS_PATH: &str = "/api/v1/workers";
const LIBRARIES_PATH: &str = "/api/v1/libraries";
const WORKER_OVERVIEW_PATH: &str = "/api/v1/worker/overview";
const RECENT_JOBS: usize = 5;

#[component]
fn Metric(label: &'static str, value: String, subtitle: String) -> impl IntoView {
    view! {
        <div class="metric-card">
            <div class="metric-label">{label}</div>
            <div class="metric-value">{value}</div>
            <div class="metric-subtitle">{subtitle}</div>
        </div>
    }
}

fn plain_http() -> bool {
    window()
        .location()
        .protocol()
        .is_ok_and(|protocol| protocol == "http:")
}

#[component]
pub fn CentralOverview(me: Me) -> impl IntoView {
    let jobs = LocalResource::new(|| api::get::<Vec<Job>>(JOBS_PATH));
    let workers = LocalResource::new(|| api::get::<Vec<Worker>>(WORKERS_PATH));
    let libraries = LocalResource::new(|| api::get::<Vec<Library>>(LIBRARIES_PATH));
    let data = move || {
        let jobs = jobs.get().and_then(Result::ok).unwrap_or_default();
        let workers = workers.get().and_then(Result::ok).unwrap_or_default();
        let libraries = libraries.get().and_then(Result::ok).unwrap_or_default();
        (jobs, workers, libraries)
    };
    view! {
        <div class="registre">
            <h1 class="section-title">{format!("Vue d'ensemble — {}", me.instance_name.clone().unwrap_or_default())}</h1>
            {move || {
                let (jobs, workers, libraries) = data();
                let now = Utc::now();
                let summary = Summary::new(&jobs, &workers, &libraries, now);
                let attention = alerts(&summary, &workers, now, plain_http());
                let recent: Vec<Job> = jobs.into_iter().take(RECENT_JOBS).collect();
                view! {
                    <section class="metrics-grid" aria-label="Chiffres clés">
                        <Metric label="Tâches en cours" value=summary.running.to_string() subtitle=format!("{} en attente", summary.queued) />
                        <Metric label="Tâches échouées" value=summary.failed.to_string() subtitle=format!("{} relançable(s)", summary.retryable) />
                        <Metric label="Workers en ligne" value=format!("{} / {}", summary.workers_online, summary.workers_total) subtitle="Central et workers".into() />
                        <Metric label="Bibliothèques prêtes" value=format!("{} / {}", summary.libraries_ready, summary.libraries_total) subtitle="Avec flux et worker".into() />
                    </section>
                    <section class="section-panel" aria-labelledby="attention-title">
                        <h2 id="attention-title" class="section-title">"À surveiller"</h2>
                        {if attention.is_empty() {
                            view! { <p class="text-dim">"Rien à signaler."</p> }.into_any()
                        } else {
                            view! { <ul>{attention.into_iter().map(|text| view! { <li class="settings-alert">{text}</li> }).collect_view()}</ul> }.into_any()
                        }}
                    </section>
                    <section class="section-panel" aria-labelledby="recent-title">
                        <div class="cluster-spread">
                            <h2 id="recent-title" class="section-title">"Tâches récentes"</h2>
                            <Link to=Route::Jobs class="btn btn-secondary btn-sm">"Toutes les tâches"</Link>
                        </div>
                        <ul>
                            {recent.into_iter().map(|job| view! {
                                <li class="cluster-spread">
                                    <span class="text-strong truncate">{job.source_name}</span>
                                    <span class=format!("status-pill {}", status_tone(job.status))>{status_label(job.status)}</span>
                                    <span class="text-dim">{format::datetime(job.created_at)}</span>
                                </li>
                            }).collect_view()}
                        </ul>
                    </section>
                }
            }}
        </div>
    }
}

#[component]
pub fn WorkerOverviewPage(me: Me) -> impl IntoView {
    let overview = LocalResource::new(|| api::get::<WorkerOverview>(WORKER_OVERVIEW_PATH));
    view! {
        <div class="registre">
            <h1 class="section-title">{format!("Worker « {} »", me.instance_name.clone().unwrap_or_default())}</h1>
            {move || match overview.get() {
                None => view! { <p class="text-dim">"Chargement…"</p> }.into_any(),
                Some(Err(error)) => view! { <p class="settings-alert" role="alert">{format!("Central injoignable : {}", error.message())}</p> }.into_any(),
                Some(Ok(view)) => {
                    let contact = view.last_seen.map_or_else(|| "aucune activité enregistrée".to_owned(), |at| format!("dernière activité {}", format::datetime(at)));
                    view! {
                        <p class="status-pill success">{format!("Central joignable • {contact}")}</p>
                        <section class="metrics-grid" aria-label="Tâches de ce worker">
                            <Metric label="En attente" value=view.jobs.queued.to_string() subtitle="Tâches en file pour ce worker".into() />
                            <Metric label="En cours" value=view.jobs.running.to_string() subtitle="Traitements actifs".into() />
                            <Metric label="Réussies" value=view.jobs.succeeded.to_string() subtitle="Menées à terme".into() />
                            <Metric label="En échec" value=view.jobs.failed.to_string() subtitle="Échouées sur ce worker".into() />
                        </section>
                    }.into_any()
                }
            }}
        </div>
    }
}
