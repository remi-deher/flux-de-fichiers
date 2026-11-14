//! Mot de passe oublié : demande d'un lien par courriel, puis choix du nouveau mot de passe (ADR 0015).

use super::forms::{Field, GateCard};
use super::{Gate, access_error, clear_address_fragment};
use crate::api;
use flux_shared::accounts::{ForgotPasswordRequest, ResetPasswordRequest};
use leptos::prelude::*;

const FORGOT_PATH: &str = "/api/v1/auth/forgot-password";
const RESET_PATH: &str = "/api/v1/auth/reset-password";
const PASSWORD_MIN_LEN: usize = 8;

/// Retour à la connexion ; le lien « mot de passe oublié » n'existe que si le central l'a proposé.
fn back_to_login(goto: Callback<Gate>, instance_name: Option<String>) -> impl IntoView {
    view! {
        <button type="button" class="btn btn-secondary auth-gate__submit" on:click=move |_| goto.run(Gate::Login {
            instance_name: instance_name.clone(),
            password_reset_enabled: true,
        })>"Retour à la connexion"</button>
    }
}

#[component]
pub fn ForgotPasswordForm(instance_name: Option<String>, goto: Callback<Gate>) -> impl IntoView {
    let email = RwSignal::new(String::new());
    let feedback = RwSignal::new(None::<String>);
    let sent = RwSignal::new(false);
    let submit = Action::new_local(move |request: &ForgotPasswordRequest| {
        let request = request.clone();
        async move { api::post_empty(FORGOT_PATH, Some(&request)).await }
    });
    Effect::new(move |_| match submit.value().get() {
        Some(Ok(())) => sent.set(true),
        Some(Err(error)) => feedback.set(Some(access_error(&error))),
        None => {}
    });
    view! {
        <GateCard title="Mot de passe oublié" subtitle="Saisissez l'adresse e-mail de votre compte pour recevoir un lien.">
            <form class="auth-gate__form" on:submit=move |event| {
                event.prevent_default();
                feedback.set(None);
                submit.dispatch(ForgotPasswordRequest { email: email.get_untracked() });
            }>
                <Field id="forgot-email" label="Adresse e-mail" value=email kind="email" autocomplete="email" />
                <p class="form-error" role="alert">{move || feedback.get()}</p>
                <p class="form-help" role="status">{move || sent.get().then_some("Si un compte correspond à cette adresse, un lien valable 30 minutes vient d'être envoyé.")}</p>
                <button type="submit" class="btn btn-primary auth-gate__submit" disabled=move || submit.pending().get()>"Envoyer le lien"</button>
                {back_to_login(goto, instance_name)}
            </form>
        </GateCard>
    }
}

#[component]
pub fn ResetPasswordForm(
    instance_name: Option<String>,
    token: String,
    goto: Callback<Gate>,
) -> impl IntoView {
    let (password, confirm) = (RwSignal::new(String::new()), RwSignal::new(String::new()));
    let feedback = RwSignal::new(None::<String>);
    let done = RwSignal::new(false);
    let submit = Action::new_local(move |request: &ResetPasswordRequest| {
        let request = request.clone();
        async move { api::post_empty(RESET_PATH, Some(&request)).await }
    });
    Effect::new(move |_| match submit.value().get() {
        Some(Ok(())) => {
            clear_address_fragment();
            done.set(true);
        }
        Some(Err(api::ApiError::Rejected { status: 400, .. })) => feedback.set(Some(
            "Lien invalide ou expiré, ou mot de passe trop court : demandez un nouveau lien."
                .into(),
        )),
        Some(Err(error)) => feedback.set(Some(access_error(&error))),
        None => {}
    });
    view! {
        <GateCard title="Nouveau mot de passe" subtitle="Choisissez le mot de passe de votre compte.">
            <form class="auth-gate__form" on:submit=move |event| {
                event.prevent_default();
                let new_password = password.get_untracked();
                if new_password != confirm.get_untracked() {
                    feedback.set(Some("Les deux mots de passe ne correspondent pas.".into()));
                    return;
                }
                if new_password.chars().count() < PASSWORD_MIN_LEN {
                    feedback.set(Some("Le mot de passe doit avoir 8 caractères au moins.".into()));
                    return;
                }
                feedback.set(None);
                submit.dispatch(ResetPasswordRequest { token: token.clone(), new_password });
                password.set(String::new());
                confirm.set(String::new());
            }>
                <Field id="reset-password" label="Nouveau mot de passe" value=password kind="password" autocomplete="new-password" help="8 caractères au moins." />
                <Field id="reset-password-confirm" label="Confirmer le mot de passe" value=confirm kind="password" autocomplete="new-password" />
                <p class="form-error" role="alert">{move || feedback.get()}</p>
                <p class="form-help" role="status">{move || done.get().then_some("Mot de passe modifié : toutes vos sessions ont été fermées. Vous pouvez vous connecter.")}</p>
                <button type="submit" class="btn btn-primary auth-gate__submit" disabled=move || submit.pending().get() || done.get()>"Changer le mot de passe"</button>
                {back_to_login(goto, instance_name)}
            </form>
        </GateCard>
    }
}
