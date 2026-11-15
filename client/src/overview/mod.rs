//! Vue d'ensemble : chiffres clés, points d'attention et tâches récentes ; sur un worker, ses tâches.

mod hardware;
mod summary;
mod widgets;

use crate::jobs::LiveJobs;
use crate::jobs::model::{status_label, status_tone};
use crate::jobs::table::job_progress_cell;
use crate::route::{Link, Route, use_router};
use crate::ui::{
    EmptyState, Icon, IconName, MetricCard, MetricTone, Registre, RegistreMessage, SectionHeader,
    StatusBadge,
};
use crate::{api, format};
use chrono::Utc;
use flux_shared::jobs::Job;
use flux_shared::libraries::Library;
use flux_shared::session::Me;
use flux_shared::workers::{Worker, WorkerOverview};
use hardware::HardwareWidget;
use leptos::prelude::*;
use summary::{Summary, alerts, ratio_percent};
use widgets::{Widget, WidgetLayout, WidgetPicker};

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
                    <span class="status-dot status-dot--pulse"></span>
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
    let router = use_router();
    let live_progress = use_context::<LiveJobs>().map(|live| live.progress);
    let jobs = LocalResource::new(|| api::get::<Vec<Job>>(JOBS_PATH));
    let workers = LocalResource::new(|| api::get::<Vec<Worker>>(WORKERS_PATH));
    let libraries = LocalResource::new(|| api::get::<Vec<Library>>(LIBRARIES_PATH));
    let instance = me.instance_name.clone().unwrap_or_default();
    let layout = RwSignal::new(WidgetLayout::restore());
    view! {
        <div class="tab-panel" role="tabpanel">
            {move || {
                let layout_now = layout.get();
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
                    <Link to=Route::NewFlow class="btn btn-primary">
                        <Icon name=IconName::Plus size=15 />
                        <span>"Nouveau flux"</span>
                    </Link>
                    <Link to=Route::Libraries class="btn btn-secondary">
                        <Icon name=IconName::Libraries size=15 />
                        <span>"Bibliothèques"</span>
                    </Link>
                    <WidgetPicker layout=layout />
                }.into_any();
                view! {
                    <Hero tone=tone status=status title=format!("Vue d'ensemble — {instance}")
                        subtitle="Tâches, workers et bibliothèques du central, en direct." actions=actions />
                    {layout_now.is_visible(Widget::Metrics).then(|| view! {
                    <section class="metrics-grid" aria-label="Statistiques clés">
                        <MetricCard label="Tâches en cours" value=summary.running.to_string() subtitle=format!("{} en attente", summary.queued) icon=IconName::Activity />
                        <MetricCard label="Tâches échouées" value=summary.failed.to_string() subtitle=format!("{} relançable(s)", summary.retryable) icon=IconName::Alert
                            tone={if summary.failed > 0 { MetricTone::Danger } else { MetricTone::Neutral }} />
                        <MetricCard label="Workers en ligne" value=format!("{} / {}", summary.workers_online, summary.workers_total) subtitle="Central et workers" icon=IconName::Workers
                            tone={if summary.workers_online < summary.workers_total { MetricTone::Warning } else { MetricTone::Success }}
                            progress=f32::from(ratio_percent(summary.workers_online, summary.workers_total)) />
                        <MetricCard label="Bibliothèques prêtes" value=format!("{} / {}", summary.libraries_ready, summary.libraries_total) subtitle="Avec flux et worker" icon=IconName::Libraries
                            progress=f32::from(ratio_percent(summary.libraries_ready, summary.libraries_total)) />
                    </section>
                    })}
                    {layout_now.is_visible(Widget::Hardware).then(|| view! { <HardwareWidget /> })}
                    <div class="overview-grid-2col">
                        {layout_now.is_visible(Widget::Attention).then(|| view! {
                        <section class="section-panel">
                            <SectionHeader title="À surveiller" />
                            {if attention.is_empty() {
                                view! { <EmptyState title="Aucune alerte" message="Tous les services fonctionnent normalement." icon_name=IconName::Check /> }.into_any()
                            } else {
                                attention.into_iter().map(|text| view! {
                                    <div class="settings-alert">
                                        <span class="settings-alert__icon" aria-hidden="true">
                                            <Icon name=IconName::Alert size=16 />
                                        </span>
                                        <span class="settings-alert__text">{text}</span>
                                    </div>
                                }).collect_view().into_any()
                            }}
                        </section>
                        })}
                        {layout_now.is_visible(Widget::RecentJobs).then(|| view! {
                        <section class="section-panel">
                            <SectionHeader title="Tâches récentes" actions=view! { <Link to=Route::Jobs class="btn btn-secondary btn-sm">"Toutes les tâches →"</Link> }.into_any() />
                            <Registre aria_label="Tâches récentes" headers=vec!["Fichier", "État", "Progression", "Créée"]>
                                {if recent.is_empty() {
                                    view! { <RegistreMessage span=4 text="Aucune tâche." /> }.into_any()
                                } else {
                                    recent.into_iter().map(|job| {
                                        let id = job.id;
                                        let status = job.status;
                                        view! {
                                            <tr class="registre-row is-clickable" on:click=move |_| router.go(Route::Jobs)>
                                                <td class="text-strong truncate">{job.source_name}</td>
                                                <td><StatusBadge label=status_label(status) variant=status_tone(status) /></td>
                                                <td>{job_progress_cell(id, status, live_progress)}</td>
                                                <td class="text-dim">{format::datetime(job.created_at)}</td>
                                            </tr>
                                        }
                                    }).collect_view().into_any()
                                }}
                            </Registre>
                        </section>
                        })}
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
                    let actions = view! {
                        <Link to=Route::Jobs class="btn btn-secondary">
                            <Icon name=IconName::Activity size=15 />
                            <span>"Activité"</span>
                        </Link>
                        <Link to=Route::Plugins class="btn btn-secondary">
                            <Icon name=IconName::Plugins size=15 />
                            <span>"Plugins"</span>
                        </Link>
                    }.into_any();
                    view! {
                        <Hero tone="success" status=format!("Central joignable • {contact}") title=format!("Worker « {name} »") subtitle="Tâches traitées par ce worker, relayées par le central." actions=actions />
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
