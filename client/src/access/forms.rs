//! Formulaires d'accès : choix du rôle, installation du central ou d'un worker, connexion.

use super::{Gate, access_error, identity_after};
use crate::api;
use crate::sleep::sleep;
use crate::ui::{BrandMark, Icon, IconName};
use forgearr_shared::InstanceKind;
use forgearr_shared::accounts::LoginRequest;
use forgearr_shared::brand::NAME;
use forgearr_shared::session::{Me, SessionInfo};
use forgearr_shared::system::{
    SetupRequest, SetupStatus, SetupWorkerApprovalRequest, SetupWorkerRequest,
    SetupWorkerRequestStatus, WorkerSetupRequestState,
};
use leptos::prelude::*;

const SETUP_CENTRAL_PATH: &str = "/api/v1/setup/central";
const SETUP_WORKER_PATH: &str = "/api/v1/setup/worker";
const SETUP_WORKER_REQUEST_PATH: &str = "/api/v1/setup/worker-request";
const SETUP_STATUS_PATH: &str = "/api/v1/setup/status";
const LOGIN_PATH: &str = "/api/v1/auth/login";
/// Redémarrage d'une installation devenue worker : une vérification par seconde, 30 au plus.
const WORKER_START_POLL_MS: i32 = 1000;
const WORKER_START_MAX_POLLS: usize = 30;
const WORKER_APPROVAL_POLL_MS: i32 = 1500;

/// Champ de formulaire lié à un signal texte.
#[component]
pub fn Field(
    id: &'static str,
    label: &'static str,
    value: RwSignal<String>,
    #[prop(default = "text")] kind: &'static str,
    #[prop(default = "off")] autocomplete: &'static str,
    #[prop(optional)] help: &'static str,
    #[prop(default = true)] required: bool,
) -> impl IntoView {
    view! {
        <div class="form-group">
            <label for=id class="form-label">{label}</label>
            <input id=id class="form-input" type=kind required=required autocomplete=autocomplete
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
    #[prop(default = false)] wide: bool,
    children: Children,
) -> impl IntoView {
    view! {
        <div class="auth-gate" role="dialog" aria-modal="true" aria-labelledby="auth-gate-title">
            <div class="auth-gate__card" class:auth-gate__card--wide=wide>
                <div class="auth-gate__brand">
                    <BrandMark />
                    <span class="auth-gate__name">{NAME}</span>
                </div>
                <h1 id="auth-gate-title" class="auth-gate__title">{title}</h1>
                <p class="auth-gate__subtitle">{subtitle}</p>
                {children()}
            </div>
        </div>
    }
}

#[derive(Clone, Copy)]
enum RoleTarget {
    Central,
    Worker,
}

impl RoleTarget {
    const fn to_gate(self) -> Gate {
        match self {
            Self::Central => Gate::SetupCentral,
            Self::Worker => Gate::SetupWorker,
        }
    }
}

struct RoleOption {
    target: RoleTarget,
    icon: IconName,
    badge: &'static str,
    title: &'static str,
    description: &'static str,
}

const ROLE_OPTIONS: [RoleOption; 2] = [
    RoleOption {
        target: RoleTarget::Central,
        icon: IconName::Dashboard,
        badge: "Recommandé pour démarrer",
        title: "Installer un central",
        description: "Orchestre principal : comptes, bibliothèques, flux nodaux et tâches.",
    },
    RoleOption {
        target: RoleTarget::Worker,
        icon: IconName::Workers,
        badge: "Nœud de calcul",
        title: "Enrôler un worker",
        description: "Exécuteur dédié rattaché à un central existant pour traiter vos médias.",
    },
];

#[component]
pub fn RoleChoice(goto: Callback<Gate>) -> impl IntoView {
    view! {
        <GateCard title="Bienvenue" subtitle="Choisissez le rôle de cette installation." wide=true>
            <div class="auth-role-grid">
                {ROLE_OPTIONS.into_iter().map(|opt| {
                    let target = opt.target;
                    view! {
                        <button type="button" class="auth-role-card" on:click=move |_| goto.run(target.to_gate())>
                            <div class="auth-role-card__top">
                                <span class="auth-role-card__icon" aria-hidden="true">
                                    <Icon name=opt.icon size=18 />
                                </span>
                                <span class="auth-role-card__badge">{opt.badge}</span>
                            </div>
                            <span class="auth-role-card__title">{opt.title}</span>
                            <span class="auth-role-card__desc">{opt.description}</span>
                        </button>
                    }
                }).collect_view()}
            </div>
        </GateCard>
    }
}

#[component]
pub fn CentralSetupForm(on_open: Callback<Me>, goto: Callback<Gate>) -> impl IntoView {
    let (name, username, password, confirm, email) = (
        RwSignal::new(String::new()),
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
                let email = email.get_untracked();
                submit.dispatch(SetupRequest { name: name.get_untracked(), username: username.get_untracked(), password: password.get_untracked(), email: Some(email).filter(|email| !email.trim().is_empty()) });
                password.set(String::new());
                confirm.set(String::new());
            }>
                <Field id="setup-name" label="Nom du central" value=name help="Affiché dans l'interface et dans la liste des workers." />
                <Field id="setup-username" label="Identifiant administrateur" value=username autocomplete="username" />
                <Field id="setup-email" label="Adresse e-mail (facultative)" value=email kind="email" autocomplete="email" required=false help="Sert à recevoir un lien si vous oubliez votre mot de passe, une fois l'envoi de courriels configuré." />
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
    let (central_url, worker_name) = (RwSignal::new(String::new()), RwSignal::new(String::new()));
    let (legacy_username, legacy_password) =
        (RwSignal::new(String::new()), RwSignal::new(String::new()));
    let feedback = RwSignal::new(None::<String>);
    let pending_status = RwSignal::new(None::<SetupWorkerRequestStatus>);
    let polling_active = RwSignal::new(false);

    let start_polling = move || {
        if polling_active.get_untracked() {
            return;
        }
        polling_active.set(true);
        leptos::task::spawn_local(async move {
            loop {
                match api::get::<SetupWorkerRequestStatus>(SETUP_WORKER_REQUEST_PATH).await {
                    Ok(status) => match status.state {
                        WorkerSetupRequestState::Pending => {
                            pending_status.set(Some(status));
                        }
                        WorkerSetupRequestState::Approved => {
                            pending_status.set(Some(status));
                            if let Some(name) = wait_for_worker().await {
                                goto.run(Gate::Login {
                                    instance_name: Some(name),
                                    password_reset_enabled: false,
                                });
                            } else {
                                feedback.set(Some(
                                    "Enrôlement approuvé, mais l'installation n'a pas redémarré : rechargez la page."
                                        .into(),
                                ));
                            }
                            break;
                        }
                        WorkerSetupRequestState::Rejected => {
                            pending_status.set(None);
                            feedback.set(Some(
                                "La demande d'enrôlement a été refusée sur le central.".into(),
                            ));
                            break;
                        }
                        WorkerSetupRequestState::Expired => {
                            pending_status.set(None);
                            feedback.set(Some(
                                "La demande d'enrôlement a expiré : relancez une demande.".into(),
                            ));
                            break;
                        }
                        WorkerSetupRequestState::Error => {
                            pending_status.set(None);
                            let code = status
                                .error_code
                                .unwrap_or_else(|| "internal_error".to_owned());
                            feedback.set(Some(access_error(&api::ApiError::Rejected {
                                status: 400,
                                code,
                                detail: None,
                            })));
                            break;
                        }
                        WorkerSetupRequestState::Idle => {
                            pending_status.set(None);
                            break;
                        }
                    },
                    Err(err) => {
                        pending_status.set(None);
                        feedback.set(Some(access_error(&err)));
                        break;
                    }
                }
                sleep(WORKER_APPROVAL_POLL_MS).await;
            }
            polling_active.set(false);
        });
    };

    leptos::task::spawn_local(async move {
        if let Ok(status) = api::get::<SetupWorkerRequestStatus>(SETUP_WORKER_REQUEST_PATH).await
            && status.state == WorkerSetupRequestState::Pending
        {
            pending_status.set(Some(status));
            start_polling();
        }
    });

    let submit_approval = Action::new_local(move |request: &SetupWorkerApprovalRequest| {
        let request = request.clone();
        async move {
            api::post::<SetupWorkerApprovalRequest, SetupWorkerRequestStatus>(
                SETUP_WORKER_REQUEST_PATH,
                Some(&request),
            )
            .await
        }
    });
    Effect::new(move |_| match submit_approval.value().get() {
        Some(Ok(status)) => {
            feedback.set(None);
            pending_status.set(Some(status));
            start_polling();
        }
        Some(Err(error)) => feedback.set(Some(access_error(&error))),
        None => {}
    });

    let cancel_approval =
        Action::new_local(move |()| async move { api::delete(SETUP_WORKER_REQUEST_PATH).await });
    Effect::new(move |_| match cancel_approval.value().get() {
        Some(Ok(())) => {
            pending_status.set(None);
            feedback.set(None);
        }
        Some(Err(error)) => feedback.set(Some(access_error(&error))),
        None => {}
    });

    let submit_legacy = Action::new_local(move |request: &SetupWorkerRequest| {
        let request = request.clone();
        async move {
            api::post_empty(SETUP_WORKER_PATH, Some(&request)).await?;
            Ok::<_, api::ApiError>(wait_for_worker().await)
        }
    });
    Effect::new(move |_| match submit_legacy.value().get() {
        Some(Ok(Some(name))) => goto.run(Gate::Login {
            instance_name: Some(name),
            password_reset_enabled: false,
        }),
        Some(Ok(None)) => feedback.set(Some(
            "Enrôlement fait, mais l'installation n'a pas redémarré : rechargez la page.".into(),
        )),
        Some(Err(error)) => feedback.set(Some(access_error(&error))),
        None => {}
    });

    view! {
        <GateCard title="Enrôlement du worker" subtitle="Rattachez cette installation à un central existant.">
            {move || {
                if let Some(status) = pending_status.get() {
                    let target = status.central_url.unwrap_or_default();
                    let name = status.worker_name.unwrap_or_else(|| "worker".to_owned());
                    let state_label = if status.state == WorkerSetupRequestState::Approved {
                        "Demande approuvée, bascule en worker…"
                    } else {
                        "En attente d'approbation sur le central…"
                    };
                    view! {
                        <div class="auth-gate__form">
                            <p class="auth-gate__notice">
                                <strong>{state_label}</strong><br />
                                "Ouvrez l'écran Workers sur "<code>{target}</code>" et approuvez le nœud "<strong>{name}</strong>"."
                            </p>
                            <p class="form-error" role="alert">{move || feedback.get()}</p>
                            <button type="button" class="btn btn-secondary auth-gate__submit"
                                disabled=move || cancel_approval.pending().get() || status.state == WorkerSetupRequestState::Approved
                                on:click=move |_| { cancel_approval.dispatch(()); }>
                                "Annuler la demande"
                            </button>
                        </div>
                    }
                    .into_any()
                } else {
                    view! {
                        <div class="stack">
                            <form class="auth-gate__form" on:submit=move |event| {
                                event.prevent_default();
                                feedback.set(None);
                                submit_approval.dispatch(SetupWorkerApprovalRequest {
                                    central_url: central_url.get_untracked(),
                                    worker_name: worker_name.get_untracked(),
                                });
                            }>
                                <Field id="worker-url" label="Adresse du central" value=central_url help="Adresse seule, sans chemin, par exemple http://central:3000. Ouvrez d'abord l'ajout de worker sur le central." />
                                <Field id="worker-name" label="Nom du worker (facultatif)" value=worker_name required=false help="Nom d'hôte par défaut. Lettres, chiffres, point, tiret et tiret bas." />
                                <p class="form-error" role="alert">{move || feedback.get()}</p>
                                <button type="submit" class="btn btn-primary auth-gate__submit" disabled=move || submit_approval.pending().get()>
                                    {move || if submit_approval.pending().get() { "Envoi de la demande…" } else { "Demander l'enrôlement" }}
                                </button>
                                <button type="button" class="btn btn-secondary auth-gate__submit" on:click=move |_| goto.run(Gate::ChooseRole)>"Retour"</button>
                            </form>
                            <details class="auth-gate__fallback">
                                <summary class="form-help">"Enrôlement direct avec identifiants administrateur (secours)"</summary>
                                <form class="auth-gate__form" on:submit=move |event| {
                                    event.prevent_default();
                                    feedback.set(None);
                                    submit_legacy.dispatch(SetupWorkerRequest {
                                        central_url: central_url.get_untracked(),
                                        username: legacy_username.get_untracked(),
                                        password: legacy_password.get_untracked(),
                                        worker_name: worker_name.get_untracked(),
                                    });
                                    legacy_password.set(String::new());
                                }>
                                    <Field id="worker-legacy-username" label="Identifiant administrateur du central" value=legacy_username autocomplete="username" />
                                    <Field id="worker-legacy-password" label="Mot de passe" value=legacy_password kind="password" autocomplete="current-password" help="Utilisé une seule fois ; non conservé." />
                                    <button type="submit" class="btn btn-secondary auth-gate__submit" disabled=move || submit_legacy.pending().get()>
                                        {move || if submit_legacy.pending().get() { "Enrôlement et redémarrage…" } else { "Rattacher avec identifiants" }}
                                    </button>
                                </form>
                            </details>
                        </div>
                    }
                    .into_any()
                }
            }}
        </GateCard>
    }
}

#[component]
pub fn LoginForm(
    instance_name: Option<String>,
    password_reset_enabled: bool,
    on_open: Callback<Me>,
    goto: Callback<Gate>,
) -> impl IntoView {
    let forgot_name = instance_name.clone();
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
                {password_reset_enabled.then(|| view! {
                    <button type="button" class="btn btn-secondary auth-gate__submit" on:click=move |_| goto.run(Gate::ForgotPassword { instance_name: forgot_name.clone() })>"Mot de passe oublié"</button>
                })}
            </form>
        </GateCard>
    }
}
