//! Mon compte : identité, changement de mot de passe et déconnexion.

use crate::api;
use crate::shell::instance_role_badge;
use crate::ui::{Drawer, SectionHeader, Toasts};
use crate::url_state::{DrawerKind, use_create_drawer};
use forgearr_shared::accounts::ChangePasswordRequest;
use forgearr_shared::session::Me;
use leptos::prelude::*;

const PASSWORD_PATH: &str = "/api/v1/account/password";
const LOGOUT_PATH: &str = "/api/v1/auth/logout";

pub fn role_label(role: forgearr_shared::Role) -> &'static str {
    match role {
        forgearr_shared::Role::Admin => "Administrateur",
        forgearr_shared::Role::Mcp => "MCP",
        forgearr_shared::Role::Worker => "Worker",
        forgearr_shared::Role::Viewer => "Lecteur",
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
    let changing_password = use_create_drawer(DrawerKind::Password);
    let feedback = RwSignal::new(None::<String>);
    let change = Action::new_local(move |request: &ChangePasswordRequest| {
        let request = request.clone();
        async move { api::put_empty(PASSWORD_PATH, &request).await }
    });
    Effect::new(move |_| match change.value().get() {
        Some(Ok(())) => {
            changing_password.set(false);
            feedback.set(None);
            Toasts::notify_success("Mot de passe changé ; vos autres sessions sont fermées.");
        }
        Some(Err(api::ApiError::Unauthorized)) => {
            feedback.set(Some("Mot de passe actuel incorrect.".into()))
        }
        Some(Err(error)) => feedback.set(Some(error.message())),
        None => {}
    });
    let can_change = me.kind == forgearr_shared::session::CallerKind::Account;
    view! {
        <section class="section-panel settings-section">
            <SectionHeader
                title="Mon compte"
                subtitle="Identité active et sécurité de connexion."
                actions=view! {
                    <div class="cluster">
                        {can_change.then(|| view! {
                            <button type="button" class="btn btn-secondary btn-sm"
                                on:click=move |_| { feedback.set(None); changing_password.set(true); }>
                                "Mot de passe"
                            </button>
                        })}
                        <button type="button" class="btn btn-secondary btn-sm"
                            on:click=move |_| leptos::task::spawn_local(logout())>
                            "Se déconnecter"
                        </button>
                    </div>
                }.into_any()
            />
            <div class="detail-grid">
                <div class="detail-row">
                    <span class="detail-term">"Identifiant"</span>
                    <span class="detail-val">{me.name.clone()}</span>
                </div>
                <div class="detail-row">
                    <span class="detail-term">"Rôle"</span>
                    <span class="detail-val">{role_label(me.role)}</span>
                </div>
                <div class="detail-row">
                    <span class="detail-term">"Installation"</span>
                    <span class="detail-val">{instance_role_badge(me.instance_kind)}</span>
                </div>
            </div>
            {can_change.then(|| view! {
                <Drawer open=changing_password eyebrow="Sécurité" title="Changer le mot de passe".to_owned()>
                    <form class="stack-form" on:submit=move |event| {
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
                        {move || feedback.get().map(|err| view! {
                            <p class="form-error" role="status">{err}</p>
                        })}
                        <div class="cluster-end">
                            <button type="submit" class="btn btn-primary" disabled=move || change.pending().get()>
                                "Changer le mot de passe"
                            </button>
                        </div>
                    </form>
                </Drawer>
            })}
        </section>
    }
}
