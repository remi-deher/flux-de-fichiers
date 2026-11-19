//! Vue d'ensemble : chiffres clés, points d'attention et tâches récentes ; sur un worker, ses tâches.

pub mod hardware;
mod jobs_widget;
mod summary;
mod widgets;

use crate::jobs::LiveJobs;
use crate::route::{Route, use_router};
use crate::ui::filters::{Filters, JobStateFilter};
use crate::ui::{EmptyState, Icon, IconName, MetricTone, SectionHeader, StatItem, StatStrip};
use crate::{api, format};
use chrono::Utc;
use forgearr_shared::format::format_bytes;
use forgearr_shared::jobs::{Job, JobStorageStats};
use forgearr_shared::libraries::Library;
use forgearr_shared::session::Me;
use forgearr_shared::workers::{Worker, WorkerOverview};
use jobs_widget::JobsWidget;
use leptos::prelude::*;
use summary::{Summary, alerts, ratio_percent};
use widgets::{Widget, WidgetLayout, WidgetPicker};

const JOBS_PATH: &str = "/api/v1/jobs?limit=200";
const JOBS_STATS_PATH: &str = "/api/v1/jobs/stats";
const WORKERS_PATH: &str = "/api/v1/workers";
const LIBRARIES_PATH: &str = "/api/v1/libraries";
const WORKER_OVERVIEW_PATH: &str = "/api/v1/worker/overview";

fn plain_http() -> bool {
    window()
        .location()
        .protocol()
        .is_ok_and(|protocol| protocol == "http:")
}

/// Bandeau d'état général compact et actions rapides de la vue d'ensemble.
#[component]
fn Hero(
    tone: &'static str,
    status: String,
    #[prop(optional)] actions: Option<AnyView>,
) -> impl IntoView {
    view! {
        <div class="overview-toolbar">
            <div class="hero-badge" data-tone=tone>
                <span class="status-dot status-dot--pulse"></span>
                <span>{status}</span>
            </div>
            {actions.map(|content| view! {
                <div class="overview-toolbar__actions">{content}</div>
            })}
        </div>
    }
}

fn open_jobs_filtered(
    router: crate::route::Router,
    filters: Option<Filters>,
    state: JobStateFilter,
) {
    if let Some(filters) = filters {
        filters.pending_job_state.set(Some(state));
    }
    router.go(Route::Jobs);
}

#[component]
pub fn CentralOverview(me: Me) -> impl IntoView {
    let _ = me;
    let router = use_router();
    let filters = use_context::<Filters>();
    let live_progress = use_context::<LiveJobs>().map(|live| live.progress);
    let jobs = LocalResource::new(|| api::get::<Vec<Job>>(JOBS_PATH));
    let job_stats = LocalResource::new(|| api::get::<JobStorageStats>(JOBS_STATS_PATH));
    let workers = LocalResource::new(|| api::get::<Vec<Worker>>(WORKERS_PATH));
    let libraries = LocalResource::new(|| api::get::<Vec<Library>>(LIBRARIES_PATH));
    let layout = RwSignal::new(WidgetLayout::restore());
    view! {
        <div class="tab-panel" role="tabpanel">
            {move || {
                let layout_now = layout.get();
                let jobs_list = jobs.get().and_then(Result::ok).unwrap_or_default();
                let storage_stats = job_stats.get().and_then(Result::ok).unwrap_or_default();
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
                let actions = view! { <WidgetPicker layout=layout /> }.into_any();
                view! {
                    <Hero tone=tone status=status actions=actions />
                    {layout_now.is_visible(Widget::Metrics).then(|| {
                        let go_active = Callback::new(move |()| {
                            open_jobs_filtered(router, filters, JobStateFilter::Active)
                        });
                        let go_failed = Callback::new(move |()| {
                            open_jobs_filtered(router, filters, JobStateFilter::Failed)
                        });
                        let go_workers = Callback::new(move |()| router.go(Route::Workers));
                        let go_libraries = Callback::new(move |()| router.go(Route::Libraries));
                        let kpis = vec![
                            StatItem::new("Tâches en cours", summary.running.to_string(), MetricTone::Neutral)
                                .with_detail(format!("{} en attente", summary.queued))
                                .with_filter(false, go_active),
                            StatItem::new(
                                "Tâches échouées",
                                summary.failed.to_string(),
                                if summary.failed > 0 { MetricTone::Danger } else { MetricTone::Neutral },
                            )
                            .with_detail(format!("{} relançable(s)", summary.retryable))
                            .with_filter(false, go_failed),
                            StatItem::new(
                                "Workers en ligne",
                                format!("{} / {}", summary.workers_online, summary.workers_total),
                                if summary.workers_online < summary.workers_total {
                                    MetricTone::Warning
                                } else {
                                    MetricTone::Success
                                },
                            )
                            .with_detail(format!("{} % actifs", ratio_percent(summary.workers_online, summary.workers_total)))
                            .with_filter(false, go_workers),
                            StatItem::new(
                                "Bibliothèques prêtes",
                                format!("{} / {}", summary.libraries_ready, summary.libraries_total),
                                MetricTone::Neutral,
                            )
                            .with_detail("flux + worker")
                            .with_filter(false, go_libraries),
                        ];
                        view! { <StatStrip aria_label="Statistiques clés" items=kpis /> }
                    })}
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
                            <JobsWidget
                                jobs=jobs_list
                                workers=workers_list
                                live_progress=live_progress
                            />
                        })}
                    </div>
                    {layout_now.is_visible(Widget::StorageSavings).then(|| {
                        let saved = storage_stats.saved_bytes;
                        let saved_label = if saved >= 0 {
                            format!("{} économisés", format_bytes(saved.unsigned_abs()))
                        } else {
                            format!("+{} utilisés", format_bytes(saved.unsigned_abs()))
                        };
                        let pct_detail = if storage_stats.total_source_bytes > 0 {
                            let pct = ((saved as f64 / storage_stats.total_source_bytes as f64) * 100.0).round() as i64;
                            format!("{pct} % sur {}", format_bytes(storage_stats.total_source_bytes.unsigned_abs()))
                        } else {
                            "Aucune tâche mesurée".to_owned()
                        };
                        view! {
                            <section class="section-panel">
                                <SectionHeader title="Économies de stockage" subtitle=format!("{saved_label} ({pct_detail})") />
                                {if storage_stats.by_library.is_empty() {
                                    view! { <EmptyState title="Aucune statistique" message="Les gains d'espace disque apparaîtront après les premières tâches réussies." icon_name=IconName::Libraries /> }.into_any()
                                } else {
                                    view! {
                                        <div class="table-wrapper">
                                            <table class="data-table" aria-label="Économies de stockage par bibliothèque">
                                                <thead>
                                                    <tr>
                                                        <th scope="col">"Bibliothèque"</th>
                                                        <th scope="col">"Réussies"</th>
                                                        <th scope="col">"Entrée"</th>
                                                        <th scope="col">"Sortie"</th>
                                                        <th scope="col" class="cell-end">"Gain"</th>
                                                    </tr>
                                                </thead>
                                                <tbody>
                                                    {storage_stats.by_library.into_iter().map(|row| {
                                                        let gain = if row.saved_bytes >= 0 {
                                                            format!("-{}", format_bytes(row.saved_bytes.unsigned_abs()))
                                                        } else {
                                                            format!("+{}", format_bytes(row.saved_bytes.unsigned_abs()))
                                                        };
                                                        view! {
                                                            <tr>
                                                                <td class="text-strong">{row.library_name}</td>
                                                                <td>{row.succeeded_count}</td>
                                                                <td class="text-dim">{format_bytes(row.source_bytes.unsigned_abs())}</td>
                                                                <td class="text-dim">{format_bytes(row.output_bytes.unsigned_abs())}</td>
                                                                <td class="cell-end text-strong">{gain}</td>
                                                            </tr>
                                                        }
                                                    }).collect_view()}
                                                </tbody>
                                            </table>
                                        </div>
                                    }.into_any()
                                }}
                            </section>
                        }
                    })}
                }
            }}
        </div>
    }
}

#[component]
pub fn WorkerOverviewPage(me: Me) -> impl IntoView {
    let can_jobs = me.allows(forgearr_shared::Permission::ViewJobs);
    let router = use_router();
    let filters = use_context::<Filters>();
    let overview = LocalResource::new(|| api::get::<WorkerOverview>(WORKER_OVERVIEW_PATH));
    view! {
        <div class="tab-panel" role="tabpanel">
            {move || match overview.get() {
                None => view! { <Hero tone="neutral" status="Vérification du central…".to_owned() /> }.into_any(),
                Some(Err(error)) => view! { <Hero tone="danger" status=format!("Central injoignable : {}", error.message()) /> }.into_any(),
                Some(Ok(view)) => {
                    let contact = view.last_seen.map_or_else(|| "aucune activité enregistrée".to_owned(), |at| format!("dernière activité {}", format::datetime(at)));
                    let mut items = vec![
                        StatItem::new("En attente", view.jobs.queued.to_string(), MetricTone::Neutral)
                            .with_detail("en file locale"),
                        StatItem::new("En cours", view.jobs.running.to_string(), MetricTone::Neutral)
                            .with_detail("traitements actifs"),
                        StatItem::new("Réussies", view.jobs.succeeded.to_string(), MetricTone::Success)
                            .with_detail("menées à terme"),
                        StatItem::new(
                            "En échec",
                            view.jobs.failed.to_string(),
                            if view.jobs.failed > 0 { MetricTone::Danger } else { MetricTone::Neutral },
                        )
                        .with_detail("sur ce worker"),
                    ];
                    if can_jobs {
                        let states = [
                            JobStateFilter::Active,
                            JobStateFilter::Active,
                            JobStateFilter::Completed,
                            JobStateFilter::Failed,
                        ];
                        for (item, state) in items.iter_mut().zip(states) {
                            *item = std::mem::replace(
                                item,
                                StatItem::new("", "", MetricTone::Neutral),
                            )
                            .with_filter(
                                false,
                                Callback::new(move |()| open_jobs_filtered(router, filters, state)),
                            );
                        }
                    }
                    view! {
                        <Hero tone="success" status=format!("Central joignable • {contact}") />
                        <StatStrip aria_label="Tâches de ce worker" items=items />
                    }.into_any()
                }
            }}
        </div>
    }
}
