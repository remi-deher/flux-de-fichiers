//! Écran Activité : registre unifié, filtrage par état/période, fiche détaillée et diagnostic
//! dans un tiroir, relance manuelle (ADR 0012, 0014).

mod detail;
mod live;
pub mod model;
pub mod table;

pub use live::LiveJobs;

use crate::api;
use crate::ui::filters::{Filters, JobStateFilter, job_state, period};
use crate::ui::search::{matches, query};
use crate::ui::{Drawer, MetricTone, StatItem, StatStrip};
use chrono::Utc;
use detail::JobDetail;
use flux_shared::flows::FlowRecord;
use flux_shared::jobs::Job;
use flux_shared::libraries::Library;
use flux_shared::session::Me;
use flux_shared::workers::Worker;
use flux_shared::{InstanceKind, Permission};
use leptos::prelude::*;
use model::{JOBS_PAGE_SIZE, may_have_more, merge_pages, summarize_jobs};
use std::collections::HashMap;
use table::JobsTable;
use uuid::Uuid;

const JOBS_PATH: &str = "/api/v1/jobs";

#[component]
pub fn JobsPage(me: Me) -> impl IntoView {
    let central = me.instance_kind == InstanceKind::Central;
    let can_manage = central && me.allows(Permission::ManageJobs);
    let can_workers = central && me.allows(Permission::ViewWorkers);
    let can_flows = central && me.allows(Permission::ViewFlows);
    let can_libraries = central && me.allows(Permission::ViewLibraries);
    let filters = use_context::<Filters>();
    // Sur le central, la liste suit le flux d'événements de l'application ; sur un worker, non.
    let live = use_context::<LiveJobs>();
    let reload = live.map_or_else(|| RwSignal::new(0_u32), |live| live.reload);
    let progress = live.map_or_else(
        || RwSignal::new(HashMap::<Uuid, u32>::new()),
        |live| live.progress,
    );
    let selected = RwSignal::new(None::<Uuid>);
    let detail_open = RwSignal::new(false);
    let selected_library = RwSignal::new(None::<Uuid>);
    let first_page = LocalResource::new(move || {
        reload.track();
        let path = format!("{JOBS_PATH}?limit={JOBS_PAGE_SIZE}");
        async move { api::get::<Vec<Job>>(&path).await }
    });
    let workers_res = LocalResource::new(move || async move {
        if can_workers {
            api::get::<Vec<Worker>>("/api/v1/workers")
                .await
                .unwrap_or_default()
        } else {
            Vec::new()
        }
    });
    let flows_res = LocalResource::new(move || async move {
        if can_flows {
            api::get::<Vec<FlowRecord>>("/api/v1/flows")
                .await
                .unwrap_or_default()
        } else {
            Vec::new()
        }
    });
    let workers_map = Signal::derive(move || {
        workers_res
            .get()
            .unwrap_or_default()
            .into_iter()
            .map(|worker| (worker.id, worker.name))
            .collect::<HashMap<Uuid, String>>()
    });
    let flows_map = Signal::derive(move || {
        flows_res
            .get()
            .unwrap_or_default()
            .into_iter()
            .map(|flow| (flow.id, flow.name))
            .collect::<HashMap<String, String>>()
    });
    let libraries_res = LocalResource::new(move || async move {
        if can_libraries {
            api::get::<Vec<Library>>("/api/v1/libraries")
                .await
                .unwrap_or_default()
        } else {
            Vec::new()
        }
    });
    let libraries_map = Signal::derive(move || {
        libraries_res
            .get()
            .unwrap_or_default()
            .into_iter()
            .map(|library| (library.id, library.name))
            .collect::<HashMap<Uuid, String>>()
    });
    let libraries_options = Signal::derive(move || {
        let mut list: Vec<(Uuid, String)> = libraries_res
            .get()
            .unwrap_or_default()
            .into_iter()
            .map(|library| (library.id, library.name))
            .collect();
        list.sort_by(|a, b| a.1.to_lowercase().cmp(&b.1.to_lowercase()));
        list
    });
    // Pages plus anciennes, ajoutées à la demande (curseur : dernière tâche chargée).
    let older = RwSignal::new(Vec::<Job>::new());
    let exhausted = RwSignal::new(false);
    let load_older = Action::new_local(move |before: &Uuid| {
        let path = format!("{JOBS_PATH}?limit={JOBS_PAGE_SIZE}&before={before}");
        async move {
            if let Ok(page) = api::get::<Vec<Job>>(&path).await {
                exhausted.set(!may_have_more(page.len()));
                older.update(|list| list.extend(page));
            }
        }
    });
    let jobs = Signal::derive(move || {
        first_page
            .get()
            .map(|result| result.map(|first| merge_pages(first, &older.get())))
    });
    let can_load_more = Signal::derive(move || {
        central
            && !exhausted.get()
            && jobs
                .get()
                .and_then(Result::ok)
                .is_some_and(|list| may_have_more(list.len()))
    });
    let refresh = Callback::new(move |job: Job| {
        selected.set(Some(job.id));
        reload.update(|count| *count = count.wrapping_add(1));
    });
    let all_jobs = Signal::derive(move || jobs.get().and_then(Result::ok).unwrap_or_default());
    let visible = Signal::derive(move || {
        let state = job_state();
        let text = query();
        let period = period();
        let lib_filter = selected_library.get();
        let now = Utc::now();
        jobs.get().map(|result| {
            result
                .map(|list| {
                    list.into_iter()
                        .filter(|job| {
                            state.accepts(job)
                                && period.accepts(job.created_at, now)
                                && lib_filter.is_none_or(|id| job.library_id == Some(id))
                                && matches(&[&job.source_name, &job.id.to_string()], &text)
                        })
                        .collect::<Vec<_>>()
                })
                .map_err(|error| error.message())
        })
    });
    let selected_job = Signal::derive(move || {
        let id = selected.get()?;
        all_jobs.get().into_iter().find(|job| job.id == id)
    });
    let open = Callback::new(move |id: Uuid| {
        selected.set(Some(id));
        detail_open.set(true);
    });
    let title = Signal::derive(move || {
        selected_job
            .get()
            .map(|job| job.source_name)
            .unwrap_or_default()
    });
    let stats = Signal::derive(move || {
        let list = all_jobs.get();
        let (running, queued, completed, failed) = summarize_jobs(&list);
        let active_filter = job_state();
        let set_state = move |next: JobStateFilter| {
            Callback::new(move |()| {
                if let Some(filters) = filters {
                    filters.job_state.set(next);
                }
            })
        };
        vec![
            StatItem::new("Toutes", list.len().to_string(), MetricTone::Neutral).with_filter(
                active_filter == JobStateFilter::Any,
                set_state(JobStateFilter::Any),
            ),
            StatItem::new(
                "En cours & file",
                (running + queued).to_string(),
                MetricTone::Neutral,
            )
            .with_detail(format!("{queued} en attente"))
            .with_filter(
                active_filter == JobStateFilter::Active,
                set_state(JobStateFilter::Active),
            ),
            StatItem::new("Terminées", completed.to_string(), MetricTone::Success).with_filter(
                active_filter == JobStateFilter::Completed,
                set_state(JobStateFilter::Completed),
            ),
            StatItem::new(
                "Échouées",
                failed.to_string(),
                if failed > 0 {
                    MetricTone::Danger
                } else {
                    MetricTone::Neutral
                },
            )
            .with_filter(
                active_filter == JobStateFilter::Failed,
                set_state(JobStateFilter::Failed),
            ),
        ]
    });
    view! {
        <div class="tab-panel" role="tabpanel">
            <StatStrip aria_label="Synthèse de l'activité" items=stats />
            {move || {
                let options = libraries_options.get();
                (!options.is_empty()).then(|| view! {
                    <div class="jobs-toolbar">
                        <label for="jobs-library-filter" class="form-label">"Bibliothèque"</label>
                        <select
                            id="jobs-library-filter"
                            class="form-select jobs-library-select"
                            prop:value=move || selected_library.get().map_or_else(String::new, |id| id.to_string())
                            on:change=move |event| {
                                let raw = event_target_value(&event);
                                selected_library.set(Uuid::parse_str(raw.trim()).ok());
                            }
                        >
                            <option value="">"Toutes les bibliothèques"</option>
                            {options.into_iter().map(|(id, name)| view! {
                                <option value=id.to_string()>{name}</option>
                            }).collect_view()}
                        </select>
                    </div>
                })
            }}
            <div class="registre">
                <JobsTable
                    jobs=visible
                    progress=progress
                    workers=workers_map
                    flows=flows_map
                    libraries=libraries_map
                    selected=selected
                    on_open=open
                />
            </div>
            {move || can_load_more.get().then(|| view! {
                <button type="button" class="btn btn-secondary btn-sm" disabled=move || load_older.pending().get()
                    on:click=move |_| if let Some(last) = all_jobs.get_untracked().last() { load_older.dispatch(last.id); }>
                    "Charger les tâches plus anciennes"
                </button>
            })}
            <Drawer open=detail_open eyebrow="Tâche" title=title>
                {move || selected_job.get().map(|job| view! {
                    <JobDetail
                        job=job
                        libraries=libraries_map
                        can_manage=can_manage
                        on_changed=refresh
                    />
                })}
            </Drawer>
        </div>
    }
}
