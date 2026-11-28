//! Écran Activité : registre unifié, filtrage par état/période, fiche détaillée et diagnostic
//! dans un tiroir, relance manuelle (ADR 0012, 0014).

mod detail;
pub(crate) use detail::JobDetail;
mod diagnosis;
mod lease;
mod live;
mod logs;
pub mod model;
mod query;
pub mod table;

pub use live::LiveJobs;

use crate::api;
use crate::ui::filters::{Filters, JobStateFilter, job_state, period};
use crate::ui::search::{matches, query};
use crate::ui::{Drawer, MetricTone, PageSubnav, StatItem, StatStrip, Toasts};
use crate::url_state::{UrlTab, use_tab};
use chrono::Utc;
use diagnosis::AiDiagnosisCard;
use forgearr_shared::flows::FlowRecord;
use forgearr_shared::jobs::{Job, JobBatchAction, JobBatchRequest, JobBatchResult};
use forgearr_shared::libraries::Library;
use forgearr_shared::session::Me;
use forgearr_shared::workers::Worker;
use forgearr_shared::{InstanceKind, Permission};
use leptos::prelude::*;
use logs::LogsPanel;
use model::{JOBS_PAGE_SIZE, may_have_more, merge_pages, summarize_jobs};
use std::collections::{HashMap, HashSet};
use table::JobsTable;
use uuid::Uuid;

const JOBS_PATH: &str = "/api/v1/jobs";
const JOBS_BATCH_PATH: &str = "/api/v1/jobs/batch";

/// Vues de l'écran Activité.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ActivityTab {
    Tasks,
    Logs,
}

impl ActivityTab {
    const ALL: [Self; 2] = [Self::Tasks, Self::Logs];

    const fn label(self) -> &'static str {
        match self {
            Self::Tasks => "Tâches",
            Self::Logs => "Journal",
        }
    }
}

impl UrlTab for ActivityTab {
    fn slug(self) -> &'static str {
        match self {
            Self::Tasks => "tasks",
            Self::Logs => "logs",
        }
    }

    fn from_slug(slug: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|tab| tab.slug() == slug)
    }
}

#[component]
pub fn JobsPage(me: Me) -> impl IntoView {
    // Le journal du central est réservé à ceux qui gèrent les clés (administrateurs).
    let can_logs =
        me.instance_kind == InstanceKind::Central && me.allows(Permission::ManageApiKeys);
    let allowed_tabs = if can_logs {
        ActivityTab::ALL.to_vec()
    } else {
        vec![ActivityTab::Tasks]
    };
    let active_tab = use_tab(ActivityTab::Tasks, allowed_tabs);
    // Le diagnostic IA n'existe que sur le central ; seul un administrateur le recalcule.
    let is_central = me.instance_kind == InstanceKind::Central;
    let can_refresh_diagnosis = is_central && me.allows(Permission::ManageSettings);
    view! {
        {can_logs.then(|| view! {
            <PageSubnav
                aria_label="Vues de l'activité"
                tabs=ActivityTab::ALL.into_iter().map(|tab| (tab, tab.label())).collect()
                active=active_tab
            />
        })}
        {move || match active_tab.get() {
            ActivityTab::Tasks => view! {
                {is_central.then(|| view! { <AiDiagnosisCard can_refresh=can_refresh_diagnosis /> })}
                <JobsTasks me=me.clone() />
            }.into_any(),
            ActivityTab::Logs => view! { <LogsPanel /> }.into_any(),
        }}
    }
}

#[component]
fn JobsTasks(me: Me) -> impl IntoView {
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
    let initial = query::JobsQuery::current();
    let selected = RwSignal::new(initial.job);
    let checked_ids = RwSignal::new(HashSet::<Uuid>::new());
    let detail_open = RwSignal::new(initial.job.is_some());
    let job_tab = RwSignal::new(usize::from(initial.step.is_some()));
    let initial_step = initial.step;
    let initial_job = initial.job;
    let selected_library = RwSignal::new(initial.library);
    let volume = RwSignal::new(initial.volume);
    let initial_state = RwSignal::new(initial.state);
    let selected_error = RwSignal::new(initial.error);
    let params = move || {
        query::JobsQuery {
            state: initial_state
                .get()
                .or_else(|| query::JobsQuery::state_name(job_state()).map(str::to_owned)),
            library: selected_library.get(),
            volume: volume.get(),
            error: selected_error.get(),
            ..Default::default()
        }
        .list_params()
    };
    let replay_version = RwSignal::new(String::new());
    let replay = Action::new_local(move |request: &forgearr_shared::jobs::FailureReplay| {
        let request = request.clone();
        async move { api::post::<_, JobBatchResult>("/api/v1/jobs/failures", Some(&request)).await }
    });
    Effect::new(move |_| {
        if let Some(result) = replay.value().get() {
            match result {
                Ok(result) => {
                    Toasts::notify_success(format!(
                        "{} remise(s) en file · {} ignorée(s)",
                        result.updated, result.skipped
                    ));
                    checked_ids.set(HashSet::new());
                    reload.update(|n| *n = n.wrapping_add(1));
                }
                Err(error) => Toasts::notify_error(error.message()),
            }
        }
    });
    let deep_job = LocalResource::new(move || {
        let id = selected.get();
        reload.track();
        async move {
            match id {
                Some(id) if central => api::get::<Job>(&format!("{JOBS_PATH}/{id}")).await.ok(),
                _ => None,
            }
        }
    });
    let batch_action = Action::new_local(move |req: &JobBatchRequest| {
        let req = req.clone();
        async move { api::post::<JobBatchRequest, JobBatchResult>(JOBS_BATCH_PATH, Some(&req)).await }
    });
    Effect::new(move |_| match batch_action.value().get() {
        Some(Ok(res)) => {
            checked_ids.set(HashSet::new());
            Toasts::notify_success(format!(
                "{} tâche(s) traitée(s) ({} ignorée(s)).",
                res.updated, res.skipped
            ));
            reload.update(|count| *count = count.wrapping_add(1));
        }
        Some(Err(err)) => Toasts::notify_error(err.message()),
        None => {}
    });
    let first_page = LocalResource::new(move || {
        reload.track();
        let path = format!(
            "{JOBS_PATH}?limit={JOBS_PAGE_SIZE}{}",
            if central { params() } else { String::new() }
        );
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
        list.sort_by_key(|a| a.1.to_lowercase());
        list
    });
    // Pages plus anciennes, ajoutées à la demande (curseur : dernière tâche chargée).
    let older = RwSignal::new(Vec::<Job>::new());
    let exhausted = RwSignal::new(false);
    Effect::new(move |_| {
        let query = params();
        older.set(Vec::new());
        exhausted.set(false);
        checked_ids.set(HashSet::new());
        let location = window().location();
        let search = location.search().unwrap_or_default();
        let deep = search
            .trim_start_matches('?')
            .split('&')
            .filter(|item| item.starts_with("job=") || item.starts_with("step="))
            .map(|item| format!("&{item}"))
            .collect::<String>();
        let query = format!("{query}{deep}");
        let path = format!(
            "{}{}",
            location.pathname().unwrap_or_default(),
            if query.is_empty() {
                String::new()
            } else {
                format!("?{}", query.trim_start_matches('&'))
            }
        );
        if let Ok(history) = window().history() {
            let _ = history.replace_state_with_url(&wasm_bindgen::JsValue::NULL, "", Some(&path));
        }
    });
    let load_older = Action::new_local(move |before: &Uuid| {
        let path = format!(
            "{JOBS_PATH}?limit={JOBS_PAGE_SIZE}&before={before}{}",
            params()
        );
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
        let state = match initial_state.get().as_deref() {
            Some("active") => JobStateFilter::Active,
            Some("failed") => JobStateFilter::Failed,
            Some("completed" | "succeeded") => JobStateFilter::Completed,
            _ => job_state(),
        };
        let succeeded_only = initial_state.get().as_deref() == Some("succeeded");
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
                                && (!succeeded_only
                                    || job.status == forgearr_shared::jobs::JobStatus::Succeeded)
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
        all_jobs
            .get()
            .into_iter()
            .find(|job| job.id == id)
            .or_else(|| deep_job.get().flatten().filter(|job| job.id == id))
    });
    let open = Callback::new(move |id: Uuid| {
        job_tab.set(0);
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
        let active_filter = match initial_state.get().as_deref() {
            Some("active") => JobStateFilter::Active,
            Some("failed") => JobStateFilter::Failed,
            Some("completed" | "succeeded") => JobStateFilter::Completed,
            _ => job_state(),
        };
        let set_state = move |next: JobStateFilter| {
            Callback::new(move |()| {
                initial_state.set(None);
                volume.set(None);
                selected_error.set(None);
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
            {move || central.then(|| view! {
                <div class="cluster">
                    {volume.get().map(|label| view! { <span>{format!("Volume · {}", if label.is_empty() { "Entrée par défaut".into() } else { label })}</span> })}
                    {selected_error.get().map(|code| view! { <span>{model::error_label(code)}</span> })}
                    {(volume.get().is_some() || selected_error.get().is_some()).then(|| view! {
                        <button type="button" class="btn btn-secondary btn-sm" on:click=move |_| { volume.set(None); selected_error.set(None); }>"Effacer le filtre de volume ou d’échec"</button>
                    })}
                    {(can_manage && !checked_ids.get().is_empty()).then(|| view! {
                        <label for="replay-flow-version">"Version pour retraiter les échecs"</label>
                        <input id="replay-flow-version" class="form-input" inputmode="numeric" placeholder="Version d’origine si vide"
                            prop:value=move || replay_version.get()
                            on:input=move |event| replay_version.set(event_target_value(&event)) />
                        <button type="button" class="btn btn-secondary" disabled=move || replay.pending().get()
                            on:click=move |_| {
                                let raw = replay_version.get_untracked();
                                let version = raw.trim().parse::<i64>().ok().filter(|number| *number > 0);
                                if !raw.trim().is_empty() && version.is_none() {
                                    Toasts::notify_error("Saisissez une version entière positive.");
                                    return;
                                }
                                let ids: Vec<_> = checked_ids.get_untracked().into_iter().collect();
                                if window().confirm_with_message(&format!("Retraiter les échecs parmi les {} tâches sélectionnées ?", ids.len())).unwrap_or(false) {
                                    replay.dispatch(forgearr_shared::jobs::FailureReplay { ids, version });
                                }
                            }>"Retraiter les échecs sélectionnés"</button>
                    })}
                </div>
            })}

            {move || {
                let options = libraries_options.get();
                let selected_count = checked_ids.with(HashSet::len);
                (can_manage || !options.is_empty()).then(|| view! {
                    <div class="jobs-toolbar cluster cluster-wrap">
                        {(!options.is_empty()).then(|| view! {
                            <div class="cluster">
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
                        })}
                        {can_manage.then(|| view! {
                            <div class="cluster cluster-wrap">
                                <button type="button" class="btn btn-secondary btn-sm"
                                    on:click=move |_| {
                                        if let Some(Ok(list)) = visible.get_untracked() {
                                            checked_ids.set(list.into_iter().map(|j| j.id).collect());
                                        }
                                    }>
                                    "Tout sélectionner"
                                </button>
                                {(selected_count > 0).then(|| {
                                    let dispatch_batch = move |action: JobBatchAction| {
                                        let ids: Vec<Uuid> = checked_ids.get_untracked().into_iter().collect();
                                        if !ids.is_empty() {
                                            batch_action.dispatch(JobBatchRequest { ids, action });
                                        }
                                    };
                                    view! {
                                        <span class="text-sm text-dim">{format!("{selected_count} sélectionnée(s)")}</span>
                                        <button type="button" class="btn btn-secondary btn-sm"
                                            disabled=move || batch_action.pending().get()
                                            on:click=move |_| dispatch_batch(JobBatchAction::Retry)>
                                            "Relancer"
                                        </button>
                                        <button type="button" class="btn btn-secondary btn-sm"
                                            disabled=move || batch_action.pending().get()
                                            on:click=move |_| dispatch_batch(JobBatchAction::Cancel)>
                                            "Annuler"
                                        </button>
                                        <button type="button" class="btn btn-danger-quiet btn-sm"
                                            disabled=move || batch_action.pending().get()
                                            on:click=move |_| dispatch_batch(JobBatchAction::Delete)>
                                            "Supprimer"
                                        </button>
                                        <button type="button" class="btn btn-secondary btn-sm"
                                            on:click=move |_| checked_ids.set(HashSet::new())>
                                            "Effacer"
                                        </button>
                                    }
                                })}
                            </div>
                        })}
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
                    checked_ids=checked_ids
                    can_manage=can_manage
                    on_open=open
                />
            </div>
            {move || can_load_more.get().then(|| view! {
                <button type="button" class="btn btn-secondary btn-sm" disabled=move || load_older.pending().get()
                    on:click=move |_| if let Some(last) = all_jobs.get_untracked().last() { load_older.dispatch(last.id); }>
                    "Charger les tâches plus anciennes"
                </button>
            })}
            <Drawer
                open=detail_open
                eyebrow="Tâche"
                title=title
                tabs=vec!["Information", "Étapes exécutées"]
                tab=job_tab
            >
                {move || selected_job.get().map(|job| view! {
                    <JobDetail
                        focused_step=if Some(job.id) == initial_job { initial_step } else { None }
                        job=job
                        tab=job_tab
                        libraries=libraries_map
                        workers=workers_map
                        flows=flows_map
                        can_manage=can_manage
                        on_changed=refresh
                    />
                })}
            </Drawer>
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn activity_tabs_are_named_in_the_address_and_read_back() {
        for tab in ActivityTab::ALL {
            assert_eq!(ActivityTab::from_slug(tab.slug()), Some(tab));
        }
        assert_eq!(ActivityTab::from_slug("inconnu"), None);
    }
}
