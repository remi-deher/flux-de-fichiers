//! Formulaires d'accès : choix du rôle, installation du central ou d'un worker, connexion.

use super::{Gate, access_error, identity_after};
use crate::api;
use crate::sleep::sleep;
use flux_shared::InstanceKind;
use flux_shared::accounts::LoginRequest;
use flux_shared::session::{Me, SessionInfo};
use flux_shared::system::{SetupRequest, SetupStatus, SetupWorkerRequest};
use leptos::prelude::*;

const SETUP_CENTRAL_PATH: &str = "/api/v1/setup/central";
const SETUP_WORKER_PATH: &str = "/api/v1/setup/worker";
const SETUP_STATUS_PATH: &str = "/api/v1/setup/status";
const LOGIN_PATH: &str = "/api/v1/auth/login";
/// Redémarrage d'une installation devenue worker : une vérification par seconde, 30 au plus.
const WORKER_START_POLL_MS: i32 = 1000;
const WORKER_START_MAX_POLLS: usize = 30;

/// Champ de formulaire lié à un signal texte.
#[component]
fn Field(
    id: &'static str,
    label: &'static str,
    value: RwSignal<String>,
    #[prop(default = "text")] kind: &'static str,
    #[prop(default = "off")] autocomplete: &'static str,
    #[prop(optional)] help: &'static str,
) -> impl IntoView {
    view! {
        <div class="form-group">
            <label for=id class="form-label">{label}</label>
            <input id=id class="form-input" type=kind required=true autocomplete=autocomplete
                prop:value=move || value.get()
                on:input=move |event| value.set(event_target_value(&event)) />
            {(!help.is_empty()).then(|| view! { <p class="form-help">{help}</p> })}
        </div>
    }
}

/// Carte de la porte d'accès (`auth-gate`) : marque, titre, sous-titre et formulaire.
#[component]
pub fn GateCard(
    #[prop(into)] title: String,
    subtitle: &'static str,
    children: Children,
) -> impl IntoView {
    view! {
        <div class="auth-gate" role="dialog" aria-modal="true" aria-labelledby="auth-gate-title">
            <div class="auth-gate__card">
                <div class="auth-gate__brand">
                    <span class="auth-gate__logo">"MKV"</span>
                    <span class="auth-gate__name">"Flux de Fichiers"</span>
                </div>
                <h1 id="auth-gate-title" class="auth-gate__title">{title}</h1>
                <p class="auth-gate__subtitle">{subtitle}</p>
                {children()}
            </div>
        </div>
    }
}

#[component]
pub fn RoleChoice(goto: Callback<Gate>) -> impl IntoView {
    view! {
        <GateCard title="Bienvenue" subtitle="Choisissez le rôle de cette installation.">
            <div class="auth-gate__form">
                <button type="button" class="btn btn-primary auth-gate__submit" on:click=move |_| goto.run(Gate::SetupCentral)>
                    "Installer un central"
                </button>
                <p class="form-help">"L'installation principale : comptes, bibliothèques, flux et tâches."</p>
                <button type="button" class="btn btn-secondary auth-gate__submit" on:click=move |_| goto.run(Gate::SetupWorker)>
                    "Enrôler un worker"
                </button>
                <p class="form-help">"Un worker rattaché à un central existant."</p>
            </div>
        </GateCard>
    }
}

#[component]
pub fn CentralSetupForm(on_open: Callback<Me>, goto: Callback<Gate>) -> impl IntoView {
    let (name, username, password, confirm) = (
        RwSignal::new(String::new()),
        RwSignal::new(String::new()),
        RwSignal::new(String::new()),
        RwSignal::new(String::new()),
    );
    let feedback = RwSignal::new(None::<String>);
    let submit = Action::new_local(move |request: &SetupRequest| {
        let request = request.clone();
        async move {
            let session =
                api::post::<SetupRequest, SessionInfo>(SETUP_CENTRAL_PATH, Some(&request)).await?;
            identity_after(session).await
        }
    });
    Effect::new(move |_| match submit.value().get() {
        Some(Ok(me)) => on_open.run(me),
        Some(Err(error)) => feedback.set(Some(access_error(&error))),
        None => {}
    });
    view! {
        <GateCard title="Installation du central" subtitle="Créez le compte administrateur pour commencer.">
            <form class="auth-gate__form" on:submit=move |event| {
                event.prevent_default();
                if password.get_untracked() != confirm.get_untracked() {
                    feedback.set(Some("Les deux mots de passe ne correspondent pas.".into()));
                    return;
                }
                submit.dispatch(SetupRequest { name: name.get_untracked(), username: username.get_untracked(), password: password.get_untracked() });
                password.set(String::new());
                confirm.set(String::new());
            }>
                <Field id="setup-name" label="Nom du central" value=name help="Affiché dans l'interface et dans la liste des workers." />
                <Field id="setup-username" label="Identifiant administrateur" value=username autocomplete="username" />
                <Field id="setup-password" label="Mot de passe" value=password kind="password" autocomplete="new-password" help="8 caractères au moins." />
                <Field id="setup-password-confirm" label="Confirmer le mot de passe" value=confirm kind="password" autocomplete="new-password" />
                <p class="auth-gate__notice">
                    "Tant que l'installation n'est pas terminée, toute personne qui atteint cette page peut la terminer : faites-le tout de suite."
                </p>
                <p class="form-error" role="alert">{move || feedback.get()}</p>
                                    <button type="submit" class="btn btn-primary auth-gate__submit" disabled=move || submit.pending().get()>"Installer le central"</button>
                    <button type="button" class="btn btn-secondary auth-gate__submit" on:click=move |_| goto.run(Gate::ChooseRole)>"Retour"</button>
            </form>
        </GateCard>
    }
}

/// Attend que l'installation ait redémarré en worker ; renvoie son nom.
async fn wait_for_worker() -> Option<String> {
    for _ in 0..WORKER_START_MAX_POLLS {
        sleep(WORKER_START_POLL_MS).await;
        if let Ok(status) = api::get::<SetupStatus>(SETUP_STATUS_PATH).await
            && status.instance_kind == InstanceKind::Worker
        {
            return Some(status.instance_name.unwrap_or_default());
        }
    }
    None
}

#[component]
pub fn WorkerSetupForm(goto: Callback<Gate>) -> impl IntoView {
    let (central_url, username, password, worker_name) = (
        RwSignal::new(String::new()),
        RwSignal::new(String::new()),
        RwSignal::new(String::new()),
        RwSignal::new(String::new()),
    );
    let feedback = RwSignal::new(None::<String>);
    let submit = Action::new_local(move |request: &SetupWorkerRequest| {
        let request = request.clone();
        async move {
            api::post_empty(SETUP_WORKER_PATH, Some(&request)).await?;
            Ok::<_, api::ApiError>(wait_for_worker().await)
        }
    });
    Effect::new(move |_| match submit.value().get() {
        Some(Ok(Some(name))) => goto.run(Gate::Login {
            instance_name: Some(name),
        }),
        Some(Ok(None)) => feedback.set(Some(
            "Enrôlement fait, mais l'installation n'a pas redémarré : rechargez la page.".into(),
        )),
        Some(Err(error)) => feedback.set(Some(access_error(&error))),
        None => {}
    });
    view! {
        <GateCard title="Enrôlement du worker" subtitle="Rattachez cette installation à un central existant.">
            <form class="auth-gate__form" on:submit=move |event| {
                event.prevent_default();
                submit.dispatch(SetupWorkerRequest {
                    central_url: central_url.get_untracked(),
                    username: username.get_untracked(),
                    password: password.get_untracked(),
                    worker_name: worker_name.get_untracked(),
                });
                password.set(String::new());
            }>
                <Field id="worker-url" label="Adresse du central" value=central_url help="Adresse seule, sans chemin, par exemple http://central:3000." />
                <Field id="worker-username" label="Identifiant administrateur du central" value=username autocomplete="username" />
                <Field id="worker-password" label="Mot de passe" value=password kind="password" autocomplete="current-password" help="Utilisé une seule fois pour créer la clé du worker ; il n'est pas conservé." />
                <Field id="worker-name" label="Nom du worker" value=worker_name help="Lettres, chiffres, point, tiret et tiret bas." />
                <p class="form-error" role="alert">{move || feedback.get()}</p>
                                    <button type="submit" class="btn btn-primary auth-gate__submit" disabled=move || submit.pending().get()>
                        {move || if submit.pending().get() { "Enrôlement et redémarrage…" } else { "Rattacher" }}
                    </button>
                    <button type="button" class="btn btn-secondary auth-gate__submit" on:click=move |_| goto.run(Gate::ChooseRole)>"Retour"</button>
            </form>
        </GateCard>
    }
}

#[component]
pub fn LoginForm(instance_name: Option<String>, on_open: Callback<Me>) -> impl IntoView {
    let (username, password) = (RwSignal::new(String::new()), RwSignal::new(String::new()));
    let feedback = RwSignal::new(None::<String>);
    let submit = Action::new_local(move |request: &LoginRequest| {
        let request = request.clone();
        async move {
            let session =
                api::post::<LoginRequest, SessionInfo>(LOGIN_PATH, Some(&request)).await?;
            identity_after(session).await
        }
    });
    Effect::new(move |_| match submit.value().get() {
        Some(Ok(me)) => on_open.run(me),
        Some(Err(error)) => feedback.set(Some(access_error(&error))),
        None => {}
    });
    let title = instance_name.map_or_else(
        || "Connexion".to_owned(),
        |name| format!("Connexion — {name}"),
    );
    view! {
        <GateCard title=title subtitle="Ouvrez une session pour piloter l'installation.">
            <form class="auth-gate__form" on:submit=move |event| {
                event.prevent_default();
                submit.dispatch(LoginRequest { username: username.get_untracked(), password: password.get_untracked() });
                password.set(String::new());
            }>
                <Field id="login-username" label="Identifiant" value=username autocomplete="username" />
                <Field id="login-password" label="Mot de passe" value=password kind="password" autocomplete="current-password" />
                <p class="form-error" role="alert">{move || feedback.get()}</p>
                <button type="submit" class="btn btn-primary auth-gate__submit" disabled=move || submit.pending().get()>"Se connecter"</button>
            </form>
        </GateCard>
    }
}
