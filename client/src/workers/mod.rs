//! Écran Workers : présence, connectivité, worker intégré, retrait et enrôlement (ADR 0014).

mod library_modes;
mod slots_field;

use crate::route::{Link, Route};
use crate::settings::duration_label;
use crate::sleep::sleep;
use crate::ui::filters::{Filters, Presence, presence};
use crate::ui::search::{matches, query};
use crate::ui::topbar_action::{ActionSpec, provide_topbar_action};
use crate::ui::{
    Drawer, EmptyState, Icon, IconName, MetricTone, ResourceCard, SectionHeader, StatItem,
    StatStrip, StatusBadge, Tag, TagTone, Toasts,
};
use crate::{api, format};
use chrono::{DateTime, Utc};
use forgearr_shared::Permission;
use forgearr_shared::jobs::{Job, JobStatus};
use forgearr_shared::libraries::Library;
use forgearr_shared::session::Me;
use forgearr_shared::workers::{
    EnrollmentWindowStatus, Liveness, UpdateWorkerSettingsRequest, WORKER_MAX_CONCURRENCY,
    WORKER_MIN_CONCURRENCY, Worker, WorkerEnrollmentList, WorkerEnrollmentSummary, WorkerSchedule,
};
use leptos::prelude::*;
use library_modes::{WorkerLibrariesSection, worker_libraries};
use serde::Serialize;
use slots_field::{ClassSlotsField, capacity_label};
use uuid::Uuid;

const WORKERS_PATH: &str = "/api/v1/workers";
const WORKER_REQUESTS_PATH: &str = "/api/v1/workers/requests";
const WORKER_ENROLLMENT_WINDOW_PATH: &str = "/api/v1/workers/enrollment-window";
const LIBRARIES_PATH: &str = "/api/v1/libraries";
const JOBS_PATH: &str = "/api/v1/jobs?limit=200";
const REQUESTS_POLL_MS: i32 = 3000;

#[derive(Serialize)]
struct EnrollmentWindowBody {
    open: bool,
}

/// Service `compose` d'un worker distant par approbation (aucune clé ni secret à copier).
pub fn enroll_approval_compose() -> &'static str {
    "services:
  forgearr-worker:
    image: mrcryllix/forgearr:latest
    restart: unless-stopped
    ports:
      - \"3001:3000\"
    volumes:
      - forgearr-worker:/config
      - /chemin/data:/data
volumes:
  forgearr-worker:
"
}

/// Libellé du temps restant avant expiration d'une fenêtre ou d'une demande.
pub fn remaining_label(expires_at: DateTime<Utc>, now: DateTime<Utc>) -> String {
    let secs = (expires_at - now).num_seconds().max(0);
    duration_label(secs)
}

/// Service `compose` d'un worker distant ; la clé vient d'un fichier, jamais d'une valeur en clair.
pub fn enroll_compose(origin: &str) -> String {
    format!(
        "services:
  forgearr-worker:
    image: mrcryllix/forgearr:latest
    command: worker
    restart: unless-stopped
    environment:
      CENTRAL_URL: {origin}
      WORKER_KEY_FILE: /config/worker_key
    volumes:
      - forgearr-worker:/config
      - /chemin/data:/data
volumes:
  forgearr-worker:
"
    )
}

pub fn origin() -> String {
    window().location().origin().unwrap_or_default()
}

fn role_meta(worker: &Worker) -> (&'static str, TagTone) {
    if worker.is_integrated() {
        ("Intégré au central", TagTone::Primary)
    } else {
        ("Worker distant", TagTone::Neutral)
    }
}

pub fn liveness_badge(liveness: Liveness) -> (&'static str, &'static str) {
    match liveness {
        Liveness::Online => ("En ligne", "online"),
        Liveness::Offline => ("Hors ligne", "warning"),
        Liveness::Silent => ("Silencieux", "error"),
    }
}

/// Ancienneté lisible du dernier contact reçu par le central.
pub fn contact_age(worker: &Worker, now: DateTime<Utc>) -> String {
    format!("il y a {}", duration_label(worker.silence_seconds(now)))
}

/// Synthèse de l'infrastructure des workers à l'instant `now`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WorkerSummary {
    pub total: usize,
    pub online: usize,
    pub offline: usize,
    pub silent: usize,
    pub busy_workers: usize,
    pub active_jobs: usize,
}

/// Nombre de tâches actives (`Running` ou `Queued`) assignées à un worker.
pub fn active_jobs_for(worker_id: Uuid, jobs: &[Job]) -> usize {
    jobs.iter()
        .filter(|job| {
            job.worker_id == Some(worker_id)
                && matches!(job.status, JobStatus::Running | JobStatus::Queued)
        })
        .count()
}

pub fn summarize_workers(workers: &[Worker], jobs: &[Job], now: DateTime<Utc>) -> WorkerSummary {
    let total = workers.len();
    let mut online = 0_usize;
    let mut offline = 0_usize;
    let mut silent = 0_usize;
    let mut busy_workers = 0_usize;
    let mut active_jobs = 0_usize;
    for worker in workers {
        match worker.liveness(now) {
            Liveness::Online => online += 1,
            Liveness::Offline => offline += 1,
            Liveness::Silent => silent += 1,
        }
        let worker_jobs = active_jobs_for(worker.id, jobs);
        if worker_jobs > 0 {
            busy_workers += 1;
            active_jobs += worker_jobs;
        }
    }
    WorkerSummary {
        total,
        online,
        offline,
        silent,
        busy_workers,
        active_jobs,
    }
}

/// Filtre les workers par recherche textuelle (`nom`, `id`) et critère de présence/activité.
pub fn filter_workers(
    workers: Vec<Worker>,
    jobs: &[Job],
    text: &str,
    online_filter: crate::ui::filters::Presence,
    now: DateTime<Utc>,
) -> Vec<Worker> {
    workers
        .into_iter()
        .filter(|worker| {
            matches(&[&worker.name, &worker.id.to_string()], text)
                && online_filter
                    .accepts(worker.is_online(now), active_jobs_for(worker.id, jobs) > 0)
        })
        .collect()
}

fn storage_used_bytes(telemetry: &forgearr_shared::workers::WorkerTelemetry) -> u64 {
    telemetry
        .storage_total_bytes
        .saturating_sub(telemetry.storage_available_bytes)
}

fn telemetry_summary(telemetry: &forgearr_shared::workers::WorkerTelemetry) -> String {
    let ram_pct =
        crate::overview::hardware::percent(telemetry.ram_used_bytes, telemetry.ram_total_bytes);
    let mut base = format!(
        "{} cœurs · CPU {:.0} % · RAM {:.0} %",
        telemetry.logical_cores, telemetry.cpu_usage_percent, ram_pct
    );
    if !telemetry.gpu_devices.is_empty() {
        base.push_str(&format!(" · {} GPU", telemetry.gpu_devices.len()));
    }
    base
}

#[component]
fn WorkerTelemetrySection(telemetry: forgearr_shared::workers::WorkerTelemetry) -> impl IntoView {
    use crate::settings::system::{DetailGrid, DetailRow};
    let cpu_pct = telemetry.cpu_usage_percent.clamp(0.0, 100.0);
    let ram_pct =
        crate::overview::hardware::percent(telemetry.ram_used_bytes, telemetry.ram_total_bytes);
    let disk_used = storage_used_bytes(&telemetry);
    let disk_pct = crate::overview::hardware::percent(disk_used, telemetry.storage_total_bytes);
    let cpu_tone = crate::overview::hardware::usage_tone(cpu_pct);
    let ram_tone = crate::overview::hardware::usage_tone(ram_pct);
    let disk_tone = crate::overview::hardware::usage_tone(disk_pct);
    let ram_detail =
        crate::overview::hardware::used_label(telemetry.ram_used_bytes, telemetry.ram_total_bytes);
    let disk_detail = if telemetry.storage_total_bytes > 0 {
        crate::overview::hardware::used_label(disk_used, telemetry.storage_total_bytes)
    } else {
        "Volume non mesuré".to_owned()
    };
    let gpus = telemetry.gpu_devices.clone();
    let gpu_headline = crate::overview::hardware::gpu_label(&gpus);
    view! {
        <div class="stack worker-telemetry-card">
            <h3 class="flow-inspector__section-title">"Télémétrie matérielle"</h3>
            <DetailGrid rows=vec![
                DetailRow::text("Relevé", format::datetime(telemetry.timestamp)),
                DetailRow::mono("Cœurs logiques", telemetry.logical_cores.to_string()),
                DetailRow::text("GPU", gpu_headline),
            ] />
            <div class="worker-telemetry-bars">
                <div class="worker-telemetry-metric">
                    <div class="worker-telemetry-metric__head">
                        <span>"CPU"</span>
                        <span class="text-mono">{format!("{cpu_pct:.0} %")}</span>
                    </div>
                    <crate::ui::ProgressBar percent=cpu_pct.round() as u32 tone=cpu_tone />
                </div>
                <div class="worker-telemetry-metric">
                    <div class="worker-telemetry-metric__head">
                        <span>"Mémoire vive"</span>
                        <span class="text-dim">{ram_detail}</span>
                    </div>
                    <crate::ui::ProgressBar percent=ram_pct.round() as u32 tone=ram_tone />
                </div>
                <div class="worker-telemetry-metric">
                    <div class="worker-telemetry-metric__head">
                        <span>"Stockage local"</span>
                        <span class="text-dim">{disk_detail}</span>
                    </div>
                    <crate::ui::ProgressBar percent=disk_pct.round() as u32 tone=disk_tone />
                </div>
            </div>
            <TagRow labels=gpus tone=TagTone::Mono />
            <TagRow labels=encoder_labels(&telemetry.capabilities) tone=TagTone::Success />
        </div>
    }
}

/// Libellés des encodeurs matériels déclarés par un worker (ex. « H.265 · NVENC »).
fn encoder_labels(capabilities: &forgearr_shared::capabilities::WorkerCapabilities) -> Vec<String> {
    let hardware = capabilities
        .hardware_encoders
        .iter()
        .map(|encoder| encoder.label());
    let software = capabilities.software_encoders.iter().map(|codec| {
        format!(
            "{} · {}",
            codec.label(),
            forgearr_shared::capabilities::EncodeAcceleration::Cpu.label()
        )
    });
    hardware.chain(software).collect()
}

/// Rangée d'étiquettes ; rien n'est affiché quand la liste est vide.
#[component]
fn TagRow(labels: Vec<String>, tone: TagTone) -> impl IntoView {
    (!labels.is_empty()).then(|| {
        view! {
            <div class="cluster cluster-wrap">
                {labels.into_iter().map(|label| view! { <Tag label tone /> }).collect_view()}
            </div>
        }
    })
}

#[component]
fn WorkerDetail(
    worker: Worker,
    can_manage: bool,
    /// Bibliothèques lisibles par l'utilisateur ; `None` sans droit de lecture.
    libraries: Option<Vec<Library>>,
    on_removed: Callback<()>,
) -> impl IntoView {
    let now = Utc::now();
    let liveness = worker.liveness(now);
    let (badge_label, badge_variant) = liveness_badge(liveness);
    let (role_label, role_tone) = role_meta(&worker);
    let age = contact_age(&worker, now);
    let feedback = RwSignal::new(None::<String>);
    let id = worker.id;
    let version = worker
        .version
        .clone()
        .unwrap_or_else(|| "Non transmise".to_owned());
    let remote_addr = worker.remote_addr.clone().unwrap_or_else(|| {
        if worker.is_integrated() {
            "Local (intégré)".to_owned()
        } else {
            "Non relevée".to_owned()
        }
    });
    let telemetry = worker.telemetry.clone();
    let worker_libs =
        libraries.map(|list| worker_libraries(&list, worker.id, worker.is_integrated()));
    let remove = Action::new_local(move |_: &()| async move {
        api::delete(&format!("{WORKERS_PATH}/{id}")).await
    });
    Effect::new(move |_| match remove.value().get() {
        Some(Ok(())) => {
            Toasts::notify_success("Worker retiré.");
            on_removed.run(());
        }
        Some(Err(error)) => Toasts::set_error(feedback, error.message()),
        None => {}
    });
    let initial_sched = worker.schedule.clone().unwrap_or(WorkerSchedule {
        enabled: false,
        start_hour: 22,
        end_hour: 6,
        days: Vec::new(),
    });
    let concurrency_val = RwSignal::new(worker.max_concurrency);
    let cpu_slots = RwSignal::new(worker.max_cpu_jobs);
    let gpu_slots = RwSignal::new(worker.max_gpu_jobs);
    let capacity = capacity_label(&worker);
    let sched_enabled = RwSignal::new(initial_sched.enabled);
    let sched_start = RwSignal::new(initial_sched.start_hour);
    let sched_end = RwSignal::new(initial_sched.end_hour);
    let save_settings = Action::new_local(move |req: &UpdateWorkerSettingsRequest| {
        let path = format!("{WORKERS_PATH}/{id}/settings");
        let req = req.clone();
        async move { api::put::<UpdateWorkerSettingsRequest, Worker>(&path, &req).await }
    });
    Effect::new(move |_| match save_settings.value().get() {
        Some(Ok(_)) => {
            Toasts::notify_success("Réglages du worker enregistrés.");
            on_removed.run(());
        }
        Some(Err(error)) => Toasts::set_error(feedback, error.message()),
        None => {}
    });
    let removable = can_manage && liveness != Liveness::Online && !worker.is_integrated();
    view! {
        <div class="inspection-panel">
            <div class="cluster cluster-wrap">
                <StatusBadge label=badge_label variant=badge_variant pulse=liveness == Liveness::Online />
                <Tag label=role_label tone=role_tone />
                {worker.version.clone().map(|v| view! { <Tag label=format!("v{v}") tone=TagTone::Mono /> })}
            </div>
            <div class="detail-grid">
                <div class="detail-term">"Identifiant"</div><div class="detail-val text-mono">{worker.id.to_string()}</div>
                <div class="detail-term">"Rôle"</div><div class="detail-val">{role_label}</div>
                <div class="detail-term">"Version"</div><div class="detail-val text-mono">{version}</div>
                <div class="detail-term">"Adresse réseau"</div><div class="detail-val text-mono">{remote_addr}</div>
                <div class="detail-term">"Dernier contact"</div><div class="detail-val">{format::datetime(worker.last_seen)}</div>
                <div class="detail-term">"Ancienneté"</div>
                <div class="detail-val" class:text-danger=liveness == Liveness::Silent>{age}</div>
                <div class="detail-term">"Capacité"</div><div class="detail-val">{capacity}</div>
            </div>
            {can_manage.then(|| view! {
                <form class="inspection-panel__section" on:submit=move |ev| {
                    ev.prevent_default();
                    let schedule = sched_enabled.get_untracked().then(|| WorkerSchedule {
                        enabled: true,
                        start_hour: sched_start.get_untracked().min(23),
                        end_hour: sched_end.get_untracked().min(23),
                        days: Vec::new(),
                    });
                    save_settings.dispatch(UpdateWorkerSettingsRequest {
                        max_concurrency: concurrency_val.get_untracked().clamp(WORKER_MIN_CONCURRENCY, WORKER_MAX_CONCURRENCY),
                        max_cpu_jobs: cpu_slots.get_untracked(),
                        max_gpu_jobs: gpu_slots.get_untracked(),
                        schedule,
                    });
                }>
                    <p class="inspection-panel__title">"Ordonnancement & concurrence"</p>
                    <div class="form-group">
                        <label class="form-label" for="worker-concurrency">"Tâches simultanées (1 à 16)"</label>
                        <input id="worker-concurrency" type="number" class="form-input"
                            min=WORKER_MIN_CONCURRENCY max=WORKER_MAX_CONCURRENCY
                            prop:value=move || concurrency_val.get().to_string()
                            on:input=move |ev| {
                                if let Ok(v) = event_target_value(&ev).parse::<i32>() {
                                    concurrency_val.set(v);
                                }
                            } />
                    </div>
                    <ClassSlotsField input_id="worker-cpu-slots" label="Places CPU (facultatif)"
                        help="Vide avec les places GPU : la concurrence ci-dessus vaut pour toutes les tâches."
                        value=cpu_slots />
                    <ClassSlotsField input_id="worker-gpu-slots" label="Places GPU (facultatif)"
                        help="Tâches à encodage matériel (NVENC, VA-API). 0 : ce worker ne prend aucune tâche GPU."
                        value=gpu_slots />
                    <label class="cluster">
                        <input type="checkbox"
                            prop:checked=move || sched_enabled.get()
                            on:change=move |ev| sched_enabled.set(event_target_checked(&ev)) />
                        <span>"Limiter l'activité à une plage horaire (heure locale)"</span>
                    </label>
                    {move || sched_enabled.get().then(|| view! {
                        <div class="cluster">
                            <label class="form-label" for="worker-sched-start">"De (h)"</label>
                            <input id="worker-sched-start" type="number" class="form-input" min="0" max="23"
                                prop:value=move || sched_start.get().to_string()
                                on:input=move |ev| {
                                    if let Ok(v) = event_target_value(&ev).parse::<u8>() {
                                        sched_start.set(v);
                                    }
                                } />
                            <label class="form-label" for="worker-sched-end">"À (h)"</label>
                            <input id="worker-sched-end" type="number" class="form-input" min="0" max="23"
                                prop:value=move || sched_end.get().to_string()
                                on:input=move |ev| {
                                    if let Ok(v) = event_target_value(&ev).parse::<u8>() {
                                        sched_end.set(v);
                                    }
                                } />
                        </div>
                    })}
                    <div class="inspection-drawer__actions">
                        <button type="submit" class="btn btn-primary btn-sm" disabled=move || save_settings.pending().get()>
                            "Enregistrer les réglages"
                        </button>
                    </div>
                </form>
            })}
            {worker_libs.map(|libraries| view! { <WorkerLibrariesSection libraries=libraries /> })}
            {match telemetry {
                Some(t) => view! { <WorkerTelemetrySection telemetry=t /> }.into_any(),
                None => view! {
                    <p class="form-help">"Aucune télémétrie matérielle encore remontée pour ce nœud."</p>
                }.into_any(),
            }}
            <div class="inspection-drawer__actions">
                {removable.then(|| view! {
                    <button type="button" class="btn btn-danger btn-sm" disabled=move || remove.pending().get()
                        on:click=move |_| { remove.dispatch(()); }>"Retirer ce worker"</button>
                })}
            </div>
            {(!removable && can_manage).then(|| view! {
                <p class="form-help">"Seul un worker distant hors ligne peut être retiré."</p>
            })}
            <p class="form-help text-danger" role="status">{move || feedback.get()}</p>
        </div>
    }
}

#[component]
pub fn WorkersPage(me: Me) -> impl IntoView {
    let can_manage = me.allows(Permission::ManageWorkers);
    let can_libraries = me.allows(Permission::ViewLibraries);
    let can_jobs = me.allows(Permission::ViewJobs);
    let filters = use_context::<Filters>();
    let live_reload = use_context::<crate::jobs::LiveJobs>().map(|live| live.reload);
    let reload = RwSignal::new(0_u32);
    let workers = LocalResource::new(move || {
        reload.track();
        if let Some(live) = live_reload {
            live.track();
        }
        api::get::<Vec<Worker>>(WORKERS_PATH)
    });
    let libraries = LocalResource::new(move || {
        reload.track();
        async move {
            if can_libraries {
                api::get::<Vec<Library>>(LIBRARIES_PATH).await
            } else {
                Ok(Vec::new())
            }
        }
    });
    let jobs = LocalResource::new(move || {
        reload.track();
        if let Some(live) = live_reload {
            live.track();
        }
        async move {
            if can_jobs {
                api::get::<Vec<Job>>(JOBS_PATH).await.unwrap_or_default()
            } else {
                Vec::new()
            }
        }
    });
    let enrollment_reload = RwSignal::new(0_u32);
    let enrollment_state = LocalResource::new(move || {
        reload.track();
        enrollment_reload.track();
        async move {
            api::get::<WorkerEnrollmentList>(WORKER_REQUESTS_PATH)
                .await
                .unwrap_or_default()
        }
    });
    let polling_requests = RwSignal::new(false);
    Effect::new(move |_| {
        let open = enrollment_state.get().is_some_and(|list| list.window.open);
        if can_manage && open && !polling_requests.get_untracked() {
            polling_requests.set(true);
            leptos::task::spawn_local(async move {
                loop {
                    sleep(REQUESTS_POLL_MS).await;
                    let still_open = enrollment_state
                        .get_untracked()
                        .is_some_and(|list| list.window.open);
                    if !still_open {
                        break;
                    }
                    enrollment_reload.update(|c| *c = c.wrapping_add(1));
                }
                polling_requests.set(false);
            });
        }
    });
    let toggle_window = Action::new_local(move |open: &bool| {
        let body = EnrollmentWindowBody { open: *open };
        async move {
            api::post::<EnrollmentWindowBody, EnrollmentWindowStatus>(
                WORKER_ENROLLMENT_WINDOW_PATH,
                Some(&body),
            )
            .await
        }
    });
    Effect::new(move |_| match toggle_window.value().get() {
        Some(Ok(status)) => {
            if status.open {
                Toasts::notify_success("Fenêtre d'acceptation ouverte pour 15 minutes.");
            } else {
                Toasts::notify_success("Fenêtre d'acceptation fermée.");
            }
            enrollment_reload.update(|c| *c = c.wrapping_add(1));
        }
        Some(Err(error)) => Toasts::notify_error(error.message()),
        None => {}
    });
    let approve_request = Action::new_local(move |id: &Uuid| {
        let path = format!("{WORKER_REQUESTS_PATH}/{id}/approve");
        async move { api::post::<(), WorkerEnrollmentSummary>(&path, None).await }
    });
    Effect::new(move |_| match approve_request.value().get() {
        Some(Ok(summary)) => {
            Toasts::notify_success(format!(
                "Worker « {} » approuvé : il se connecte dans un instant.",
                summary.worker_name
            ));
            reload.update(|c| *c = c.wrapping_add(1));
        }
        Some(Err(error)) => Toasts::notify_error(error.message()),
        None => {}
    });
    let reject_request = Action::new_local(move |id: &Uuid| {
        let path = format!("{WORKER_REQUESTS_PATH}/{id}/reject");
        async move { api::post::<(), WorkerEnrollmentSummary>(&path, None).await }
    });
    Effect::new(move |_| match reject_request.value().get() {
        Some(Ok(summary)) => {
            Toasts::notify_success(format!("Demande de « {} » refusée.", summary.worker_name));
            enrollment_reload.update(|c| *c = c.wrapping_add(1));
        }
        Some(Err(error)) => Toasts::notify_error(error.message()),
        None => {}
    });

    let selected = RwSignal::new(None::<Uuid>);
    let detail_open = RwSignal::new(false);
    let enroll_open = RwSignal::new(false);
    if can_manage {
        provide_topbar_action(ActionSpec {
            label: "Ajouter un worker",
            on_click: Callback::new(move |()| enroll_open.set(true)),
        });
    }
    let remove_worker = Action::new_local(move |id: &Uuid| {
        let path = format!("{WORKERS_PATH}/{id}");
        async move { api::delete(&path).await }
    });
    Effect::new(move |_| match remove_worker.value().get() {
        Some(Ok(())) => {
            detail_open.set(false);
            Toasts::notify_success("Worker retiré.");
            reload.update(|count| *count = count.wrapping_add(1));
        }
        Some(Err(error)) => Toasts::notify_error(error.message()),
        None => {}
    });
    let on_removed = Callback::new(move |()| {
        detail_open.set(false);
        reload.update(|count| *count = count.wrapping_add(1));
    });
    let list = Signal::derive(move || workers.get().and_then(Result::ok).unwrap_or_default());
    let stats = Signal::derive(move || {
        let all = list.get();
        let jobs_list = jobs.get().unwrap_or_default();
        let summary = summarize_workers(&all, &jobs_list, Utc::now());
        let active = presence();
        let set_presence = move |next: Presence| {
            Callback::new(move |()| {
                if let Some(filters) = filters {
                    filters.presence.set(next);
                }
            })
        };
        let inactive = summary.offline + summary.silent;
        let inactive_tone = if summary.silent > 0 {
            MetricTone::Danger
        } else if summary.offline > 0 {
            MetricTone::Warning
        } else {
            MetricTone::Neutral
        };
        let mut offline_pill = StatItem::new("Hors ligne", inactive.to_string(), inactive_tone)
            .with_filter(active == Presence::Offline, set_presence(Presence::Offline));
        if summary.silent > 0 {
            offline_pill = offline_pill.with_detail(format!("{} silencieux", summary.silent));
        }
        let mut items = vec![
            StatItem::new("Tous", summary.total.to_string(), MetricTone::Neutral)
                .with_detail("central inclus")
                .with_filter(active == Presence::Any, set_presence(Presence::Any)),
            StatItem::new(
                "En ligne",
                summary.online.to_string(),
                if summary.online > 0 {
                    MetricTone::Success
                } else {
                    MetricTone::Neutral
                },
            )
            .with_filter(active == Presence::Online, set_presence(Presence::Online)),
            offline_pill,
        ];
        if can_jobs {
            let mut working_pill = StatItem::new(
                "En traitement",
                summary.busy_workers.to_string(),
                if summary.busy_workers > 0 {
                    MetricTone::Success
                } else {
                    MetricTone::Neutral
                },
            )
            .with_filter(active == Presence::Working, set_presence(Presence::Working));
            if summary.active_jobs > 0 {
                let job_word = if summary.active_jobs == 1 {
                    "tâche en cours"
                } else {
                    "tâches en cours"
                };
                working_pill =
                    working_pill.with_detail(format!("{} {job_word}", summary.active_jobs));
            }
            items.push(working_pill);
        }
        items
    });
    let selected_worker = Signal::derive(move || {
        let id = selected.get()?;
        list.get().into_iter().find(|worker| worker.id == id)
    });
    let grid = move || {
        let text = query();
        let online_filter = presence();
        let libraries_result = libraries.get();
        let libraries_failed = libraries_result.as_ref().is_some_and(Result::is_err);
        let libs = libraries_result.and_then(Result::ok);
        let jobs_list = jobs.get().unwrap_or_default();
        match workers.get() {
            None => view! {
                <div class="empty-state-panel">
                    <p class="text-dim">"Chargement des workers…"</p>
                </div>
            }
            .into_any(),
            Some(Err(error)) => view! {
                <div class="empty-state-panel">
                    <p class="form-error">{error.message()}</p>
                </div>
            }
            .into_any(),
            Some(Ok(all)) => {
                let now = Utc::now();
                let shown = filter_workers(all, &jobs_list, &text, online_filter, now);
                if shown.is_empty() {
                    let unfiltered = text.trim().is_empty() && online_filter == Presence::Any;
                    let empty_action = (can_manage && unfiltered)
                        .then(|| {
                            view! {
                                <button type="button" class="btn btn-primary" on:click=move |_| enroll_open.set(true)>
                                    <Icon name=IconName::Plus size=15 />
                                    <span>"Enrôler un worker"</span>
                                </button>
                            }
                        })
                        .into_any();
                    return view! {
                        <div class="empty-state-panel">
                            <EmptyState
                                title=if unfiltered {
                                    "Aucun worker disponible"
                                } else {
                                    "Aucun worker correspondant"
                                }
                                message=if unfiltered {
                                    "Enrôlez un nœud distant ou activez le worker intégré pour exécuter vos flux."
                                } else {
                                    "Ajustez votre recherche ou le filtre de présence pour retrouver un nœud."
                                }
                                icon_name=IconName::Workers
                                action=empty_action
                            />
                        </div>
                    }
                    .into_any();
                }
                view! {
                    <div class="resource-grid resource-grid--list">
                        {shown.into_iter().map(|worker| {
                            let id = worker.id;
                            let slot_total = worker.slots().total;
                            let liveness = worker.liveness(now);
                            let (badge_label, badge_variant) = liveness_badge(liveness);
                            let (role_label, role_tone) = role_meta(&worker);
                            let age = contact_age(&worker, now);
                            let last_contact = format::datetime(worker.last_seen);
                            let removable = can_manage && liveness != Liveness::Online && !worker.is_integrated();
                            let footer_hint = if worker.is_integrated() {
                                "Rattaché au central"
                            } else {
                                "Nœud distant"
                            };
                            let mounted_libs = libs.as_ref().map(|libraries|
                                worker_libraries(libraries, id, worker.is_integrated()).len());
                            let active_jobs = active_jobs_for(id, &jobs_list);
                            let libs_label = match mounted_libs {
                                Some(0) => "0 bibliothèque".to_owned(),
                                Some(1) => "1 bibliothèque".to_owned(),
                                Some(count) => format!("{count} bibliothèques"),
                                None if libraries_failed => "Bibliothèques indisponibles".to_owned(),
                                None => "Chargement des bibliothèques".to_owned(),
                            };
                            let jobs_label = match active_jobs {
                                0 => "Inactif".to_owned(),
                                1 => "1 tâche active".to_owned(),
                                count => format!("{count} tâches actives"),
                            };
                            let short_id: String = id.to_string().chars().take(8).collect();
                            let tone = match liveness {
                                Liveness::Online => MetricTone::Success,
                                Liveness::Offline => MetricTone::Warning,
                                Liveness::Silent => MetricTone::Danger,
                            };
                            let badge = view! {
                                <StatusBadge
                                    label=badge_label
                                    variant=badge_variant
                                    pulse=liveness == Liveness::Online
                                />
                            }
                            .into_any();
                            let open_detail = Callback::new(move |()| {
                                selected.set(Some(id));
                                detail_open.set(true);
                            });
                            let actions = view! {
                                {removable.then(|| view! {
                                    <button type="button" class="btn btn-sm btn-danger-quiet"
                                        disabled=move || remove_worker.pending().get()
                                        on:click=move |_| { remove_worker.dispatch(id); }>
                                        "Retirer"
                                    </button>
                                })}
                                <button type="button" class="btn btn-secondary btn-sm"
                                    on:click=move |_| { selected.set(Some(id)); detail_open.set(true); }>
                                    "Inspecter"
                                </button>
                            }
                            .into_any();
                            view! {
                                <ResourceCard
                                    title=worker.name
                                    subtitle=Some(format!("Dernier signal : {age}"))
                                    icon=IconName::Workers
                                    tone=tone
                                    badge=Some(badge)
                                    active=Some(Signal::derive(move || selected.get() == Some(id)))
                                    on_click=Some(open_detail)
                                    footer_meta=format!("{footer_hint} · {last_contact}")
                                    footer_actions=Some(actions)
                                >
                                    <div class="cluster cluster-wrap">
                                        <Tag label=role_label tone=role_tone />
                                        {(slot_total > 1).then(|| view! {
                                            <Tag label=format!("{slot_total}× simult.") tone=TagTone::Neutral />
                                        })}
                                        {worker.schedule.as_ref().filter(|s| s.enabled).map(|s| view! {
                                            <Tag label=format!("{:02}h–{:02}h", s.start_hour, s.end_hour) tone=TagTone::Mono />
                                        })}
                                        {can_libraries.then(|| view! { <Tag
                                            label=libs_label
                                            tone={if mounted_libs.is_some_and(|count| count > 0) { TagTone::Success } else { TagTone::Warning }}
                                            icon=Some(IconName::Libraries)
                                        /> })}
                                        <Tag
                                            label=jobs_label
                                            tone={if active_jobs > 0 { TagTone::Primary } else { TagTone::Neutral }}
                                            icon=Some(IconName::Activity)
                                        />
                                        {worker.version.clone().map(|v| view! { <Tag label=format!("v{v}") tone=TagTone::Mono /> })}
                                        {worker.remote_addr.clone().map(|addr| view! { <Tag label=addr tone=TagTone::Mono /> })}
                                        <Tag label=short_id tone=TagTone::Mono />
                                    </div>
                                    {worker.telemetry.as_ref().map(|t| view! {
                                        <p class="form-help worker-card-telemetry">{telemetry_summary(t)}</p>
                                    })}
                                </ResourceCard>
                            }
                        }).collect_view()}
                    </div>
                }
                .into_any()
            }
        }
    };
    let title = Signal::derive(move || {
        selected_worker
            .get()
            .map(|worker| worker.name)
            .unwrap_or_default()
    });
    let enrollment_panel = move || {
        let state = enrollment_state.get().unwrap_or_default();
        if !state.window.open && state.requests.is_empty() {
            return None;
        }
        let now = Utc::now();
        let window_text = match state.window.expires_at {
            Some(exp) if state.window.open => format!(
                "Fenêtre d'acceptation ouverte (expire dans {})",
                remaining_label(exp, now)
            ),
            _ => "Fenêtre d'acceptation fermée".to_owned(),
        };
        let reqs = state.requests;
        let header_actions = if can_manage {
            view! {
                <div class="cluster">
                    <button type="button" class="btn btn-secondary btn-sm"
                        disabled=move || toggle_window.pending().get()
                        on:click=move |_| { toggle_window.dispatch(true); }>
                        {if state.window.open { "Prolonger (15 min)" } else { "Ouvrir (15 min)" }}
                    </button>
                    {state.window.open.then(|| view! {
                        <button type="button" class="btn btn-secondary btn-sm"
                            disabled=move || toggle_window.pending().get()
                            on:click=move |_| { toggle_window.dispatch(false); }>
                            "Fermer"
                        </button>
                    })}
                </div>
            }
            .into_any()
        } else {
            ().into_any()
        };
        Some(view! {
            <section class="section-panel">
                <SectionHeader
                    title="Demandes en attente"
                    subtitle=window_text
                    actions=header_actions
                />
                <div class="registre">
                    {if reqs.is_empty() {
                        view! {
                            <p class="form-help">"Aucune demande reçue pour le moment. Sur le nouveau worker, choisissez « Enrôler un worker » et saisissez l'adresse de ce central."</p>
                        }.into_any()
                    } else {
                        view! {
                            <div class="table-wrapper">
                                <table class="data-table" aria-label="Demandes d'enrôlement en attente">
                                    <thead>
                                        <tr>
                                            <th scope="col">"Worker"</th>
                                            <th scope="col">"Adresse"</th>
                                            <th scope="col">"Version"</th>
                                            <th scope="col">"Expiration"</th>
                                            <th scope="col" class="cell-end">"Action"</th>
                                        </tr>
                                    </thead>
                                    <tbody>
                                        {reqs.into_iter().map(|req| {
                                            let req_id = req.id;
                                            let version_tag = req.worker_version.unwrap_or_else(|| "—".to_owned());
                                            let addr_tag = req.source_addr.unwrap_or_else(|| "—".to_owned());
                                            let exp_label = remaining_label(req.expires_at, now);
                                            view! {
                                                <tr>
                                                    <td class="text-strong">{req.worker_name}</td>
                                                    <td class="text-dim">{addr_tag}</td>
                                                    <td class="text-dim">{version_tag}</td>
                                                    <td class="text-dim">{format!("dans {exp_label}")}</td>
                                                    <td class="cell-end">
                                                        {can_manage.then(|| view! {
                                                            <div class="cluster">
                                                                <button type="button" class="btn btn-primary btn-sm"
                                                                    disabled=move || approve_request.pending().get() || reject_request.pending().get()
                                                                    on:click=move |_| { approve_request.dispatch(req_id); }>
                                                                    "Approuver"
                                                                </button>
                                                                <button type="button" class="btn btn-danger-quiet btn-sm"
                                                                    disabled=move || approve_request.pending().get() || reject_request.pending().get()
                                                                    on:click=move |_| { reject_request.dispatch(req_id); }>
                                                                    "Refuser"
                                                                </button>
                                                            </div>
                                                        })}
                                                    </td>
                                                </tr>
                                            }
                                        }).collect_view()}
                                    </tbody>
                                </table>
                            </div>
                        }.into_any()
                    }}
                </div>
            </section>
        })
    };
    view! {
        <div class="tab-panel" role="tabpanel">
            <StatStrip aria_label="Synthèse des workers" items=stats />
            {enrollment_panel}
            {grid}
            <Drawer open=detail_open eyebrow="Worker" title=title>
                {move || selected_worker.get().map(|worker| view! {
                    <WorkerDetail worker=worker can_manage=can_manage
                        libraries=libraries.get().and_then(Result::ok).filter(|_| can_libraries)
                        on_removed=on_removed />
                })}
                {move || (can_libraries && libraries.get().is_some_and(|result| result.is_err())).then(|| view! {
                    <p class="form-help" role="status">"Impossible de charger les bibliothèques de ce worker."</p>
                })}
            </Drawer>
            <Drawer open=enroll_open eyebrow="Infrastructure" title="Ajouter un worker">
                <div class="inspection-panel">
                    <div class="inspection-panel__section">
                        <p class="inspection-panel__title">"Enrôlement par approbation (recommandé)"</p>
                        <ol class="form-help">
                            <li>"Ouvrez la fenêtre d'acceptation ci-dessous (valable 15 minutes)."</li>
                            <li>"Démarrez un conteneur vierge, ouvrez son interface et choisissez « Enrôler un worker »."</li>
                            <li>"Saisissez l'adresse de ce central ("<code>{origin()}</code>") puis approuvez la demande affichée ici."</li>
                        </ol>
                        <div class="inspection-drawer__actions">
                            <button type="button" class="btn btn-primary"
                                disabled=move || toggle_window.pending().get()
                                on:click=move |_| { toggle_window.dispatch(true); }>
                                "Ouvrir la fenêtre d'acceptation (15 min)"
                            </button>
                        </div>
                        <pre class="code-block-wrapper"><code>{enroll_approval_compose()}</code></pre>
                    </div>
                    <details class="inspection-panel__section">
                        <summary class="form-help">"Méthode manuelle avec clé d'API (`compose` / `/config/worker_key`)"</summary>
                        <ol class="form-help">
                            <li>"Créez une clé d'API de rôle worker (Paramètres → Clés d'API) ; elle ne s'affiche qu'une fois."</li>
                            <li>"Enregistrez-la dans le fichier "<code>"/config/worker_key"</code>" du volume de configuration du worker."</li>
                            <li>"Démarrez le service suivant sur l'hôte distant :"</li>
                        </ol>
                        <pre class="code-block-wrapper"><code>{enroll_compose(&origin())}</code></pre>
                        <div class="inspection-drawer__actions">
                            <Link to=Route::Settings class="btn btn-secondary">
                                <Icon name=IconName::Key size=15 />
                                <span>"Ouvrir Paramètres & Clés d'API"</span>
                            </Link>
                        </div>
                    </details>
                </div>
            </Drawer>
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::filters::Presence;
    use forgearr_shared::workers::{
        INTEGRATED_WORKER_NAME, WORKER_ONLINE_SECONDS, WORKER_SILENT_ALERT_SECONDS,
    };

    #[test]
    fn compose_snippet_targets_this_central_without_any_secret() {
        let compose = enroll_compose("https://flux.example");
        assert!(compose.contains("CENTRAL_URL: https://flux.example"));
        assert!(compose.contains("WORKER_KEY_FILE: /config/worker_key"));
        assert!(!compose.contains("WORKER_KEY:"));
        let approval_compose = enroll_approval_compose();
        assert!(!approval_compose.contains("WORKER_KEY"));
        assert!(!approval_compose.contains("ADMIN"));
    }

    #[test]
    fn remaining_label_formats_positive_and_past_expirations() {
        let now = DateTime::<Utc>::UNIX_EPOCH + chrono::Duration::seconds(10_000);
        assert_eq!(
            remaining_label(now + chrono::Duration::seconds(900), now),
            "15 min"
        );
        assert_eq!(
            remaining_label(now - chrono::Duration::seconds(30), now),
            "0 s"
        );
    }

    #[test]
    fn badges_escalate_with_liveness_severity() {
        assert_eq!(liveness_badge(Liveness::Online), ("En ligne", "online"));
        assert_eq!(liveness_badge(Liveness::Offline), ("Hors ligne", "warning"));
        assert_eq!(liveness_badge(Liveness::Silent), ("Silencieux", "error"));
    }

    #[test]
    fn summarize_and_filter_workers_handle_all_liveness_states() {
        let now = DateTime::<Utc>::UNIX_EPOCH + chrono::Duration::seconds(10_000);
        let workers = vec![
            Worker {
                id: Uuid::from_u128(1),
                name: INTEGRATED_WORKER_NAME.into(),
                last_seen: now,
                version: None,
                remote_addr: None,
                telemetry: None,
                max_concurrency: 1,
                max_cpu_jobs: None,
                max_gpu_jobs: None,
                schedule: None,
                transfer_enabled: false,
            },
            Worker {
                id: Uuid::from_u128(2),
                name: "nas-offline".into(),
                last_seen: now - chrono::Duration::seconds(WORKER_ONLINE_SECONDS),
                version: None,
                remote_addr: None,
                telemetry: None,
                max_concurrency: 1,
                max_cpu_jobs: None,
                max_gpu_jobs: None,
                schedule: None,
                transfer_enabled: false,
            },
            Worker {
                id: Uuid::from_u128(3),
                name: "nas-silent".into(),
                last_seen: now - chrono::Duration::seconds(WORKER_SILENT_ALERT_SECONDS),
                version: None,
                remote_addr: None,
                telemetry: None,
                max_concurrency: 1,
                max_cpu_jobs: None,
                max_gpu_jobs: None,
                schedule: None,
                transfer_enabled: false,
            },
        ];
        let jobs = vec![Job {
            id: Uuid::nil(),
            source_name: "/media/film.mkv".into(),
            source_sha256: None,
            source_bytes: None,
            output_sha256: None,
            output_bytes: None,
            warning: false,
            target_worker_id: None,
            worker_id: Some(Uuid::from_u128(1)),
            status: JobStatus::Running,
            attempts: 1,
            lease_epoch: 1,
            lease_token: None,
            leased_until: None,
            output_name: None,
            error_code: None,
            library_id: None,
            flow_id: Some("f1".into()),
            flow_version: Some(1),
            priority: 0,
            created_at: now,
            updated_at: now,
        }];
        let summary = summarize_workers(&workers, &jobs, now);
        assert_eq!(
            summary,
            WorkerSummary {
                total: 3,
                online: 1,
                offline: 1,
                silent: 1,
                busy_workers: 1,
                active_jobs: 1,
            }
        );
        assert_eq!(contact_age(&workers[2], now), "il y a 10 min");

        assert_eq!(
            filter_workers(workers.clone(), &jobs, "", Presence::Any, now).len(),
            3
        );
        assert_eq!(
            filter_workers(workers.clone(), &jobs, "nas", Presence::Any, now).len(),
            2
        );
        assert_eq!(
            filter_workers(workers.clone(), &jobs, "", Presence::Online, now).len(),
            1
        );
        assert_eq!(
            filter_workers(workers.clone(), &jobs, "", Presence::Working, now).len(),
            1
        );
        assert_eq!(
            filter_workers(workers, &jobs, "silent", Presence::Offline, now).len(),
            1
        );
    }

    #[test]
    fn telemetry_summary_formats_cpu_ram_and_gpu_count() {
        let telemetry = forgearr_shared::workers::WorkerTelemetry {
            timestamp: Utc::now(),
            logical_cores: 8,
            cpu_usage_percent: 42.4,
            ram_total_bytes: 16 * 1024 * 1024 * 1024,
            ram_used_bytes: 4 * 1024 * 1024 * 1024,
            storage_total_bytes: 1_000_000_000,
            storage_available_bytes: 250_000_000,
            gpu_devices: vec!["Intel QuickSync".into()],
            capabilities: Default::default(),
        };
        assert_eq!(storage_used_bytes(&telemetry), 750_000_000);
        let summary = telemetry_summary(&telemetry);
        assert!(summary.contains("8 cœurs"));
        assert!(summary.contains("CPU 42 %"));
        assert!(summary.contains("RAM 25 %"));
        assert!(summary.contains("1 GPU"));
    }

    #[test]
    fn encoder_labels_lists_hardware_encoders() {
        use forgearr_shared::capabilities::{
            EncodeAcceleration, HardwareEncoder, VideoCodec, WorkerCapabilities,
        };
        let capabilities = WorkerCapabilities {
            gpu_models: Vec::new(),
            software_encoders: vec![VideoCodec::Av1],
            hardware_encoders: vec![HardwareEncoder {
                codec: VideoCodec::Hevc,
                acceleration: EncodeAcceleration::Nvenc,
            }],
        };
        assert_eq!(
            encoder_labels(&capabilities),
            ["H.265 · NVENC", "AV1 · CPU"]
        );
        assert!(encoder_labels(&WorkerCapabilities::default()).is_empty());
    }
}
