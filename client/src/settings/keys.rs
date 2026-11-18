//! Clés d'API : liste, création (secret affiché une seule fois) et révocation.

use super::account::role_label;
use crate::api;
use crate::ui::search::{matches, query};
use crate::ui::{Drawer, Icon, IconName, SectionHeader, Toasts};
use crate::workers::origin;
use flux_shared::accounts::{ApiKeyRecord, CreateKeyRequest, CreatedKey};
use flux_shared::{Permission, Role};
use leptos::prelude::*;

const KEYS_PATH: &str = "/api/v1/api-keys";
/// Rôles possibles d'une clé : `worker` pour les nœuds, `mcp` pour l'automatisation MCP.
const KEY_ROLES: [Role; 4] = [Role::Admin, Role::Mcp, Role::Worker, Role::Viewer];

#[derive(Clone, Debug, PartialEq, Eq)]
struct RevealedKey {
    key: String,
    is_mcp: bool,
}

/// Génère un extrait `.mcp.json` prêt à copier pour connecter un client MCP au central.
pub fn build_mcp_client_snippet(central_origin: &str, key: &str) -> String {
    let base = central_origin.trim().trim_end_matches('/');
    let base = if base.is_empty() {
        "http://localhost:3000"
    } else {
        base
    };
    format!(
        "{{\n  \"mcpServers\": {{\n    \"flux-de-fichiers\": {{\n      \"type\": \"http\",\n      \"url\": \"{base}/mcp\",\n      \"headers\": {{\n        \"Authorization\": \"Bearer {key}\"\n      }}\n    }}\n  }}\n}}"
    )
}

fn format_scope_summary(scopes: &Option<Vec<Permission>>) -> String {
    match scopes {
        None => "Complète".to_owned(),
        Some(list) if list.is_empty() => "Aucune".to_owned(),
        Some(list) if list.len() <= 2 => list
            .iter()
            .map(|p| p.label())
            .collect::<Vec<_>>()
            .join(", "),
        Some(list) => format!("{} droits", list.len()),
    }
}

#[component]
pub fn KeysSection() -> impl IntoView {
    let reload = RwSignal::new(0_u32);
    let keys = LocalResource::new(move || {
        reload.track();
        api::get::<Vec<ApiKeyRecord>>(KEYS_PATH)
    });
    let feedback = RwSignal::new(None::<String>);
    let revealed = RwSignal::new(None::<RevealedKey>);
    let creating = RwSignal::new(false);
    let (name, role) = (RwSignal::new(String::new()), RwSignal::new(Role::Worker));
    let full_scope = RwSignal::new(true);
    let selected_scopes = RwSignal::new(Role::Worker.permissions());
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
            let is_mcp = Role::parse(&created.record.role) == Some(Role::Mcp);
            revealed.set(Some(RevealedKey {
                key: created.key,
                is_mcp,
            }));
            name.set(String::new());
            full_scope.set(true);
            selected_scopes.set(role.get_untracked().permissions());
            feedback.set(None);
            Toasts::notify_success("Clé d'API créée.");
            refresh();
        }
        Some(Err(error)) => Toasts::set_error(feedback, error.message()),
        None => {}
    });
    Effect::new(move |_| match revoke.value().get() {
        Some(Ok(())) => {
            feedback.set(None);
            Toasts::notify_success("Clé d'API révoquée.");
            refresh();
        }
        Some(Err(error)) => Toasts::set_error(feedback, error.message()),
        None => {}
    });
    let header_actions = view! {
        <button type="button" class="btn btn-primary btn-sm" on:click=move |_| creating.set(true)>
            <Icon name=IconName::Plus size=14 />
            <span>"Créer une clé"</span>
        </button>
    }
    .into_any();
    let visible_rows = Signal::derive(move || {
        let text = query();
        let list = keys.get().and_then(Result::ok).unwrap_or_default();
        let section_hit = !text.trim().is_empty()
            && matches(
                &[
                    "clés d'api",
                    "cles d'api",
                    "api",
                    "jeton",
                    "token",
                    "worker",
                ],
                &text,
            );
        if text.trim().is_empty() || section_hit {
            (true, list)
        } else {
            let filtered: Vec<ApiKeyRecord> = list
                .into_iter()
                .filter(|key| {
                    let role = match Role::parse(&key.role) {
                        Some(r) => role_label(r),
                        None => key.role.as_str(),
                    };
                    matches(&[&key.name, role, &key.role], &text)
                })
                .collect();
            (!filtered.is_empty(), filtered)
        }
    });
    view! {
        <section class="section-panel settings-section" hidden=move || {
            keys.get().is_some() && !visible_rows.get().0
        }>
            <SectionHeader
                title="Clés d'API"
                subtitle="Jetons d'authentification des workers et accès automatisés."
                actions=header_actions
            />
            <div class="registre">
            {move || revealed.get().map(|item| {
                let snippet = item
                    .is_mcp
                    .then(|| build_mcp_client_snippet(&origin(), &item.key));
                view! {
                    <div class="settings-alert" role="status">
                        <p>"Copiez cette clé maintenant : elle ne sera plus affichée."</p>
                        <pre class="code-block-wrapper"><code>{item.key}</code></pre>
                        {snippet.map(|cfg| view! {
                            <div class="stack-xs">
                                <p class="form-help">"Configuration prête à copier pour Claude Code / Cursor (`.mcp.json`) :"</p>
                                <pre class="code-block-wrapper"><code>{cfg}</code></pre>
                            </div>
                        })}
                        <button type="button" class="btn btn-secondary btn-sm" on:click=move |_| revealed.set(None)>"C'est noté"</button>
                    </div>
                }
            })}
            <div class="table-wrapper">
                <table class="data-table" aria-label="Clés d'API">
                    <thead><tr>
                        <th scope="col">"Nom"</th><th scope="col">"Rôle"</th><th scope="col">"Droits d'accès"</th>
                        <th scope="col">"Dernier usage"</th><th scope="col">"État"</th><th scope="col" class="cell-end">"Action"</th>
                    </tr></thead>
                    <tbody>
                        {move || visible_rows.get().1.into_iter().map(|key| {
                            let id = key.id.clone();
                            let revoked = key.revoked_at.is_some();
                            let role = Role::parse(&key.role).map_or(key.role.clone(), |role| role_label(role).to_owned());
                            let scope_text = format_scope_summary(&key.scopes);
                            view! {
                                <tr>
                                    <td class="text-strong">{key.name.clone()}</td>
                                    <td>{role}</td>
                                    <td class="text-dim">{scope_text}</td>
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
                let current_role = role.get_untracked();
                let scopes = if full_scope.get_untracked() || current_role == Role::Worker {
                    None
                } else {
                    let chosen: Vec<Permission> = selected_scopes
                        .get_untracked()
                        .into_iter()
                        .filter(|perm| current_role.allows(*perm))
                        .collect();
                    if chosen.is_empty() {
                        Toasts::set_error(feedback, "Sélectionnez au moins un droit d'accès.");
                        return;
                    }
                    Some(chosen)
                };
                create.dispatch(CreateKeyRequest {
                    name: name.get_untracked(),
                    role: current_role.as_str().into(),
                    scopes,
                });
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
                        let next = Role::parse(&event_target_value(&event)).unwrap_or(Role::Worker);
                        role.set(next);
                        selected_scopes.set(next.permissions());
                    }>
                        {KEY_ROLES.into_iter().map(|choice| view! {
                            <option value=choice.as_str() selected=choice == Role::Worker>{role_label(choice)}</option>
                        }).collect_view()}
                    </select>
                </div>
                {move || (role.get() != Role::Worker).then(|| {
                    let role_perms = role.get().permissions();
                    view! {
                        <div class="form-group">
                            <span class="form-label">"Droits d'accès"</span>
                            <label class="form-check">
                                <input type="checkbox"
                                    prop:checked=move || full_scope.get()
                                    on:change=move |event| full_scope.set(event_target_checked(&event)) />
                                <span>"Tous les droits du rôle"</span>
                            </label>
                            {move || (!full_scope.get()).then(|| {
                                let perms = role_perms.clone();
                                view! {
                                    <div class="cluster">
                                        <button type="button" class="btn btn-secondary btn-sm"
                                            on:click={
                                                let all = perms.clone();
                                                move |_| selected_scopes.set(all.clone())
                                            }>"Tout cocher"</button>
                                        <button type="button" class="btn btn-secondary btn-sm"
                                            on:click=move |_| selected_scopes.set(Vec::new())>"Tout décocher"</button>
                                    </div>
                                    <div class="stack-xs">
                                        {perms.into_iter().map(|perm| view! {
                                            <label class="form-check">
                                                <input type="checkbox"
                                                    prop:checked=move || selected_scopes.get().contains(&perm)
                                                    on:change=move |event| {
                                                        let checked = event_target_checked(&event);
                                                        selected_scopes.update(|list| {
                                                            if checked {
                                                                if !list.contains(&perm) {
                                                                    list.push(perm);
                                                                }
                                                            } else {
                                                                list.retain(|item| *item != perm);
                                                            }
                                                        });
                                                    } />
                                                <span>{perm.label()}</span>
                                            </label>
                                        }).collect_view()}
                                    </div>
                                }
                            })}
                        </div>
                    }
                })}
                <div class="inspection-drawer__actions">
                    <button type="submit" class="btn btn-primary" disabled=move || create.pending().get()>"Créer la clé"</button>
                </div>
            </form>
            </Drawer>
            <p class="form-help" role="status">{move || feedback.get()}</p>
            </div>
        </section>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mcp_client_snippet_embeds_origin_and_bearer_token() {
        let json = build_mcp_client_snippet("http://192.168.1.50:18480/", "flx_secret");
        assert!(json.contains("\"url\": \"http://192.168.1.50:18480/mcp\""));
        assert!(json.contains("\"Authorization\": \"Bearer flx_secret\""));
    }
}
