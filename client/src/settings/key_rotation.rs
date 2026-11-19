//! Renouvellement de la clé de chiffrement depuis la page Système, central en marche (ADR 0011).

use crate::api::{self, ApiError};
use crate::sleep::sleep;
use forgearr_shared::system::{KeySource, SystemStatus};
use leptos::prelude::*;

const ROTATE_PATH: &str = "/api/v1/system/secret-key/rotate";
/// Relecture de l'état après le délai de grâce du central (30 s) et le rechiffrement, en millisecondes.
const ROTATION_REFRESH_DELAY_MS: i32 = 35_000;
const CONFLICT_STATUS: u16 = 409;

/// Ce que la page peut proposer pour la clé de chiffrement.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyRotationState {
    /// Clé fournie par l'orchestrateur : elle se remplace hors de l'application.
    External,
    /// Anciennes clés encore présentes : rechiffrement en cours.
    InProgress,
    Ready,
}

pub fn key_rotation_state(source: KeySource, retired_key_ids: &[String]) -> KeyRotationState {
    if source == KeySource::External {
        KeyRotationState::External
    } else if !retired_key_ids.is_empty() {
        KeyRotationState::InProgress
    } else {
        KeyRotationState::Ready
    }
}

fn state_help(state: KeyRotationState) -> &'static str {
    match state {
        KeyRotationState::External => {
            "Clé fournie par l'orchestrateur (SECRET_KEY_FILE) : pour la renouveler, remplacez ce fichier."
        }
        KeyRotationState::InProgress => {
            "Renouvellement en cours : les secrets sont rechiffrés en arrière-plan, puis l'ancienne clé est oubliée."
        }
        KeyRotationState::Ready => {
            "Sans interruption ni sauvegarde à faire : la nouvelle clé sert aussitôt, les secrets existants sont rechiffrés en arrière-plan."
        }
    }
}

fn rotation_error(error: &ApiError) -> String {
    match error {
        ApiError::Rejected {
            status: CONFLICT_STATUS,
            ..
        } => {
            "Un renouvellement est déjà en cours, ou la clé est fournie par l'orchestrateur.".into()
        }
        other => other.message(),
    }
}

#[component]
pub fn KeyRotationControl(
    status: Signal<Option<Result<SystemStatus, String>>>,
    on_changed: Callback<()>,
) -> impl IntoView {
    let rotate = Action::new_local(move |()| async move {
        let result = api::post::<(), SystemStatus>(ROTATE_PATH, None).await;
        if result.is_ok() {
            on_changed.run(());
            sleep(ROTATION_REFRESH_DELAY_MS).await;
            on_changed.run(());
        }
        result
    });
    let state = Signal::derive(move || {
        status
            .get()
            .and_then(Result::ok)
            .map(|s| key_rotation_state(s.key_source, &s.retired_key_ids))
    });
    move || {
        state.get().map(|current| {
            view! {
                <div class="stack">
                    {(current != KeyRotationState::External).then(|| view! {
                        <div class="cluster cluster-wrap">
                            <button type="button" class="btn btn-secondary btn-sm"
                                disabled=move || rotate.pending().get() || current == KeyRotationState::InProgress
                                on:click=move |_| { rotate.dispatch(()); }>
                                {move || if rotate.pending().get() { "Renouvellement…" } else { "Renouveler la clé de chiffrement" }}
                            </button>
                            {move || rotate.value().get().and_then(Result::err).map(|err| view! {
                                <span class="form-error">{rotation_error(&err)}</span>
                            })}
                        </div>
                    })}
                    <p class="form-help">{state_help(current)}</p>
                </div>
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_rotation_state_follows_the_key_source_and_retired_keys() {
        let retired = ["old".to_owned()];
        assert_eq!(
            key_rotation_state(KeySource::Local, &[]),
            KeyRotationState::Ready
        );
        assert_eq!(
            key_rotation_state(KeySource::Local, &retired),
            KeyRotationState::InProgress
        );
        assert_eq!(
            key_rotation_state(KeySource::External, &retired),
            KeyRotationState::External
        );
    }
}
