//! Écran Workers : présence, connectivité, worker intégré, retrait et enrôlement (ADR 0014).

use crate::route::{Link, Route};
use crate::settings::duration_label;
use crate::ui::filters::presence;
use crate::ui::search::{matches, query};
use crate::ui::topbar_action::{ActionSpec, provide_topbar_action};
use crate::ui::{
    Drawer, EmptyState, Icon, IconName, MetricTone, StatItem, StatStrip, StatusBadge, Toasts,
};
use crate::{api, format};
use chrono::{DateTime, Utc};
use flux_shared::Permission;
use flux_shared::session::Me;
use flux_shared::workers::{Liveness, Worker};
use leptos::prelude::*;
use uuid::Uuid;

const WORKERS_PATH: &str = "/api/v1/workers";

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

fn role_meta(worker: &Worker) -> (&'static str, &'static str) {
    if worker.is_integrated() {
        ("Intégré au central", "repo-badge official")
    } else {
        ("Worker distant", "repo-badge community")
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

#[component]
fn WorkerDetail(worker: Worker, can_manage: bool, on_removed: Callback<()>) -> impl IntoView {
    let now = Utc::now();
    let liveness = worker.liveness(now);
    let (badge_label, badge_variant) = liveness_badge(liveness);
    let (role_label, role_class) = role_meta(&worker);
    let age = contact_age(&worker, now);
    let feedback = RwSignal::new(None::<String>);
    let id = worker.id;
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
                <span class=role_class>{role_label}</span>
            </div>
            <div class="detail-grid">
                <div class="detail-term">"Identifiant"</div><div class="detail-val text-mono">{worker.id.to_string()}</div>
                <div class="detail-term">"Rôle"</div><div class="detail-val">{role_label}</div>
                <div class="detail-term">"Dernier contact"</div><div class="detail-val">{format::datetime(worker.last_seen)}</div>
                <div class="detail-term">"Ancienneté"</div>
                <div class="detail-val" class:text-danger=liveness == Liveness::Silent>{age}</div>
            </div>
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
    let reload = RwSignal::new(0_u32);
    let workers = LocalResource::new(move || {
        reload.track();
        api::get::<Vec<Worker>>(WORKERS_PATH)
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
        );
        if summary.silent > 0 {
            inactive_pill = inactive_pill.with_detail(format!("{} silencieux", summary.silent));
        }
        vec![
            StatItem::new(
                "Workers déclarés",
                summary.total.to_string(),
                MetricTone::Neutral,
            )
            .with_detail("central inclus"),
            StatItem::new(
                "En ligne",
                summary.online.to_string(),
                if summary.online > 0 {
                    MetricTone::Success
                } else {
                    MetricTone::Neutral
                },
            ),
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
                    let empty_action = (can_manage && text.trim().is_empty())
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
                                title=if text.trim().is_empty() {
                                    "Aucun worker disponible"
                                } else {
                                    "Aucun worker correspondant"
                                }
                                message=if text.trim().is_empty() {
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
                    <div class="workers-grid">
                        {shown.into_iter().map(|worker| {
                            let id = worker.id;
                            let liveness = worker.liveness(now);
                            let (badge_label, badge_variant) = liveness_badge(liveness);
                            let (role_label, role_class) = role_meta(&worker);
                            let age = contact_age(&worker, now);
                            let last_contact = format::datetime(worker.last_seen);
                            let removable = can_manage && liveness != Liveness::Online && !worker.is_integrated();
                            let footer_hint = if worker.is_integrated() {
                                "Rattaché au central"
                            } else {
                                "Connecté au central"
                            };
                            view! {
                                <article
                                    class="worker-card"
                                    class:is-open=move || selected.get() == Some(id)
                                >
                                    <div class="worker-card__head">
                                        <div class="worker-card__title-wrap">
                                            <span class="worker-card__icon" aria-hidden="true">
                                                <Icon name=IconName::Workers size=17 />
                                            </span>
                                            <div class="worker-card__heading">
                                                <h2 class="worker-card__title">{worker.name}</h2>
                                                <span class=role_class>{role_label}</span>
                                            </div>
                                        </div>
                                        <StatusBadge
                                            label=badge_label
                                            variant=badge_variant
                                            pulse=liveness == Liveness::Online
                                        />
                                    </div>
                                    <div class="worker-card__body">
                                        <div class="worker-card__meta-row">
                                            <span class="worker-card__meta-label">"Identifiant"</span>
                                            <code class="code-badge truncate">{id.to_string()}</code>
                                        </div>
                                        <div class="worker-card__meta-row">
                                            <span class="worker-card__meta-label">"Dernier contact"</span>
                                            <span class="text-sm">{last_contact}</span>
                                        </div>
                                        <div class="worker-card__meta-row">
                                            <span class="worker-card__meta-label">"Ancienneté"</span>
                                            <span class="text-sm" class:text-danger=liveness == Liveness::Silent>{age}</span>
                                        </div>
                                    </div>
                                    <div class="worker-card__foot">
                                        {if removable {
                                            view! {
                                                <button type="button" class="btn btn-sm btn-danger-quiet"
                                                    disabled=move || remove_worker.pending().get()
                                                    on:click=move |_| { remove_worker.dispatch(id); }>
                                                    "Retirer"
                                                </button>
                                            }.into_any()
                                        } else {
                                            view! { <span class="text-dim text-xs">{footer_hint}</span> }.into_any()
                                        }}
                                        <button type="button" class="btn btn-secondary btn-sm"
                                            on:click=move |_| { selected.set(Some(id)); detail_open.set(true); }>
                                            "Inspecter"
                                        </button>
                                    </div>
                                </article>
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
            },
            Worker {
                id: Uuid::from_u128(2),
                name: "nas-offline".into(),
                last_seen: now - chrono::Duration::seconds(WORKER_ONLINE_SECONDS),
            },
            Worker {
                id: Uuid::from_u128(3),
                name: "nas-silent".into(),
                last_seen: now - chrono::Duration::seconds(WORKER_SILENT_ALERT_SECONDS),
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
}
