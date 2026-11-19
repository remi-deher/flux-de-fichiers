//! Kit de récupération de la clé de chiffrement : création, téléchargement et restauration (ADR 0011).

use crate::access::forms::Field;
use crate::api::{self, ApiError};
use forgearr_shared::system::{
    ExportRecoveryKitRequest, RECOVERY_PASSPHRASE_MIN_LEN, RecoveryKit, RestoreRecoveryKitRequest,
    SystemStatus,
};
use leptos::prelude::*;
use wasm_bindgen::JsCast;

const EXPORT_PATH: &str = "/api/v1/system/secret-key/recovery-kit";
const RESTORE_PATH: &str = "/api/v1/system/secret-key/restore";
const BAD_REQUEST_STATUS: u16 = 400;
const KIT_FILE_PREFIX: &str = "forgearr-kit-recuperation";

/// Le dernier kit exporté couvre-t-il la clé active ?
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KitState {
    Missing,
    Outdated,
    UpToDate,
}

pub fn kit_state(key_id: &str, recovery_kit_key_id: Option<&str>) -> KitState {
    match recovery_kit_key_id {
        None => KitState::Missing,
        Some(exported) if exported == key_id => KitState::UpToDate,
        Some(_) => KitState::Outdated,
    }
}

pub fn kit_state_label(state: KitState) -> &'static str {
    match state {
        KitState::Missing => "⚠ Jamais créé : sans lui, une base restaurée perd ses secrets",
        KitState::Outdated => "⚠ À refaire : la clé a changé depuis le dernier kit",
        KitState::UpToDate => "✓ À jour",
    }
}

/// Raison de refuser la phrase de passe avant tout appel au central.
pub fn passphrase_problem(passphrase: &str, confirmation: &str) -> Option<String> {
    if passphrase.chars().count() < RECOVERY_PASSPHRASE_MIN_LEN {
        Some(format!(
            "La phrase de passe doit compter au moins {RECOVERY_PASSPHRASE_MIN_LEN} caractères."
        ))
    } else if passphrase != confirmation {
        Some("Les deux phrases de passe diffèrent.".to_owned())
    } else {
        None
    }
}

/// Lien de téléchargement du kit, sans passer par un fichier sur le central.
fn kit_download_href(kit: &RecoveryKit) -> String {
    let json = serde_json::to_string_pretty(kit).unwrap_or_default();
    format!(
        "data:application/json;charset=utf-8,{}",
        js_sys::encode_uri_component(&json)
    )
}

fn kit_error(error: &ApiError) -> String {
    match error {
        ApiError::Rejected {
            status: BAD_REQUEST_STATUS,
            ..
        } => "Phrase de passe incorrecte ou kit illisible.".into(),
        other => other.message(),
    }
}

async fn read_selected_file(input: web_sys::HtmlInputElement) -> Option<String> {
    let file = input.files()?.get(0)?;
    let text = wasm_bindgen_futures::JsFuture::from(file.text())
        .await
        .ok()?;
    text.as_string()
}

#[component]
fn ExportKitForm(on_changed: Callback<()>) -> impl IntoView {
    let passphrase = RwSignal::new(String::new());
    let confirmation = RwSignal::new(String::new());
    let problem = RwSignal::new(None::<String>);
    let export = Action::new_local(move |phrase: &String| {
        let request = ExportRecoveryKitRequest {
            passphrase: phrase.clone(),
        };
        async move {
            let result = api::post::<_, RecoveryKit>(EXPORT_PATH, Some(&request)).await;
            if result.is_ok() {
                on_changed.run(());
            }
            result
        }
    });
    let submit = move |event: leptos::ev::SubmitEvent| {
        event.prevent_default();
        let phrase = passphrase.get_untracked();
        problem.set(passphrase_problem(&phrase, &confirmation.get_untracked()));
        if problem.get_untracked().is_none() {
            export.dispatch(phrase);
        }
    };
    view! {
        <form class="stack" on:submit=submit>
            <Field id="recovery-kit-passphrase" label="Phrase de passe du kit" value=passphrase
                kind="password" autocomplete="new-password"
                help="Notez-la à part : sans elle, le kit ne sert à rien. Elle n'est jamais enregistrée." />
            <Field id="recovery-kit-confirmation" label="Confirmer la phrase de passe" value=confirmation
                kind="password" autocomplete="new-password" />
            <div class="cluster cluster-wrap">
                <button type="submit" class="btn btn-secondary btn-sm" disabled=move || export.pending().get()>
                    {move || if export.pending().get() { "Création…" } else { "Créer le kit de récupération" }}
                </button>
                {move || problem.get().map(|text| view! { <span class="form-error">{text}</span> })}
                {move || match export.value().get() {
                    Some(Ok(kit)) => view! {
                        <a class="btn btn-primary btn-sm" href=kit_download_href(&kit)
                            download=format!("{KIT_FILE_PREFIX}-{}.json", kit.key_id)>
                            "Télécharger le kit"
                        </a>
                    }.into_any(),
                    Some(Err(err)) => view! { <span class="form-error">{kit_error(&err)}</span> }.into_any(),
                    None => ().into_any(),
                }}
            </div>
        </form>
    }
}

#[component]
fn RestoreKitForm(on_changed: Callback<()>) -> impl IntoView {
    let kit = RwSignal::new(None::<RecoveryKit>);
    let passphrase = RwSignal::new(String::new());
    let problem = RwSignal::new(None::<&'static str>);
    let restore = Action::new_local(move |request: &RestoreRecoveryKitRequest| {
        let request = request.clone();
        async move {
            let result = api::post::<_, SystemStatus>(RESTORE_PATH, Some(&request)).await;
            if result.is_ok() {
                on_changed.run(());
            }
            result
        }
    });
    let on_file = move |event: leptos::ev::Event| {
        let Some(input) = event
            .target()
            .and_then(|target| target.dyn_into::<web_sys::HtmlInputElement>().ok())
        else {
            return;
        };
        wasm_bindgen_futures::spawn_local(async move {
            let parsed = read_selected_file(input)
                .await
                .and_then(|text| serde_json::from_str::<RecoveryKit>(&text).ok());
            problem.set(
                parsed
                    .is_none()
                    .then_some("Ce fichier n'est pas un kit de récupération."),
            );
            kit.set(parsed);
        });
    };
    let submit = move |event: leptos::ev::SubmitEvent| {
        event.prevent_default();
        let Some(selected) = kit.get_untracked() else {
            problem.set(Some("Choisissez d'abord le fichier du kit."));
            return;
        };
        restore.dispatch(RestoreRecoveryKitRequest {
            kit: selected,
            passphrase: passphrase.get_untracked(),
        });
    };
    view! {
        <form class="stack" on:submit=submit>
            <p class="form-help">
                "Après avoir remis en place une sauvegarde de la base sur une nouvelle installation : les secrets enregistrés redeviennent lisibles."
            </p>
            <div class="form-group">
                <label class="form-label" for="recovery-kit-file">"Fichier du kit"</label>
                <input id="recovery-kit-file" class="form-input" type="file" accept="application/json,.json"
                    on:change=on_file />
            </div>
            <Field id="recovery-kit-restore-passphrase" label="Phrase de passe du kit" value=passphrase
                kind="password" autocomplete="off" />
            <div class="cluster cluster-wrap">
                <button type="submit" class="btn btn-secondary btn-sm" disabled=move || restore.pending().get()>
                    {move || if restore.pending().get() { "Restauration…" } else { "Restaurer" }}
                </button>
                {move || problem.get().map(|text| view! { <span class="form-error">{text}</span> })}
                {move || match restore.value().get() {
                    Some(Ok(_)) => view! { <span class="text-strong">"Kit restauré : les secrets sont de nouveau lisibles."</span> }.into_any(),
                    Some(Err(err)) => view! { <span class="form-error">{kit_error(&err)}</span> }.into_any(),
                    None => ().into_any(),
                }}
            </div>
        </form>
    }
}

/// Création et restauration du kit, repliées sous la section Système.
#[component]
pub fn RecoveryKitControl(on_changed: Callback<()>) -> impl IntoView {
    view! {
        <details class="settings-help-details">
            <summary class="settings-help-details__summary">"Kit de récupération de la clé"</summary>
            <div class="settings-help-details__body stack">
                <p class="form-help">
                    "Le kit contient la clé de chiffrement, protégée par une phrase de passe. Rangez-le avec vos sauvegardes de /config et refaites-le après chaque renouvellement de la clé."
                </p>
                <ExportKitForm on_changed=on_changed />
            </div>
        </details>
        <details class="settings-help-details">
            <summary class="settings-help-details__summary">"Restaurer depuis un kit"</summary>
            <div class="settings-help-details__body stack">
                <RestoreKitForm on_changed=on_changed />
            </div>
        </details>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_kit_state_compares_the_exported_key_with_the_active_one() {
        assert_eq!(kit_state("abcd", None), KitState::Missing);
        assert_eq!(kit_state("abcd", Some("abcd")), KitState::UpToDate);
        assert_eq!(kit_state("abcd", Some("0000")), KitState::Outdated);
    }

    #[test]
    fn the_passphrase_must_be_long_enough_and_confirmed() {
        assert!(passphrase_problem("courte", "courte").is_some());
        assert!(passphrase_problem("une phrase assez longue", "autre").is_some());
        assert_eq!(
            passphrase_problem("une phrase assez longue", "une phrase assez longue"),
            None
        );
    }
}
