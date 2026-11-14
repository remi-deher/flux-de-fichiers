//! Connectivité des workers : dernier contact, ancienneté, alerte de silence et raccordement.

use super::system::duration_label;
use crate::ui::{Drawer, Registre, RegistreMessage, SectionHeader, StatusBadge};
use crate::workers::{enroll_compose, origin};
use crate::{api, format};
use chrono::{DateTime, Utc};
use flux_shared::workers::{Liveness, Worker};
use leptos::prelude::*;

const WORKERS_PATH: &str = "/api/v1/workers";
const HEADERS: [&str; 5] = [
    "Worker",
    "Rôle",
    "Connectivité",
    "Dernier contact",
    "Ancienneté",
];

fn liveness_badge(liveness: Liveness) -> (&'static str, &'static str) {
    match liveness {
        Liveness::Online => ("En ligne", "online"),
        Liveness::Offline => ("Hors ligne", "warning"),
        Liveness::Silent => ("Silencieux", "error"),
    }
}

/// Ancienneté du dernier contact, lisible.
fn contact_age(worker: &Worker, now: DateTime<Utc>) -> String {
    format!("il y a {}", duration_label(worker.silence_seconds(now)))
}

#[component]
fn EnrollSteps() -> impl IntoView {
    view! {
        <div class="inspection-panel">
            <ol class="form-help">
                <li>"Créez une clé d'API de rôle worker (Paramètres → Clés d'API) ; elle ne s'affiche qu'une fois."</li>
                <li>"Enregistrez-la dans le fichier "<code>"worker_key"</code>" du volume de configuration du worker."</li>
                <li>"Lancez ce service sur la machine du worker :"</li>
            </ol>
            <pre class="code-block-wrapper"><code>{enroll_compose(&origin())}</code></pre>
            <p class="form-help">
                "Autre voie (ADR 0011) : ouvrir l'adresse d'une installation vierge, choisir « Enrôler un worker » et saisir l'adresse du central avec un compte administrateur. Aucun secret n'est affiché ici."
            </p>
        </div>
    }
}

#[component]
pub fn WorkersConnectivityCard() -> impl IntoView {
    let workers = LocalResource::new(|| api::get::<Vec<Worker>>(WORKERS_PATH));
    let enroll_open = RwSignal::new(false);
    let rows = move || match workers.get() {
        None => view! { <RegistreMessage span=HEADERS.len() text="Chargement des workers…" /> }
            .into_any(),
        Some(Err(error)) => {
            view! { <RegistreMessage span=HEADERS.len() text=error.message() /> }.into_any()
        }
        Some(Ok(list)) if list.is_empty() => {
            view! { <RegistreMessage span=HEADERS.len() text="Aucun worker." /> }.into_any()
        }
        Some(Ok(list)) => {
            let now = Utc::now();
            list.into_iter().map(|worker| {
                let liveness = worker.liveness(now);
                let (label, variant) = liveness_badge(liveness);
                view! {
                    <tr class="registre-row">
                        <td class="text-strong">{worker.name.clone()}</td>
                        <td>{if worker.is_integrated() { "Intégré au central" } else { "Distant" }}</td>
                        <td><StatusBadge label=label variant=variant pulse=liveness == Liveness::Online /></td>
                        <td class="text-dim">{format::datetime(worker.last_seen)}</td>
                        <td class:text-danger=liveness == Liveness::Silent>{contact_age(&worker, now)}</td>
                    </tr>
                }
            }).collect_view().into_any()
        }
    };
    let actions = view! {
        <button type="button" class="btn btn-primary btn-sm" on:click=move |_| enroll_open.set(true)>"Ajouter un worker"</button>
    }.into_any();
    view! {
        <div class="settings-section">
            <SectionHeader title="Connectivité des workers"
                subtitle="Dernier contact reçu par le central ; un worker distant silencieux depuis plus de 10 minutes est signalé."
                actions=actions />
            <Registre aria_label="Connectivité des workers" headers=HEADERS.to_vec()>{rows}</Registre>
            <Drawer open=enroll_open eyebrow="Workers" title="Ajouter un worker".to_owned()>
                {move || view! { <EnrollSteps /> }}
            </Drawer>
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use flux_shared::workers::WORKER_SILENT_ALERT_SECONDS;
    use uuid::Uuid;

    #[test]
    fn badges_escalate_in_severity() {
        assert_eq!(liveness_badge(Liveness::Online).1, "online");
        assert_eq!(liveness_badge(Liveness::Offline).1, "warning");
        assert_eq!(liveness_badge(Liveness::Silent).1, "error");
    }

    #[test]
    fn contact_age_reads_the_silence() {
        let now = DateTime::<Utc>::UNIX_EPOCH + chrono::Duration::seconds(10_000);
        let worker = Worker {
            id: Uuid::nil(),
            name: "nas".into(),
            last_seen: now - chrono::Duration::seconds(WORKER_SILENT_ALERT_SECONDS),
        };
        assert_eq!(contact_age(&worker, now), "il y a 10 min");
    }
}
