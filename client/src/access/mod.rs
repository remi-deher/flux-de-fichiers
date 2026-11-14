//! Accès : installation (central ou worker), connexion et session (ADR 0011, 0014).

mod forms;
mod recovery;

use crate::api::{self, ApiError};
use flux_shared::session::{Me, SessionInfo};
use flux_shared::system::SetupStatus;
use forms::{CentralSetupForm, GateCard, LoginForm, RoleChoice, WorkerSetupForm};
use leptos::prelude::*;
use recovery::{ForgotPasswordForm, ResetPasswordForm};

const SETUP_STATUS_PATH: &str = "/api/v1/setup/status";
const SESSION_PATH: &str = "/api/v1/auth/session";
const ME_PATH: &str = "/api/v1/me";

/// Écran d'accès à afficher, ou session ouverte.
#[derive(Clone, Debug, PartialEq)]
pub enum Gate {
    /// Installation vierge : choisir le rôle.
    ChooseRole,
    SetupCentral,
    SetupWorker,
    Login {
        instance_name: Option<String>,
        /// Le central sait-il envoyer un lien de réinitialisation par courriel ?
        password_reset_enabled: bool,
    },
    /// Demande d'un lien de réinitialisation par courriel (ADR 0015).
    ForgotPassword {
        instance_name: Option<String>,
    },
    /// Choix d'un nouveau mot de passe avec le jeton reçu par courriel.
    ResetPassword {
        instance_name: Option<String>,
        token: String,
    },
    Ready(Me),
}

/// Message d'un refus pendant l'accès.
pub fn access_error(error: &ApiError) -> String {
    match (error, error.code()) {
        (ApiError::Unauthorized, _) => "Identifiant ou mot de passe incorrect.".into(),
        (_, Some("too_many_attempts")) => "Trop d'essais : réessayez dans quelques minutes.".into(),
        (_, Some("invalid_input")) => "Vérifiez les champs : identifiant de 3 à 40 caractères (lettres, chiffres, . - _), mot de passe d'au moins 8 caractères, nom de worker (lettres, chiffres, . - _) et adresse sans chemin.".into(),
        (_, Some("already_configured")) => "Cette installation est déjà configurée : rechargez la page.".into(),
        (_, Some("not_found")) => "La réinitialisation par courriel n'est pas disponible sur ce central.".into(),
        (_, Some("forbidden")) => "Requête refusée : le compte doit être administrateur et l'origine autorisée.".into(),
        (_, Some("central_unavailable")) => "Central injoignable : vérifiez son adresse.".into(),
        _ => error.message(),
    }
}

/// Début du fragment d'adresse du lien reçu par courriel : `#reset=<jeton>`.
const RESET_FRAGMENT_PREFIX: &str = "#reset=";

/// Jeton d'un fragment d'adresse de réinitialisation ; `None` s'il n'en porte pas ou s'il est vide.
pub fn reset_token_from_fragment(fragment: &str) -> Option<String> {
    fragment
        .strip_prefix(RESET_FRAGMENT_PREFIX)
        .map(str::trim)
        .filter(|token| !token.is_empty())
        .map(str::to_owned)
}

/// Retire le jeton de l'adresse affichée : il ne doit pas rester dans l'historique du navigateur.
pub fn clear_address_fragment() {
    let _ = window().history().and_then(|history| {
        let path = window().location().pathname().unwrap_or_default();
        history.replace_state_with_url(&wasm_bindgen::JsValue::NULL, "", Some(&path))
    });
}

/// Lit l'identité après ouverture d'une session (jeton CSRF retenu).
pub async fn identity_after(session: SessionInfo) -> Result<Me, ApiError> {
    api::set_csrf_token(&session.csrf_token);
    api::get::<Me>(ME_PATH).await
}

/// Installation à faire, sinon session existante, sinon connexion.
pub async fn initial_gate() -> Result<Gate, ApiError> {
    let status = api::get::<SetupStatus>(SETUP_STATUS_PATH).await?;
    if status.needs_setup {
        return Ok(Gate::ChooseRole);
    }
    match api::get::<SessionInfo>(SESSION_PATH).await {
        Ok(session) => identity_after(session).await.map(Gate::Ready),
        Err(ApiError::Unauthorized) => {
            let fragment = window().location().hash().unwrap_or_default();
            Ok(match reset_token_from_fragment(&fragment) {
                Some(token) if status.password_reset_enabled => Gate::ResetPassword {
                    instance_name: status.instance_name,
                    token,
                },
                _ => Gate::Login {
                    instance_name: status.instance_name,
                    password_reset_enabled: status.password_reset_enabled,
                },
            })
        }
        Err(error) => Err(error),
    }
}

/// Porte d'accès : affiche le formulaire voulu puis l'application une fois la session ouverte.
#[component]
pub fn AccessGate(children: ChildrenFn) -> impl IntoView {
    let gate = RwSignal::new(None::<Result<Gate, ApiError>>);
    leptos::task::spawn_local(async move { gate.set(Some(initial_gate().await)) });
    let open = Callback::new(move |me: Me| gate.set(Some(Ok(Gate::Ready(me)))));
    let goto = Callback::new(move |next: Gate| gate.set(Some(Ok(next))));
    let children = StoredValue::new(children);
    move || {
        match gate.get() {
        None => {
            view! { <GateCard title="Chargement…" subtitle=""><span></span></GateCard> }.into_any()
        }
        Some(Err(error)) => view! {
            <GateCard title="Central injoignable" subtitle="">
                <p class="form-error" role="alert">{access_error(&error)}</p>
            </GateCard>
        }
        .into_any(),
        Some(Ok(Gate::Ready(me))) => {
            provide_context(me);
            children.read_value()().into_any()
        }
        Some(Ok(Gate::ChooseRole)) => view! { <RoleChoice goto=goto /> }.into_any(),
        Some(Ok(Gate::SetupCentral)) => {
            view! { <CentralSetupForm on_open=open goto=goto /> }.into_any()
        }
        Some(Ok(Gate::SetupWorker)) => view! { <WorkerSetupForm goto=goto /> }.into_any(),
        Some(Ok(Gate::Login {
            instance_name,
            password_reset_enabled,
        })) => view! {
            <LoginForm instance_name=instance_name password_reset_enabled=password_reset_enabled on_open=open goto=goto />
        }
        .into_any(),
        Some(Ok(Gate::ForgotPassword { instance_name })) => {
            view! { <ForgotPasswordForm instance_name=instance_name goto=goto /> }.into_any()
        }
        Some(Ok(Gate::ResetPassword { instance_name, token })) => {
            view! { <ResetPasswordForm instance_name=instance_name token=token goto=goto /> }.into_any()
        }
    }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_reset_token_is_read_from_the_fragment_only() {
        assert_eq!(
            reset_token_from_fragment("#reset=abc123").as_deref(),
            Some("abc123")
        );
        assert_eq!(reset_token_from_fragment("#reset="), None);
        assert_eq!(reset_token_from_fragment("#autre=abc"), None);
        assert_eq!(reset_token_from_fragment(""), None);
    }

    #[test]
    fn access_errors_are_explained() {
        assert!(access_error(&ApiError::Unauthorized).contains("incorrect"));
        let throttled = ApiError::Rejected {
            status: 429,
            code: "too_many_attempts".into(),
        };
        assert!(access_error(&throttled).contains("Trop d'essais"));
        let other = ApiError::Rejected {
            status: 500,
            code: "internal".into(),
        };
        assert_eq!(access_error(&other), other.message());
    }
}
