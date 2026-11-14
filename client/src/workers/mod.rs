//! Écran Workers : présence, worker intégré, retrait d'un worker et raccordement (ADR 0014).

use crate::ui::search::{matches, query};
use crate::ui::{Drawer, Registre, RegistreMessage, StatusBadge, SubNav, SubTab};
use crate::{api, format};
use chrono::Utc;
use flux_shared::Permission;
use flux_shared::session::Me;
use flux_shared::workers::{SystemConfig, Worker};
use leptos::prelude::*;

const WORKERS_PATH: &str = "/api/v1/workers";
const CONFIG_PATH: &str = "/api/v1/config";

/// Commande de raccordement d'un worker distant ; la clé se crée dans Paramètres → Clés d'API.
pub fn enroll_command(origin: &str) -> String {
    format!(
        "docker run -d --name flux-worker -e CENTRAL_URL={origin} \
-e WORKER_KEY_FILE=/config/worker_key -v flux-worker:/config -v /chemin/data:/data \
mrcryllix/flux-de-fichiers:latest worker"
    )
}

fn origin() -> String {
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
fn EnrollHelp(config: Option<SystemConfig>) -> impl IntoView {
    view! {
        <div class="inspection-panel">
            {config.map(|config| view! {
                <div class="detail-grid">
                    <div class="detail-term">"Version du central"</div><div class="detail-val">{config.version}</div>
                    <div class="detail-term">"Worker intégré"</div><div class="detail-val">{if config.local_worker { "Actif" } else { "Désactivé" }}</div>
                </div>
            })}
            <p class="form-help">
                "Créez une clé d'API de rôle worker (Paramètres → Clés d'API), enregistrez-la dans le fichier "
                <code>"worker_key"</code>" du volume de configuration, puis lancez sur la machine du worker :"
            </p>
            <pre class="code-block-wrapper"><code>{enroll_command(&origin())}</code></pre>
            <p class="form-help">"Vous pouvez aussi ouvrir l'adresse d'une installation vierge et choisir « Enrôler un worker »."</p>
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
    let config = LocalResource::new(|| api::get::<SystemConfig>(CONFIG_PATH));
    let selected = RwSignal::new(None::<uuid::Uuid>);
    let detail_open = RwSignal::new(false);
    let enroll_open = RwSignal::new(false);
    let tab = RwSignal::new(0_usize);
    let on_removed = Callback::new(move |()| {
        detail_open.set(false);
        reload.update(|count| *count = count.wrapping_add(1));
    });
    let list = Signal::derive(move || workers.get().and_then(Result::ok).unwrap_or_default());
    let selected_worker = Signal::derive(move || {
        let id = selected.get()?;
        list.get().into_iter().find(|worker| worker.id == id)
    });
    let rows = move || {
        let text = query();
        match workers.get() {
            None => view! { <RegistreMessage span=HEADERS.len() text="Chargement des workers…" /> }
                .into_any(),
            Some(Err(error)) => {
                view! { <RegistreMessage span=HEADERS.len() text=error.message() /> }.into_any()
            }
            Some(Ok(all)) => {
                let shown: Vec<Worker> = all
                    .into_iter()
                    .filter(|worker| matches(&[&worker.name], &text))
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
    let actions = can_manage.then(|| view! {
        <button type="button" class="btn btn-primary btn-sm" on:click=move |_| enroll_open.set(true)>"Raccorder un worker"</button>
    }.into_any());
    view! {
        <div class="tab-panel" role="tabpanel">
            <SubNav aria_label="Sous-navigation des workers" tabs=vec![SubTab::new("Workers", "🖥️")] selected=tab
                actions=actions.unwrap_or_else(|| ().into_any()) />
            <div class="registre">
                <Registre aria_label="Workers qui exécutent" headers=HEADERS.to_vec()>{rows}</Registre>
            </div>
            <Drawer open=detail_open eyebrow="Worker" title=title>
                {move || selected_worker.get().map(|worker| view! { <WorkerDetail worker=worker can_manage=can_manage on_removed=on_removed /> })}
            </Drawer>
            <Drawer open=enroll_open eyebrow="Workers" title="Raccorder un worker".to_owned()>
                {move || view! { <EnrollHelp config=config.get().and_then(Result::ok) /> }}
            </Drawer>
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enroll_command_targets_this_central_and_reads_the_key_from_a_file() {
        let command = enroll_command("https://flux.example");
        assert!(command.contains("CENTRAL_URL=https://flux.example"));
        assert!(command.contains("WORKER_KEY_FILE="));
        assert!(command.ends_with(" worker"));
        assert!(!command.contains("WORKER_KEY="));
    }
}
