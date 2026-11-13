//! Écran Workers : présence, worker intégré, retrait d'un worker et raccordement (ADR 0014).

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

#[component]
fn WorkerRow(worker: Worker, can_manage: bool, on_removed: Callback<()>) -> impl IntoView {
    let online = worker.is_online(Utc::now());
    let role = if worker.is_integrated() {
        "Intégré au central"
    } else {
        "Distant"
    };
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
        <tr>
            <td class="text-strong">{worker.name.clone()}</td>
            <td>{role}</td>
            <td>
                <span class=if online { "status-pill success" } else { "status-pill danger" }>
                    {if online { "En ligne" } else { "Hors ligne" }}
                </span>
            </td>
            <td class="text-dim">{format::datetime(worker.last_seen)}</td>
            <td class="cell-end">
                {removable.then(|| view! {
                    <button
                        type="button"
                        class="btn btn-sm btn-danger-quiet"
                        disabled=move || remove.pending().get()
                        on:click=move |_| { remove.dispatch(()); }
                    >
                        "Retirer"
                    </button>
                })}
                <span class="text-danger text-xs" role="status">{move || feedback.get()}</span>
            </td>
        </tr>
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
    let on_removed = Callback::new(move |()| reload.update(|count| *count = count.wrapping_add(1)));

    let rows = move || {
        match workers.get() {
        None => view! { <tr><td colspan="5" class="text-dim">"Chargement…"</td></tr> }.into_any(),
        Some(Err(error)) => view! { <tr><td colspan="5" class="text-danger">{error.message()}</td></tr> }.into_any(),
        Some(Ok(list)) if list.is_empty() => {
            view! { <tr><td colspan="5" class="text-dim">"Aucun worker enregistré."</td></tr> }.into_any()
        }
        Some(Ok(list)) => list
            .into_iter()
            .map(|worker| view! { <WorkerRow worker=worker can_manage=can_manage on_removed=on_removed /> })
            .collect_view()
            .into_any(),
    }
    };
    let config_facts = move || {
        config.get().and_then(Result::ok).map(|config| view! {
            <div class="detail-grid">
                <div class="detail-term">"Version du central"</div>
                <div class="detail-val">{config.version}</div>
                <div class="detail-term">"Worker intégré"</div>
                <div class="detail-val">{if config.local_worker { "Actif" } else { "Désactivé" }}</div>
            </div>
        })
    };

    view! {
        <div class="registre">
            <h1 class="section-title">"Workers"</h1>
            {config_facts}
            <div class="table-wrapper">
                <table class="data-table" aria-label="Workers">
                    <thead>
                        <tr>
                            <th scope="col">"Worker"</th>
                            <th scope="col">"Rôle"</th>
                            <th scope="col">"Présence"</th>
                            <th scope="col">"Dernier contact"</th>
                            <th scope="col" class="cell-end">"Action"</th>
                        </tr>
                    </thead>
                    <tbody>{rows}</tbody>
                </table>
            </div>
            {can_manage.then(|| view! {
                <section class="section-panel" aria-labelledby="enroll-title">
                    <h2 id="enroll-title" class="section-title">"Raccorder un worker"</h2>
                    <p class="form-help">
                        "Créez une clé d'API de rôle worker (Paramètres → Clés d'API), enregistrez-la dans le fichier "
                        <code>"worker_key"</code>
                        " du volume de configuration, puis lancez sur la machine du worker :"
                    </p>
                    <pre class="code-block-wrapper"><code>{enroll_command(&origin())}</code></pre>
                    <p class="form-help">
                        "Vous pouvez aussi ouvrir l'adresse d'une installation vierge et choisir « Worker »."
                    </p>
                </section>
            })}
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
