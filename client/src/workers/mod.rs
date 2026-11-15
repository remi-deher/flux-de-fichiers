//! Écran Workers : présence, connectivité, worker intégré, retrait et enrôlement (ADR 0014).

use crate::route::{Link, Route};
use crate::settings::duration_label;
use crate::ui::filters::{Filters, Presence, presence};
use crate::ui::search::{matches, query};
use crate::ui::topbar_action::{ActionSpec, provide_topbar_action};
use crate::ui::{
    Drawer, EmptyState, Icon, IconName, MetricTone, ResourceCard, StatItem, StatStrip, StatusBadge,
    Tag, TagTone, Toasts,
};
use crate::{api, format};
use chrono::{DateTime, Utc};
use flux_shared::Permission;
use flux_shared::jobs::{Job, JobStatus};
use flux_shared::libraries::Library;
use flux_shared::session::Me;
use flux_shared::workers::{Liveness, Worker};
use leptos::prelude::*;
use uuid::Uuid;

const WORKERS_PATH: &str = "/api/v1/workers";
const LIBRARIES_PATH: &str = "/api/v1/libraries";
const JOBS_PATH: &str = "/api/v1/jobs?limit=200";

/// Service `compose` d'un worker distant ; la clé vient d'un fichier, jamais d'une valeur en clair.
pub fn enroll_compose(origin: &str) -> String {
    format!(
        "services:
  flux-worker:
    image: mrcryllix/flux-de-fichiers:latest
    command: worker
    restart: unless-stopped
    environment:
      CENTRAL_URL: {origin}
      WORKER_KEY_FILE: /config/worker_key
    volumes:
      - flux-worker:/config
      - /chemin/data:/data
volumes:
  flux-worker:
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
    pub remote: usize,
    pub offline: usize,
    pub silent: usize,
}

pub fn summarize_workers(workers: &[Worker], now: DateTime<Utc>) -> WorkerSummary {
    let total = workers.len();
    let mut online = 0_usize;
    let mut remote = 0_usize;
    let mut offline = 0_usize;
    let mut silent = 0_usize;
    for worker in workers {
        if !worker.is_integrated() {
            remote += 1;
        }
        match worker.liveness(now) {
            Liveness::Online => online += 1,
            Liveness::Offline => offline += 1,
            Liveness::Silent => silent += 1,
        }
    }
    WorkerSummary {
        total,
        online,
        remote,
        offline,
        silent,
    }
}

/// Filtre les workers par recherche textuelle (`nom`, `id`) et critère de présence.
pub fn filter_workers(
    workers: Vec<Worker>,
    text: &str,
    online_filter: crate::ui::filters::Presence,
    now: DateTime<Utc>,
) -> Vec<Worker> {
    workers
        .into_iter()
        .filter(|worker| {
            matches(&[&worker.name, &worker.id.to_string()], text)
                && online_filter.accepts(worker.is_online(now))
        })
        .collect()
}

fn storage_used_bytes(telemetry: &flux_shared::workers::WorkerTelemetry) -> u64 {
    telemetry
        .storage_total_bytes
        .saturating_sub(telemetry.storage_available_bytes)
}

fn telemetry_summary(telemetry: &flux_shared::workers::WorkerTelemetry) -> String {
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
fn WorkerTelemetrySection(telemetry: flux_shared::workers::WorkerTelemetry) -> impl IntoView {
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
            {(!gpus.is_empty()).then(|| view! {
                <div class="cluster cluster-wrap">
                    {gpus.into_iter().map(|gpu| view! {
                        <Tag label=gpu tone=TagTone::Mono />
                    }).collect_view()}
                </div>
            })}
        </div>
    }
}

#[component]
fn WorkerDetail(worker: Worker, can_manage: bool, on_removed: Callback<()>) -> impl IntoView {
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
            </div>
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
                api::get::<Vec<Library>>(LIBRARIES_PATH)
                    .await
                    .unwrap_or_default()
            } else {
                Vec::new()
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
    let selected = RwSignal::new(None::<Uuid>);
    let detail_open = RwSignal::new(false);
    let enroll_open = RwSignal::new(false);
    if can_manage {
        provide_topbar_action(ActionSpec {
            label: "Enrôler un worker",
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
        let summary = summarize_workers(&all, Utc::now());
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
        let mut inactive_pill = StatItem::new(
            "Hors ligne / Silencieux",
            inactive.to_string(),
            inactive_tone,
        )
        .with_filter(active == Presence::Offline, set_presence(Presence::Offline));
        if summary.silent > 0 {
            inactive_pill = inactive_pill.with_detail(format!("{} silencieux", summary.silent));
        }
        vec![
            StatItem::new(
                "Workers déclarés",
                summary.total.to_string(),
                MetricTone::Neutral,
            )
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
            StatItem::new(
                "Nœuds distants",
                summary.remote.to_string(),
                MetricTone::Neutral,
            ),
            inactive_pill,
        ]
    });
    let selected_worker = Signal::derive(move || {
        let id = selected.get()?;
        list.get().into_iter().find(|worker| worker.id == id)
    });
    let grid = move || {
        let text = query();
        let online_filter = presence();
        let libs = libraries.get().unwrap_or_default();
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
                let shown = filter_workers(all, &text, online_filter, now);
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
                    <div class="resource-grid">
                        {shown.into_iter().map(|worker| {
                            let id = worker.id;
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
                            let mounted_libs = libs
                                .iter()
                                .filter(|lib| lib.workers.iter().any(|entry| entry.worker_id == id))
                                .count();
                            let active_jobs = jobs_list
                                .iter()
                                .filter(|job| {
                                    job.worker_id == Some(id)
                                        && matches!(job.status, JobStatus::Running | JobStatus::Queued)
                                })
                                .count();
                            let libs_label = match mounted_libs {
                                0 => "0 bibliothèque".to_owned(),
                                1 => "1 bibliothèque".to_owned(),
                                count => format!("{count} bibliothèques"),
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
                                        <Tag
                                            label=libs_label
                                            tone={if mounted_libs > 0 { TagTone::Success } else { TagTone::Warning }}
                                            icon=Some(IconName::Libraries)
                                        />
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
    view! {
        <div class="tab-panel" role="tabpanel">
            <StatStrip aria_label="Synthèse des workers" items=stats />
            {grid}
            <Drawer open=detail_open eyebrow="Worker" title=title>
                {move || selected_worker.get().map(|worker| view! { <WorkerDetail worker=worker can_manage=can_manage on_removed=on_removed /> })}
            </Drawer>
            <Drawer open=enroll_open eyebrow="Infrastructure" title="Enrôler un worker">
                <div class="inspection-panel">
                    <ol class="form-help">
                        <li>"Créez une clé d'API de rôle worker (Paramètres → Clés d'API) ; elle ne s'affiche qu'une fois."</li>
                        <li>"Enregistrez-la dans le fichier "<code>"/config/worker_key"</code>" du volume de configuration du worker."</li>
                        <li>"Démarrez le service suivant sur l'hôte distant :"</li>
                    </ol>
                    <pre class="code-block-wrapper"><code>{enroll_compose(&origin())}</code></pre>
                    <p class="form-help">
                        "Autre voie (ADR 0011) : ouvrir l'adresse d'une installation vierge, choisir « Enrôler un worker » et saisir l'adresse du central avec un compte administrateur."
                    </p>
                    <div class="inspection-drawer__actions">
                        <Link to=Route::Settings class="btn btn-secondary">
                            <Icon name=IconName::Key size=15 />
                            <span>"Ouvrir Paramètres & Clés d'API"</span>
                        </Link>
                    </div>
                </div>
            </Drawer>
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::filters::Presence;
    use flux_shared::workers::{
        INTEGRATED_WORKER_NAME, WORKER_ONLINE_SECONDS, WORKER_SILENT_ALERT_SECONDS,
    };

    #[test]
    fn compose_snippet_targets_this_central_without_any_secret() {
        let compose = enroll_compose("https://flux.example");
        assert!(compose.contains("CENTRAL_URL: https://flux.example"));
        assert!(compose.contains("WORKER_KEY_FILE: /config/worker_key"));
        assert!(!compose.contains("WORKER_KEY:"));
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
            },
            Worker {
                id: Uuid::from_u128(2),
                name: "nas-offline".into(),
                last_seen: now - chrono::Duration::seconds(WORKER_ONLINE_SECONDS),
                version: None,
                remote_addr: None,
                telemetry: None,
            },
            Worker {
                id: Uuid::from_u128(3),
                name: "nas-silent".into(),
                last_seen: now - chrono::Duration::seconds(WORKER_SILENT_ALERT_SECONDS),
                version: None,
                remote_addr: None,
                telemetry: None,
            },
        ];
        let summary = summarize_workers(&workers, now);
        assert_eq!(
            summary,
            WorkerSummary {
                total: 3,
                online: 1,
                remote: 2,
                offline: 1,
                silent: 1,
            }
        );
        assert_eq!(contact_age(&workers[2], now), "il y a 10 min");

        assert_eq!(
            filter_workers(workers.clone(), "", Presence::Any, now).len(),
            3
        );
        assert_eq!(
            filter_workers(workers.clone(), "nas", Presence::Any, now).len(),
            2
        );
        assert_eq!(
            filter_workers(workers.clone(), "", Presence::Online, now).len(),
            1
        );
        assert_eq!(
            filter_workers(workers, "silent", Presence::Offline, now).len(),
            1
        );
    }

    #[test]
    fn telemetry_summary_formats_cpu_ram_and_gpu_count() {
        let telemetry = flux_shared::workers::WorkerTelemetry {
            timestamp: Utc::now(),
            logical_cores: 8,
            cpu_usage_percent: 42.4,
            ram_total_bytes: 16 * 1024 * 1024 * 1024,
            ram_used_bytes: 4 * 1024 * 1024 * 1024,
            storage_total_bytes: 1_000_000_000,
            storage_available_bytes: 250_000_000,
            gpu_devices: vec!["Intel QuickSync".into()],
        };
        assert_eq!(storage_used_bytes(&telemetry), 750_000_000);
        let summary = telemetry_summary(&telemetry);
        assert!(summary.contains("8 cœurs"));
        assert!(summary.contains("CPU 42 %"));
        assert!(summary.contains("RAM 25 %"));
        assert!(summary.contains("1 GPU"));
    }
}
