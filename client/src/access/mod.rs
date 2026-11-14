//! Accès : installation (central ou worker), connexion et session (ADR 0011, 0014).

mod forms;

use crate::api::{self, ApiError};
use flux_shared::session::{Me, SessionInfo};
use flux_shared::system::SetupStatus;
use forms::{CentralSetupForm, GateCard, LoginForm, RoleChoice, WorkerSetupForm};
use leptos::prelude::*;

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
        (_, Some("forbidden")) => "Requête refusée : le compte doit être administrateur et l'origine autorisée.".into(),
        (_, Some("central_unavailable")) => "Central injoignable : vérifiez son adresse.".into(),
        _ => error.message(),
    }
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
        Err(ApiError::Unauthorized) => Ok(Gate::Login {
            instance_name: status.instance_name,
        }),
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
    move || match gate.get() {
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
        Some(Ok(Gate::Login { instance_name })) => {
            view! { <LoginForm instance_name=instance_name on_open=open /> }.into_any()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
