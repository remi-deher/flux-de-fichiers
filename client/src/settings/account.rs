//! Mon compte : identité, changement de mot de passe et déconnexion.

use crate::api;
use flux_shared::InstanceKind;
use flux_shared::accounts::ChangePasswordRequest;
use flux_shared::session::Me;
use leptos::prelude::*;

const PASSWORD_PATH: &str = "/api/v1/account/password";
const LOGOUT_PATH: &str = "/api/v1/auth/logout";

pub fn role_label(role: flux_shared::Role) -> &'static str {
    match role {
        flux_shared::Role::Admin => "Administrateur",
        flux_shared::Role::Worker => "Worker",
        flux_shared::Role::Viewer => "Lecteur",
    }
}

/// Ferme la session puis recharge la page sur l'écran de connexion.
pub async fn logout() {
    let _ = api::post_empty::<()>(LOGOUT_PATH, None).await;
    let _ = window().location().reload();
}

#[component]
pub fn AccountSection(me: Me) -> impl IntoView {
    let (current, next, confirm) = (
        RwSignal::new(String::new()),
        RwSignal::new(String::new()),
        RwSignal::new(String::new()),
    );
    let feedback = RwSignal::new(None::<String>);
    let change = Action::new_local(move |request: &ChangePasswordRequest| {
        let request = request.clone();
        async move { api::put_empty(PASSWORD_PATH, &request).await }
    });
    Effect::new(move |_| match change.value().get() {
        Some(Ok(())) => feedback.set(Some(
            "Mot de passe changé ; vos autres sessions sont fermées.".into(),
        )),
        Some(Err(api::ApiError::Unauthorized)) => {
            feedback.set(Some("Mot de passe actuel incorrect.".into()))
        }
        Some(Err(error)) => feedback.set(Some(error.message())),
        None => {}
    });
    let can_change = me.instance_kind == InstanceKind::Central
        && me.kind == flux_shared::session::CallerKind::Account;
    view! {
        <section class="section-panel" aria-labelledby="account-title">
            <div class="cluster-spread">
                <h2 id="account-title" class="section-title">"Mon compte"</h2>
                <button type="button" class="btn btn-secondary btn-sm"
                    on:click=move |_| leptos::task::spawn_local(logout())>"Se déconnecter"</button>
            </div>
            <div class="detail-grid">
                <div class="detail-term">"Identifiant"</div><div class="detail-val">{me.name.clone()}</div>
                <div class="detail-term">"Rôle"</div><div class="detail-val">{role_label(me.role)}</div>
            </div>
            {can_change.then(|| view! {
                <form class="form-grid-2" on:submit=move |event| {
                    event.prevent_default();
                    if next.get_untracked() != confirm.get_untracked() {
                        feedback.set(Some("Les deux mots de passe ne correspondent pas.".into()));
                        return;
                    }
                    change.dispatch(ChangePasswordRequest { current_password: current.get_untracked(), new_password: next.get_untracked() });
                    current.set(String::new());
                    next.set(String::new());
                    confirm.set(String::new());
                }>
                    {[("password-current", "Mot de passe actuel", current, "current-password"),
                      ("password-new", "Nouveau mot de passe", next, "new-password"),
                      ("password-confirm", "Confirmation", confirm, "new-password")]
                        .into_iter().map(|(id, label, value, autocomplete)| view! {
                            <div class="form-group">
                                <label for=id class="form-label">{label}</label>
                                <input id=id class="form-input" type="password" required=true autocomplete=autocomplete
                                    prop:value=move || value.get() on:input=move |event| value.set(event_target_value(&event)) />
                            </div>
                        }).collect_view()}
                    <button type="submit" class="btn btn-primary" disabled=move || change.pending().get()>"Changer le mot de passe"</button>
                </form>
            })}
            <p class="form-help" role="status">{move || feedback.get()}</p>
        </section>
    }
}
