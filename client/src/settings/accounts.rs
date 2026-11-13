//! Comptes humains : liste, création, rôle et désactivation (le dernier administrateur est protégé
//! par le central).

use super::account::role_label;
use crate::api;
use flux_shared::Role;
use flux_shared::accounts::{Account, CreateAccountRequest, UpdateAccountRequest};
use leptos::prelude::*;

const ACCOUNTS_PATH: &str = "/api/v1/accounts";

#[component]
pub fn AccountsSection() -> impl IntoView {
    let reload = RwSignal::new(0_u32);
    let accounts = LocalResource::new(move || {
        reload.track();
        api::get::<Vec<Account>>(ACCOUNTS_PATH)
    });
    let feedback = RwSignal::new(None::<String>);
    let update = Action::new_local(move |(id, request): &(String, UpdateAccountRequest)| {
        let (path, request) = (format!("{ACCOUNTS_PATH}/{id}"), request.clone());
        async move { api::put::<UpdateAccountRequest, Account>(&path, &request).await }
    });
    let create = Action::new_local(move |request: &CreateAccountRequest| {
        let request = request.clone();
        async move { api::post::<CreateAccountRequest, Account>(ACCOUNTS_PATH, Some(&request)).await }
    });
    let done = move |result: Option<Result<Account, api::ApiError>>| match result {
        Some(Ok(_)) => {
            feedback.set(None);
            reload.update(|count| *count = count.wrapping_add(1));
        }
        Some(Err(api::ApiError::Rejected { status: 409, .. })) => {
            feedback.set(Some(
                "Refusé : le central doit garder au moins un administrateur actif.".into(),
            ));
        }
        Some(Err(error)) => feedback.set(Some(error.message())),
        None => {}
    };
    Effect::new(move |_| done(update.value().get()));
    Effect::new(move |_| done(create.value().get()));
    let (username, password, role) = (
        RwSignal::new(String::new()),
        RwSignal::new(String::new()),
        RwSignal::new(Role::Viewer),
    );
    view! {
        <section class="section-panel" aria-labelledby="accounts-title">
            <h2 id="accounts-title" class="section-title">"Comptes"</h2>
            <div class="table-wrapper">
                <table class="data-table" aria-label="Comptes">
                    <thead><tr>
                        <th scope="col">"Identifiant"</th><th scope="col">"Rôle"</th><th scope="col">"État"</th><th scope="col" class="cell-end">"Actions"</th>
                    </tr></thead>
                    <tbody>
                        {move || accounts.get().and_then(Result::ok).unwrap_or_default().into_iter().map(|account| {
                            let (role_id, state_id) = (account.id.clone(), account.id.clone());
                            let other_role = if account.role == Role::Admin { Role::Viewer } else { Role::Admin };
                            let disabled = account.disabled;
                            view! {
                                <tr>
                                    <td class="text-strong">{account.username.clone()}</td>
                                    <td>{role_label(account.role)}</td>
                                    <td><span class=if disabled { "status-pill danger" } else { "status-pill success" }>
                                        {if disabled { "Désactivé" } else { "Actif" }}</span></td>
                                    <td class="cell-end inline-actions">
                                        <button type="button" class="btn btn-secondary btn-sm" on:click=move |_| {
                                            update.dispatch((role_id.clone(), UpdateAccountRequest { role: Some(other_role.as_str().into()), disabled: None }));
                                        }>{format!("Passer {}", role_label(other_role).to_lowercase())}</button>
                                        <button type="button" class="btn btn-sm btn-danger-quiet" on:click=move |_| {
                                            update.dispatch((state_id.clone(), UpdateAccountRequest { role: None, disabled: Some(!disabled) }));
                                        }>{if disabled { "Réactiver" } else { "Désactiver" }}</button>
                                    </td>
                                </tr>
                            }
                        }).collect_view()}
                    </tbody>
                </table>
            </div>
            <form class="form-grid-2" on:submit=move |event| {
                event.prevent_default();
                create.dispatch(CreateAccountRequest { username: username.get_untracked(), password: password.get_untracked(), role: role.get_untracked().as_str().into() });
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
                    <label for="account-role" class="form-label">"Rôle"</label>
                    <select id="account-role" class="form-select" on:change=move |event| {
                        role.set(Role::parse(&event_target_value(&event)).unwrap_or(Role::Viewer));
                    }>
                        {Role::ACCOUNT_ROLES.into_iter().map(|choice| view! {
                            <option value=choice.as_str() selected=choice == Role::Viewer>{role_label(choice)}</option>
                        }).collect_view()}
                    </select>
                </div>
                <button type="submit" class="btn btn-primary" disabled=move || create.pending().get()>"Créer le compte"</button>
            </form>
            <p class="form-help" role="status">{move || feedback.get()}</p>
        </section>
    }
}
