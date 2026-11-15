//! Écran Activité : registre unifié, filtrage par état/période, fiche détaillée et diagnostic
//! dans un tiroir, relance manuelle (ADR 0012, 0014).

mod detail;
mod live;
pub mod model;
pub mod table;

pub use live::LiveJobs;

use crate::api;
use crate::ui::filters::{job_state, period};
use crate::ui::search::{matches, query};
use crate::ui::{Drawer, MetricTone, StatItem, StatStrip};
use chrono::Utc;
use detail::JobDetail;
use flux_shared::jobs::Job;
use flux_shared::session::Me;
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
    // Sur le central, la liste suit le flux d'événements de l'application ; sur un worker, non.
    let live = use_context::<LiveJobs>();
    let reload = live.map_or_else(|| RwSignal::new(0_u32), |live| live.reload);
    let progress = live.map_or_else(
        || RwSignal::new(HashMap::<Uuid, u32>::new()),
        |live| live.progress,
    );
    let selected = RwSignal::new(None::<Uuid>);
    let detail_open = RwSignal::new(false);
    let first_page = LocalResource::new(move || {
        reload.track();
        let path = format!("{JOBS_PATH}?limit={JOBS_PAGE_SIZE}");
        async move { api::get::<Vec<Job>>(&path).await }
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
        let now = Utc::now();
        jobs.get().map(|result| {
            result
                .map(|list| {
                    list.into_iter()
                        .filter(|job| {
                            state.accepts(job)
                                && period.accepts(job.created_at, now)
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
        vec![
            StatItem::new("En cours", running.to_string(), MetricTone::Neutral),
            StatItem::new("En attente", queued.to_string(), MetricTone::Neutral),
            StatItem::new("Terminées", completed.to_string(), MetricTone::Success),
            StatItem::new(
                "Échouées",
                failed.to_string(),
                if failed > 0 {
                    MetricTone::Danger
                } else {
                    MetricTone::Neutral
                },
            ),
        ]
    });
    view! {
        <div class="tab-panel" role="tabpanel">
            <StatStrip aria_label="Synthèse de l'activité" items=stats />
            <div class="registre">
                <JobsTable jobs=visible progress=progress selected=selected on_open=open />
            </div>
            {move || can_load_more.get().then(|| view! {
                <button type="button" class="btn btn-secondary btn-sm" disabled=move || load_older.pending().get()
                    on:click=move |_| if let Some(last) = all_jobs.get_untracked().last() { load_older.dispatch(last.id); }>
                    "Charger les tâches plus anciennes"
                </button>
            })}
            <Drawer open=detail_open eyebrow="Tâche" title=title>
                {move || selected_job.get().map(|job| view! { <JobDetail job=job can_manage=can_manage on_changed=refresh /> })}
            </Drawer>
        </div>
    }
}
