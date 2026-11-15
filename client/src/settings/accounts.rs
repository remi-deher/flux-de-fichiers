//! Comptes humains : liste, création, rôle et désactivation (le dernier administrateur est protégé
//! par le central).

use super::account::role_label;
use crate::api;
use crate::ui::search::{matches, query};
use crate::ui::{Drawer, Icon, IconName, SectionHeader, Toasts};
use flux_shared::Role;
use flux_shared::accounts::{Account, CreateAccountRequest, UpdateAccountRequest};
use leptos::prelude::*;

const ACCOUNTS_PATH: &str = "/api/v1/accounts";

/// Demande l'adresse à enregistrer ; `None` si la saisie est annulée, chaîne vide pour effacer.
fn ask_email(current: Option<&str>) -> Option<String> {
    window()
        .prompt_with_message_and_default(
            "Adresse e-mail du compte (vide pour l'effacer) :",
            current.unwrap_or_default(),
        )
        .ok()
        .flatten()
}

#[component]
pub fn AccountsSection() -> impl IntoView {
    let reload = RwSignal::new(0_u32);
    let accounts = LocalResource::new(move || {
        reload.track();
        api::get::<Vec<Account>>(ACCOUNTS_PATH)
    });
    let feedback = RwSignal::new(None::<String>);
    let creating = RwSignal::new(false);
    let update = Action::new_local(move |(id, request): &(String, UpdateAccountRequest)| {
        let (path, request) = (format!("{ACCOUNTS_PATH}/{id}"), request.clone());
        async move { api::put::<UpdateAccountRequest, Account>(&path, &request).await }
    });
    let create = Action::new_local(move |request: &CreateAccountRequest| {
        let request = request.clone();
        async move { api::post::<CreateAccountRequest, Account>(ACCOUNTS_PATH, Some(&request)).await }
    });
    let handle_result = move |result: Option<Result<Account, api::ApiError>>,
                              success_msg: &'static str| {
        match result {
            Some(Ok(_)) => {
                feedback.set(None);
                creating.set(false);
                Toasts::notify_success(success_msg);
                reload.update(|count| *count = count.wrapping_add(1));
            }
            Some(Err(api::ApiError::Rejected { status: 409, .. })) => {
                Toasts::set_error(
                    feedback,
                    "Refusé : adresse e-mail déjà utilisée, ou le central doit garder au moins un administrateur actif.",
                );
            }
            Some(Err(error)) => Toasts::set_error(feedback, error.message()),
            None => {}
        }
    };
    Effect::new(move |_| handle_result(update.value().get(), "Compte mis à jour."));
    Effect::new(move |_| handle_result(create.value().get(), "Compte créé."));
    let (username, password, role, email) = (
        RwSignal::new(String::new()),
        RwSignal::new(String::new()),
        RwSignal::new(Role::Viewer),
        RwSignal::new(String::new()),
    );
    let header_actions = view! {
        <button type="button" class="btn btn-primary btn-sm" on:click=move |_| creating.set(true)>
            <Icon name=IconName::Plus size=14 />
            <span>"Créer un compte"</span>
        </button>
    }
    .into_any();
    let visible_rows = Signal::derive(move || {
        let text = query();
        let list = accounts.get().and_then(Result::ok).unwrap_or_default();
        let section_hit = !text.trim().is_empty()
            && matches(
                &[
                    "utilisateurs",
                    "comptes",
                    "permissions",
                    "accès",
                    "rôle",
                    "e-mail",
                    "email",
                ],
                &text,
            );
        if text.trim().is_empty() || section_hit {
            (true, list)
        } else {
            let filtered: Vec<Account> = list
                .into_iter()
                .filter(|account| {
                    matches(
                        &[
                            &account.username,
                            account.email.as_deref().unwrap_or(""),
                            role_label(account.role),
                        ],
                        &text,
                    )
                })
                .collect();
            (!filtered.is_empty(), filtered)
        }
    });
    view! {
        <section class="section-panel settings-section" hidden=move || {
            accounts.get().is_some() && !visible_rows.get().0
        }>
            <SectionHeader
                title="Utilisateurs"
                subtitle="Comptes autorisés à piloter l'installation et leurs permissions."
                actions=header_actions
            />
            <div class="registre">
                <div class="table-wrapper">
                <table class="data-table" aria-label="Comptes">
                    <thead><tr>
                        <th scope="col">"Identifiant"</th><th scope="col">"E-mail"</th><th scope="col">"Rôle"</th><th scope="col">"État"</th><th scope="col" class="cell-end">"Actions"</th>
                    </tr></thead>
                    <tbody>
                        {move || visible_rows.get().1.into_iter().map(|account| {
                            let (role_id, state_id, email_id) = (account.id.clone(), account.id.clone(), account.id.clone());
                            let current_email = account.email.clone();
                            let other_role = if account.role == Role::Admin { Role::Viewer } else { Role::Admin };
                            let disabled = account.disabled;
                            view! {
                                <tr>
                                    <td class="text-strong">{account.username.clone()}</td>
                                    <td>{account.email.clone().unwrap_or_else(|| "—".into())}</td>
                                    <td>{role_label(account.role)}</td>
                                    <td><span class=if disabled { "status-pill danger" } else { "status-pill success" }>
                                        {if disabled { "Désactivé" } else { "Actif" }}</span></td>
                                    <td class="cell-end inline-actions">
                                        <button type="button" class="btn btn-secondary btn-sm" on:click=move |_| {
                                            if let Some(email) = ask_email(current_email.as_deref()) {
                                                update.dispatch((email_id.clone(), UpdateAccountRequest { role: None, disabled: None, email: Some(email) }));
                                            }
                                        }>"E-mail"</button>
                                        <button type="button" class="btn btn-secondary btn-sm" on:click=move |_| {
                                            update.dispatch((role_id.clone(), UpdateAccountRequest { role: Some(other_role.as_str().into()), disabled: None, email: None }));
                                        }>{format!("Passer {}", role_label(other_role).to_lowercase())}</button>
                                        <button type="button" class="btn btn-sm btn-danger-quiet" on:click=move |_| {
                                            update.dispatch((state_id.clone(), UpdateAccountRequest { role: None, disabled: Some(!disabled), email: None }));
                                        }>{if disabled { "Réactiver" } else { "Désactiver" }}</button>
                                    </td>
                                </tr>
                            }
                        }).collect_view()}
                    </tbody>
                </table>
            </div>
            <Drawer open=creating eyebrow="Utilisateurs" title="Nouveau compte".to_owned()>
            <form class="inspection-panel" on:submit=move |event| {
                event.prevent_default();
                let email_value = email.get_untracked();
                create.dispatch(CreateAccountRequest { username: username.get_untracked(), password: password.get_untracked(), role: role.get_untracked().as_str().into(), email: Some(email_value).filter(|value| !value.trim().is_empty()) });
                email.set(String::new());
                username.set(String::new());
                password.set(String::new());
            }>
                <div class="form-group">
                    <label for="account-username" class="form-label">"Identifiant"</label>
                    <input id="account-username" class="form-input" required=true autocomplete="off"
                        prop:value=move || username.get() on:input=move |event| username.set(event_target_value(&event)) />
                </div>
                <div class="form-group">
                    <label for="account-password" class="form-label">"Mot de passe initial"</label>
                    <input id="account-password" class="form-input" type="password" required=true autocomplete="new-password"
                        prop:value=move || password.get() on:input=move |event| password.set(event_target_value(&event)) />
                </div>
                <div class="form-group">
                    <label for="account-email" class="form-label">"Adresse e-mail (facultative)"</label>
                    <input id="account-email" class="form-input" type="email" autocomplete="off"
                        prop:value=move || email.get() on:input=move |event| email.set(event_target_value(&event)) />
                    <p class="form-help">"Pour recevoir le lien « mot de passe oublié »."</p>
                </div>
                <div class="form-group">
                    <label for="account-role" class="form-label">"Rôle"</label>
                    <select id="account-role" class="form-select" on:change=move |event| {
                        role.set(Role::parse(&event_target_value(&event)).unwrap_or(Role::Viewer));
                    }>
                        {Role::ACCOUNT_ROLES.into_iter().map(|choice| view! {
                            <option value=choice.as_str() selected=choice == Role::Viewer>{role_label(choice)}</option>
                        }).collect_view()}
                    </select>
                </div>
                <div class="inspection-drawer__actions">
                    <button type="submit" class="btn btn-primary" disabled=move || create.pending().get()>"Créer le compte"</button>
                </div>
            </form>
            </Drawer>
            <p class="form-help" role="status">{move || feedback.get()}</p>
            </div>
        </section>
    }
}
