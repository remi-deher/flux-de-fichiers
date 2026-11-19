//! Connexion OAuth de l'assistant IA (flux « code d'appareil », sans clé d'API).

use crate::api;
use crate::sleep::sleep;
use crate::ui::{Tag, TagTone, Toasts};
use forgearr_shared::ai::{
    AiOauthAccount, AiOauthDeviceCode, AiOauthPollResponse, AiOauthPollStatus,
};
use leptos::prelude::*;

const AI_OAUTH_START_PATH: &str = "/api/v1/ai/oauth/start";
const AI_OAUTH_POLL_PATH: &str = "/api/v1/ai/oauth/poll";
const AI_OAUTH_PATH: &str = "/api/v1/ai/oauth";
const MILLIS_PER_SECOND: u64 = 1000;

/// Compte connecté, tel qu'affiché à côté du bouton de déconnexion.
fn account_label(account: &AiOauthAccount) -> String {
    let email = account.email.as_deref().unwrap_or("compte ChatGPT");
    match account.plan.as_deref() {
        Some(plan) => format!("{email} · offre {plan}"),
        None => email.to_owned(),
    }
}

/// Interroge le central jusqu'à la validation du code, son expiration ou l'abandon.
///
/// `attempt` identifie la connexion suivie : tout changement (annulation, nouvelle connexion,
/// démontage du composant) arrête la boucle.
async fn follow_connection(
    attempt: RwSignal<u64>,
    id: u64,
    interval_ms: i32,
    account: RwSignal<Option<AiOauthAccount>>,
    code: RwSignal<Option<AiOauthDeviceCode>>,
    feedback: RwSignal<Option<String>>,
) {
    loop {
        sleep(interval_ms).await;
        if attempt.try_get_untracked() != Some(id) {
            return;
        }
        let result = api::post::<(), AiOauthPollResponse>(AI_OAUTH_POLL_PATH, None).await;
        if attempt.try_get_untracked() != Some(id) {
            return;
        }
        match result {
            Ok(poll) => match poll.status {
                AiOauthPollStatus::Pending => {}
                AiOauthPollStatus::Connected => {
                    code.set(None);
                    account.set(poll.account);
                    Toasts::notify_success("Compte ChatGPT connecté.");
                    return;
                }
                AiOauthPollStatus::Expired => {
                    code.set(None);
                    Toasts::set_error(feedback, "Le code a expiré : relancez la connexion.");
                    return;
                }
            },
            Err(err) => {
                code.set(None);
                Toasts::set_error(feedback, err.message());
                return;
            }
        }
    }
}

#[component]
pub fn AiOauthPanel(account: RwSignal<Option<AiOauthAccount>>) -> impl IntoView {
    let code = RwSignal::new(None::<AiOauthDeviceCode>);
    let feedback = RwSignal::new(None::<String>);
    let attempt = RwSignal::new(0u64);
    on_cleanup(move || {
        attempt.try_update(|n| *n = n.wrapping_add(1));
    });

    let connect = move |_| {
        feedback.set(None);
        let id = attempt.get_untracked().wrapping_add(1);
        attempt.set(id);
        leptos::task::spawn_local(async move {
            match api::post::<(), AiOauthDeviceCode>(AI_OAUTH_START_PATH, None).await {
                Ok(device) => {
                    let interval_ms =
                        i32::try_from(device.interval_seconds.saturating_mul(MILLIS_PER_SECOND))
                            .unwrap_or(i32::MAX);
                    code.set(Some(device));
                    follow_connection(attempt, id, interval_ms, account, code, feedback).await;
                }
                Err(err) => Toasts::set_error(feedback, err.message()),
            }
        });
    };

    let cancel = move |_| {
        attempt.update(|n| *n = n.wrapping_add(1));
        code.set(None);
    };

    let disconnect = move |_| {
        leptos::task::spawn_local(async move {
            match api::delete(AI_OAUTH_PATH).await {
                Ok(()) => {
                    account.set(None);
                    Toasts::notify_success("Compte ChatGPT déconnecté.");
                }
                Err(err) => Toasts::set_error(feedback, err.message()),
            }
        });
    };

    view! {
        <div class="form-group">
            <span class="form-label">"Connexion ChatGPT"</span>
            {move || match (account.get(), code.get()) {
                (Some(connected), _) => view! {
                    <div class="cluster cluster-wrap">
                        <Tag label="Connecté" tone=TagTone::Success />
                        <span class="text-mono">{account_label(&connected)}</span>
                        <button type="button" class="btn btn-secondary" on:click=disconnect>
                            "Se déconnecter"
                        </button>
                    </div>
                }.into_any(),
                (None, Some(device)) => {
                    let url = device.verification_url;
                    let href = url.clone();
                    view! {
                    <div class="diagnostic-box" role="status">
                        <div>
                            <div class="diagnostic-box__title">
                                "Saisissez ce code sur la page de validation"
                            </div>
                            <div class="diagnostic-box__message text-mono">{device.user_code}</div>
                            <a href=href target="_blank" rel="noopener noreferrer">
                                {url}
                            </a>
                            <p class="text-dim">"En attente de validation…"</p>
                            <button type="button" class="btn btn-secondary" on:click=cancel>
                                "Annuler"
                            </button>
                        </div>
                    </div>
                }.into_any()
                }
                (None, None) => view! {
                    <button type="button" class="btn btn-primary" on:click=connect>
                        "Se connecter avec ChatGPT"
                    </button>
                }.into_any(),
            }}
            <p class="form-help">
                "Utilise votre abonnement ChatGPT, comme Codex CLI : aucune clé d'API. Cet accès n'est pas une API publique garantie et reste soumis aux conditions d'OpenAI. Les jetons sont chiffrés au repos et jamais exposés au navigateur."
            </p>
            {move || feedback.get().map(|err| view! {
                <p class="form-error" role="alert">{err}</p>
            })}
        </div>
    }
}
