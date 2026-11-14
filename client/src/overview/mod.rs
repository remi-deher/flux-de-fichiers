//! Vue d'ensemble : chiffres clés, points d'attention et tâches récentes ; sur un worker, ses tâches.

mod summary;

use crate::jobs::model::{status_label, status_tone};
use crate::route::{Link, Route};
use crate::ui::{
    EmptyState, IconName, MetricCard, MetricTone, Registre, RegistreMessage, SectionHeader,
    StatusBadge,
};
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

fn plain_http() -> bool {
    window()
        .location()
        .protocol()
        .is_ok_and(|protocol| protocol == "http:")
}

/// Carte d'accueil : état général, titre et raccourcis.
#[component]
fn Hero(
    tone: &'static str,
    status: String,
    title: String,
    subtitle: &'static str,
    #[prop(optional)] actions: Option<AnyView>,
) -> impl IntoView {
    view! {
        <div class="overview-hero-card">
            <div class="hero-content">
                <div class="hero-badge" data-tone=tone>
                    <span class="status-dot"></span>
                    <span>{status}</span>
                </div>
                <h1 class="hero-title">{title}</h1>
                <p class="hero-subtitle">{subtitle}</p>
            </div>
            <div class="hero-actions">{actions}</div>
        </div>
    }
}

#[component]
pub fn CentralOverview(me: Me) -> impl IntoView {
    let jobs = LocalResource::new(|| api::get::<Vec<Job>>(JOBS_PATH));
    let workers = LocalResource::new(|| api::get::<Vec<Worker>>(WORKERS_PATH));
    let libraries = LocalResource::new(|| api::get::<Vec<Library>>(LIBRARIES_PATH));
    let instance = me.instance_name.clone().unwrap_or_default();
    view! {
        <div class="tab-panel" role="tabpanel">
            {move || {
                let jobs_list = jobs.get().and_then(Result::ok).unwrap_or_default();
                let workers_list = workers.get().and_then(Result::ok).unwrap_or_default();
                let libraries_list = libraries.get().and_then(Result::ok).unwrap_or_default();
                let now = Utc::now();
                let summary = Summary::new(&jobs_list, &workers_list, &libraries_list, now);
                let attention = alerts(&summary, &workers_list, now, plain_http());
                let (tone, status) = if attention.is_empty() {
                    ("success", "Tout fonctionne".to_owned())
                } else {
                    ("warning", format!("{} point(s) à surveiller", attention.len()))
                };
                let recent: Vec<Job> = jobs_list.into_iter().take(RECENT_JOBS).collect();
                let actions = view! {
                    <Link to=Route::Libraries class="btn btn-primary">"Parcourir les bibliothèques"</Link>
                    <Link to=Route::Flows class="btn btn-secondary">"Concepteur de flux"</Link>
                }.into_any();
                view! {
                    <Hero tone=tone status=status title=format!("Vue d'ensemble — {instance}")
                        subtitle="Tâches, workers et bibliothèques du central, en direct." actions=actions />
                    <section class="metrics-grid" aria-label="Statistiques clés">
                        <MetricCard label="Tâches en cours" value=summary.running.to_string() subtitle=format!("{} en attente", summary.queued) icon=IconName::Activity />
                        <MetricCard label="Tâches échouées" value=summary.failed.to_string() subtitle=format!("{} relançable(s)", summary.retryable) icon=IconName::Alert
                            tone={if summary.failed > 0 { MetricTone::Danger } else { MetricTone::Neutral }} />
                        <MetricCard label="Workers en ligne" value=format!("{} / {}", summary.workers_online, summary.workers_total) subtitle="Central et workers" icon=IconName::Workers
                            tone={if summary.workers_online < summary.workers_total { MetricTone::Warning } else { MetricTone::Success }} />
                        <MetricCard label="Bibliothèques prêtes" value=format!("{} / {}", summary.libraries_ready, summary.libraries_total) subtitle="Avec flux et worker" icon=IconName::Libraries />
                    </section>
                    <div class="overview-grid-2col">
                        <section class="section-panel">
                            <SectionHeader title="À surveiller" />
                            {if attention.is_empty() {
                                view! { <EmptyState title="Rien à signaler" message="Aucun point d'attention pour le moment." /> }.into_any()
                            } else {
                                attention.into_iter().map(|text| view! {
                                    <div class="settings-alert"><span class="settings-alert__dot"></span><span class="settings-alert__text">{text}</span></div>
                                }).collect_view().into_any()
                            }}
                        </section>
                        <section class="section-panel">
                            <SectionHeader title="Tâches récentes" actions=view! { <Link to=Route::Jobs class="btn btn-secondary btn-sm">"Toutes les tâches →"</Link> }.into_any() />
                            <Registre aria_label="Tâches récentes" headers=vec!["Fichier", "État", "Créée"]>
                                {if recent.is_empty() {
                                    view! { <RegistreMessage span=3 text="Aucune tâche." /> }.into_any()
                                } else {
                                    recent.into_iter().map(|job| view! {
                                        <tr>
                                            <td class="text-strong truncate">{job.source_name}</td>
                                            <td><StatusBadge label=status_label(job.status) variant=status_tone(job.status) /></td>
                                            <td class="text-dim">{format::datetime(job.created_at)}</td>
                                        </tr>
                                    }).collect_view().into_any()
                                }}
                            </Registre>
                        </section>
                    </div>
                }
            }}
        </div>
    }
}

#[component]
pub fn WorkerOverviewPage(me: Me) -> impl IntoView {
    let overview = LocalResource::new(|| api::get::<WorkerOverview>(WORKER_OVERVIEW_PATH));
    let name = me.instance_name.clone().unwrap_or_default();
    view! {
        <div class="tab-panel" role="tabpanel">
            {move || match overview.get() {
                None => view! { <Hero tone="neutral" status="Vérification du central…".to_owned() title=format!("Worker « {name} »") subtitle="Tâches traitées par ce worker, relayées par le central." /> }.into_any(),
                Some(Err(error)) => view! { <Hero tone="danger" status=format!("Central injoignable : {}", error.message()) title=format!("Worker « {name} »") subtitle="Tâches traitées par ce worker, relayées par le central." /> }.into_any(),
                Some(Ok(view)) => {
                    let contact = view.last_seen.map_or_else(|| "aucune activité enregistrée".to_owned(), |at| format!("dernière activité {}", format::datetime(at)));
                    view! {
                        <Hero tone="success" status=format!("Central joignable • {contact}") title=format!("Worker « {name} »") subtitle="Tâches traitées par ce worker, relayées par le central." />
                        <section class="metrics-grid" aria-label="Tâches de ce worker">
                            <MetricCard label="En attente" value=view.jobs.queued.to_string() subtitle="Tâches en file pour ce worker" icon=IconName::Activity />
                            <MetricCard label="En cours" value=view.jobs.running.to_string() subtitle="Traitements actifs" icon=IconName::Refresh />
                            <MetricCard label="Réussies" value=view.jobs.succeeded.to_string() subtitle="Menées à terme" icon=IconName::Check tone=MetricTone::Success />
                            <MetricCard label="En échec" value=view.jobs.failed.to_string() subtitle="Échouées sur ce worker" icon=IconName::Alert
                                tone={if view.jobs.failed > 0 { MetricTone::Danger } else { MetricTone::Neutral }} />
                        </section>
                    }.into_any()
                }
            }}
        </div>
    }
}
