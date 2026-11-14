//! Clés d'API : liste, création (secret affiché une seule fois) et révocation.

use super::account::role_label;
use crate::api;
use crate::ui::Drawer;
use flux_shared::Role;
use flux_shared::accounts::{ApiKeyRecord, CreateKeyRequest, CreatedKey};
use leptos::prelude::*;

const KEYS_PATH: &str = "/api/v1/api-keys";
/// Rôles possibles d'une clé : une clé worker sert au protocole des workers.
const KEY_ROLES: [Role; 3] = [Role::Admin, Role::Worker, Role::Viewer];

#[component]
pub fn KeysSection() -> impl IntoView {
    let reload = RwSignal::new(0_u32);
    let keys = LocalResource::new(move || {
        reload.track();
        api::get::<Vec<ApiKeyRecord>>(KEYS_PATH)
    });
    let feedback = RwSignal::new(None::<String>);
    let revealed = RwSignal::new(None::<String>);
    let creating = RwSignal::new(false);
    let (name, role) = (RwSignal::new(String::new()), RwSignal::new(Role::Worker));
    let create = Action::new_local(move |request: &CreateKeyRequest| {
        let request = request.clone();
        async move { api::post::<CreateKeyRequest, CreatedKey>(KEYS_PATH, Some(&request)).await }
    });
    let revoke = Action::new_local(move |id: &String| {
        let path = format!("{KEYS_PATH}/{id}");
        async move { api::delete(&path).await }
    });
    let refresh = move || reload.update(|count| *count = count.wrapping_add(1));
    Effect::new(move |_| match create.value().get() {
        Some(Ok(created)) => {
            creating.set(false);
            revealed.set(Some(created.key));
            name.set(String::new());
            refresh();
        }
        Some(Err(error)) => feedback.set(Some(error.message())),
        None => {}
    });
    Effect::new(move |_| match revoke.value().get() {
        Some(Ok(())) => refresh(),
        Some(Err(error)) => feedback.set(Some(error.message())),
        None => {}
    });
    view! {
        <div class="registre">
            <div class="cluster-end">
                <button type="button" class="btn btn-primary btn-sm" on:click=move |_| creating.set(true)>"Créer une clé"</button>
            </div>
            {move || revealed.get().map(|key| view! {
                <div class="settings-alert" role="status">
                    <p>"Copiez cette clé maintenant : elle ne sera plus affichée."</p>
                    <pre class="code-block-wrapper"><code>{key}</code></pre>
                    <button type="button" class="btn btn-secondary btn-sm" on:click=move |_| revealed.set(None)>"C'est noté"</button>
                </div>
            })}
            <div class="table-wrapper">
                <table class="data-table" aria-label="Clés d'API">
                    <thead><tr>
                        <th scope="col">"Nom"</th><th scope="col">"Rôle"</th><th scope="col">"Dernier usage"</th>
                        <th scope="col">"État"</th><th scope="col" class="cell-end">"Action"</th>
                    </tr></thead>
                    <tbody>
                        {move || keys.get().and_then(Result::ok).unwrap_or_default().into_iter().map(|key| {
                            let id = key.id.clone();
                            let revoked = key.revoked_at.is_some();
                            let role = Role::parse(&key.role).map_or(key.role.clone(), |role| role_label(role).to_owned());
                            view! {
                                <tr>
                                    <td class="text-strong">{key.name.clone()}</td>
                                    <td>{role}</td>
                                    <td class="text-dim">{key.last_used_at.clone().unwrap_or_else(|| "Jamais".into())}</td>
                                    <td><span class=if revoked { "status-pill danger" } else { "status-pill success" }>
                                        {if revoked { "Révoquée" } else { "Active" }}</span></td>
                                    <td class="cell-end">{(!revoked).then(|| view! {
                                        <button type="button" class="btn btn-sm btn-danger-quiet" disabled=move || revoke.pending().get()
                                            on:click=move |_| { revoke.dispatch(id.clone()); }>"Révoquer"</button>
                                    })}</td>
                                </tr>
                            }
                        }).collect_view()}
                    </tbody>
                </table>
            </div>
            <Drawer open=creating eyebrow="Clés d'API" title="Nouvelle clé".to_owned()>
            <form class="inspection-panel" on:submit=move |event| {
                event.prevent_default();
                create.dispatch(CreateKeyRequest { name: name.get_untracked(), role: role.get_untracked().as_str().into() });
            }>
                <div class="form-group">
                    <label for="key-name" class="form-label">"Nom de la clé"</label>
                    <input id="key-name" class="form-input" required=true autocomplete="off" placeholder="nas-salon"
                        prop:value=move || name.get() on:input=move |event| name.set(event_target_value(&event)) />
                    <p class="form-help">"Pour un worker, le nom de la clé devient son nom."</p>
                </div>
                <div class="form-group">
                    <label for="key-role" class="form-label">"Rôle"</label>
                    <select id="key-role" class="form-select" on:change=move |event| {
                        role.set(Role::parse(&event_target_value(&event)).unwrap_or(Role::Worker));
                    }>
                        {KEY_ROLES.into_iter().map(|choice| view! {
                            <option value=choice.as_str() selected=choice == Role::Worker>{role_label(choice)}</option>
                        }).collect_view()}
                    </select>
                </div>
                <div class="inspection-drawer__actions">
                    <button type="submit" class="btn btn-primary" disabled=move || create.pending().get()>"Créer la clé"</button>
                </div>
            </form>
            </Drawer>
            <p class="form-help" role="status">{move || feedback.get()}</p>
        </div>
    }
}
