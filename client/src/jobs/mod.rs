//! Écran Tâches : liste en direct, filtres, fiche, création et relance (ADR 0012, 0014).

mod create;
mod detail;
mod live;

pub use live::LiveJobs;
pub mod model;
mod table;

use crate::api;
use create::CreateJob;
use detail::JobDetail;
use flux_shared::Permission;
use flux_shared::jobs::Job;
use flux_shared::session::Me;
use leptos::prelude::*;
use model::JobFilter;
use std::collections::HashMap;
use table::JobsTable;
use uuid::Uuid;

/// Tâches lues par page ; le central en sert 200 au plus.
const JOBS_PATH: &str = "/api/v1/jobs?limit=200";

#[component]
pub fn JobsPage(me: Me) -> impl IntoView {
    let central = me.instance_kind == flux_shared::InstanceKind::Central;
    let can_manage = central && me.allows(Permission::ManageJobs);
    // Sur le central, la liste suit le flux d'événements de l'application ; sur un worker, non.
    let live = use_context::<LiveJobs>();
    let reload = live.map_or_else(|| RwSignal::new(0_u32), |live| live.reload);
    let progress = live.map_or_else(
        || RwSignal::new(HashMap::<Uuid, u32>::new()),
        |live| live.progress,
    );
    let filter = RwSignal::new(JobFilter::All);
    let selected = RwSignal::new(None::<Uuid>);
    let jobs = LocalResource::new(move || {
        reload.track();
        api::get::<Vec<Job>>(JOBS_PATH)
    });

    let refresh = Callback::new(move |job: Job| {
        selected.set(Some(job.id));
        reload.update(|count| *count = count.wrapping_add(1));
    });
    let all_jobs = Signal::derive(move || jobs.get().and_then(Result::ok).unwrap_or_default());
    let filtered = Signal::derive(move || {
        let current = filter.get();
        all_jobs
            .get()
            .into_iter()
            .filter(|job| current.accepts(job))
            .collect::<Vec<_>>()
    });
    let selected_job = move || {
        let id = selected.get()?;
        all_jobs.get().into_iter().find(|job| job.id == id)
    };
    let load_error = move || match jobs.get() {
        Some(Err(error)) => {
            Some(view! { <p class="settings-alert" role="alert">{error.message()}</p> })
        }
        _ => None,
    };

    view! {
        <div class="registre">
            <div class="cluster-spread">
                <h1 class="section-title">"Tâches"</h1>
                <div class="theme-segmented" role="group" aria-label="Filtrer par état">
                    {JobFilter::CHOICES.into_iter().map(|choice| view! {
                        <button
                            type="button"
                            class="theme-segmented__btn"
                            aria-pressed=move || (filter.get() == choice).to_string()
                            on:click=move |_| filter.set(choice)
                        >
                            {choice.label()}
                        </button>
                    }).collect_view()}
                </div>
            </div>
            {load_error}
            {can_manage.then(|| view! { <CreateJob on_created=refresh /> })}
            <div class="overview-grid-2col">
                <JobsTable jobs=filtered progress=progress selected=selected />
                {move || selected_job().map(|job| view! {
                    <JobDetail job=job can_manage=can_manage on_changed=refresh />
                })}
            </div>
        </div>
    }
}
