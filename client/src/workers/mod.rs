//! Écran Workers : présence, worker intégré, retrait d'un worker et raccordement (ADR 0014).

use crate::route::{Link, Route};
use crate::ui::filters::presence;
use crate::ui::search::{matches, query};
use crate::ui::topbar_action::{ActionSpec, provide_topbar_action};
use crate::ui::{
    Drawer, Icon, IconName, MetricTone, PageHeader, Registre, RegistreMessage, StatItem, StatStrip,
    StatusBadge,
};
use crate::{api, format};
use chrono::{DateTime, Utc};
use flux_shared::Permission;
use flux_shared::session::Me;
use flux_shared::workers::Worker;
use leptos::prelude::*;

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

const HEADERS: [&str; 5] = ["Worker", "Rôle", "Présence", "Dernier contact", "Action"];

fn role_label(worker: &Worker) -> &'static str {
    if worker.is_integrated() {
        "Intégré au central"
    } else {
        "Distant"
    }
}

/// Synthèse `(total, en_ligne, hors_ligne)` des workers à l'instant `now`.
pub fn worker_counts(workers: &[Worker], now: DateTime<Utc>) -> (usize, usize, usize) {
    let total = workers.len();
    let online = workers
        .iter()
        .filter(|worker| worker.is_online(now))
        .count();
    (total, online, total.saturating_sub(online))
}

#[component]
fn WorkerDetail(worker: Worker, can_manage: bool, on_removed: Callback<()>) -> impl IntoView {
    let online = worker.is_online(Utc::now());
    let feedback = RwSignal::new(None::<String>);
    let id = worker.id;
    let remove = Action::new_local(move |_: &()| async move {
        api::delete(&format!("{WORKERS_PATH}/{id}")).await
    });
    Effect::new(move |_| match remove.value().get() {
        Some(Ok(())) => on_removed.run(()),
        Some(Err(error)) => feedback.set(Some(error.message())),
        None => {}
    });
    let removable = can_manage && !online && !worker.is_integrated();
    view! {
        <div class="inspection-panel">
            <StatusBadge label=if online { "En ligne" } else { "Hors ligne" } variant=if online { "online" } else { "offline" } pulse=online />
            <div class="detail-grid">
                <div class="detail-term">"Identifiant"</div><div class="detail-val text-mono">{worker.id.to_string()}</div>
                <div class="detail-term">"Rôle"</div><div class="detail-val">{role_label(&worker)}</div>
                <div class="detail-term">"Dernier contact"</div><div class="detail-val">{format::datetime(worker.last_seen)}</div>
            </div>
            <div class="inspection-drawer__actions">
                {removable.then(|| view! {
                    <button type="button" class="btn btn-danger-quiet" disabled=move || remove.pending().get()
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
    let selected = RwSignal::new(None::<uuid::Uuid>);
    let detail_open = RwSignal::new(false);
    let enroll_open = RwSignal::new(false);
    if can_manage {
        provide_topbar_action(ActionSpec {
            label: "Enrôler un worker",
            on_click: Callback::new(move |()| enroll_open.set(true)),
        });
    }
    let on_removed = Callback::new(move |()| {
        detail_open.set(false);
        reload.update(|count| *count = count.wrapping_add(1));
    });
    let list = Signal::derive(move || workers.get().and_then(Result::ok).unwrap_or_default());
    let stats = Signal::derive(move || {
        let all = list.get();
        let (total, online, offline) = worker_counts(&all, Utc::now());
        vec![
            StatItem::new("Total", total.to_string(), MetricTone::Neutral),
            StatItem::new("En ligne", online.to_string(), MetricTone::Success),
            StatItem::new(
                "Hors ligne",
                offline.to_string(),
                if offline > 0 {
                    MetricTone::Warning
                } else {
                    MetricTone::Neutral
                },
            ),
        ]
    });
    let selected_worker = Signal::derive(move || {
        let id = selected.get()?;
        list.get().into_iter().find(|worker| worker.id == id)
    });
    let rows = move || {
        let text = query();
        let presence = presence();
        match workers.get() {
            None => view! { <RegistreMessage span=HEADERS.len() text="Chargement des workers…" /> }
                .into_any(),
            Some(Err(error)) => {
                view! { <RegistreMessage span=HEADERS.len() text=error.message() /> }.into_any()
            }
            Some(Ok(all)) => {
                let shown: Vec<Worker> = all
                    .into_iter()
                    .filter(|worker| {
                        matches(&[&worker.name], &text)
                            && presence.accepts(worker.is_online(Utc::now()))
                    })
                    .collect();
                if shown.is_empty() {
                    return view! { <RegistreMessage span=HEADERS.len() text="Aucun worker." /> }
                        .into_any();
                }
                shown.into_iter().map(|worker| {
                    let id = worker.id;
                    let online = worker.is_online(Utc::now());
                    view! {
                        <tr class="registre-row" class:is-open=move || selected.get() == Some(id)>
                            <td class="text-strong">{worker.name.clone()}</td>
                            <td>{role_label(&worker)}</td>
                            <td><StatusBadge label=if online { "En ligne" } else { "Hors ligne" } variant=if online { "online" } else { "offline" } pulse=online /></td>
                            <td class="text-dim">{format::datetime(worker.last_seen)}</td>
                            <td class="cell-end">
                                <button type="button" class="btn btn-secondary btn-sm registre-open-btn"
                                    on:click=move |_| { selected.set(Some(id)); detail_open.set(true); }>"Ouvrir"</button>
                            </td>
                        </tr>
                    }
                }).collect_view().into_any()
            }
        }
    };
    let title = Signal::derive(move || {
        selected_worker
            .get()
            .map(|worker| worker.name)
            .unwrap_or_default()
    });
    let header_actions = can_manage
        .then(|| {
            view! {
                <button type="button" class="btn btn-primary" on:click=move |_| enroll_open.set(true)>
                    <Icon name=IconName::Plus size=15 />
                    <span>"Enrôler un worker"</span>
                </button>
            }
        })
        .into_any();
    view! {
        <div class="tab-panel" role="tabpanel">
            <PageHeader
                eyebrow="Infrastructure"
                title="Workers"
                subtitle="Nœuds de traitement enregistrés, état de présence et enrôlement."
                icon=IconName::Workers
                actions=header_actions
            />
            <StatStrip aria_label="Synthèse des workers" items=stats />
            <div class="registre">
                <Registre aria_label="Workers qui exécutent" headers=HEADERS.to_vec()>{rows}</Registre>
            </div>
            <Drawer open=detail_open eyebrow="Worker" title=title>
                {move || selected_worker.get().map(|worker| view! { <WorkerDetail worker=worker can_manage=can_manage on_removed=on_removed /> })}
            </Drawer>
            <Drawer open=enroll_open eyebrow="Infrastructure" title="Enrôler un worker">
                <div class="inspection-panel">
                    <p class="form-help">
                        "Créez d'abord une clé d'API de rôle worker dans les paramètres, enregistrez-la dans le fichier "
                        <code>"/config/worker_key"</code>
                        " puis démarrez le service suivant :"
                    </p>
                    <pre class="code-block-wrapper"><code>{enroll_compose(&origin())}</code></pre>
                    <div class="inspection-drawer__actions">
                        <Link to=Route::Settings class="btn btn-secondary">
                            <Icon name=IconName::Key size=15 />
                            <span>"Gérer les clés d'API"</span>
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

    #[test]
    fn compose_snippet_targets_this_central_without_any_secret() {
        let compose = enroll_compose("https://flux.example");
        assert!(compose.contains("CENTRAL_URL: https://flux.example"));
        assert!(compose.contains("WORKER_KEY_FILE: /config/worker_key"));
        assert!(!compose.contains("WORKER_KEY:"));
    }

    #[test]
    fn worker_counts_splits_online_and_offline_workers() {
        let now = Utc::now();
        let workers = [
            Worker {
                id: uuid::Uuid::nil(),
                name: "central".into(),
                last_seen: now,
            },
            Worker {
                id: uuid::Uuid::from_u128(2),
                name: "stale".into(),
                last_seen: now - chrono::Duration::hours(2),
            },
        ];
        assert_eq!(worker_counts(&[], now), (0, 0, 0));
        assert_eq!(worker_counts(&workers, now), (2, 1, 1));
    }
}
