//! Écran Activité : file en direct, historique et diagnostic, fiche et création dans des tiroirs,
//! relance manuelle (ADR 0012, 0014).

mod create;
mod detail;
mod live;
pub mod model;
mod table;

pub use live::LiveJobs;

use crate::api;
use crate::ui::search::{matches, query};
use crate::ui::{Drawer, SubNav};
use create::CreateJob;
use detail::JobDetail;
use flux_shared::jobs::Job;
use flux_shared::session::Me;
use flux_shared::{InstanceKind, Permission};
use leptos::prelude::*;
use model::{JOBS_PAGE_SIZE, JobFilter, may_have_more, merge_pages};
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
    let tab = RwSignal::new(0_usize);
    let selected = RwSignal::new(None::<Uuid>);
    let detail_open = RwSignal::new(false);
    let create_open = RwSignal::new(false);
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
        create_open.set(false);
        reload.update(|count| *count = count.wrapping_add(1));
    });
    let all_jobs = Signal::derive(move || jobs.get().and_then(Result::ok).unwrap_or_default());
    let visible = Signal::derive(move || {
        let filter = JobFilter::TABS[tab.get()];
        let text = query();
        jobs.get().map(|result| {
            result
                .map(|list| {
                    list.into_iter()
                        .filter(|job| {
                            filter.accepts(job)
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
    let actions = can_manage.then(|| view! {
        <button type="button" class="btn btn-primary btn-sm" on:click=move |_| create_open.set(true)>"Nouvelle tâche"</button>
    }.into_any());

    view! {
        <div class="tab-panel" role="tabpanel">
            <SubNav aria_label="Sous-navigation de l'activité"
                tabs=JobFilter::TABS.iter().map(|filter| filter.tab()).collect()
                selected=tab actions=actions.unwrap_or_else(|| ().into_any()) />
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
            <Drawer open=create_open eyebrow="Activité" title="Nouvelle tâche".to_owned()>
                <CreateJob on_created=refresh />
            </Drawer>
        </div>
    }
}
