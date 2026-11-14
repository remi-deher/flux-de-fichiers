//! Plugins : dépôts synchronisés, outils distribués, catalogue et réglages (ADR 0010, 0014).

mod settings;

use crate::api;
use crate::route::encode_segment;
use crate::ui::search::{matches, query};
use crate::ui::{Drawer, SubNav, SubTab};
use flux_shared::plugins::vocab::CatalogItemType;
use flux_shared::plugins::{
    CatalogEntry, CreateRepositoryRequest, PluginRepository, ToolStatus, ToolStatusSummary,
};
use flux_shared::session::Me;
use flux_shared::{InstanceKind, Permission};
use leptos::prelude::*;
use serde_json::Value;
use settings::PluginSettings;

const REPOSITORIES_PATH: &str = "/api/v1/plugins/repositories";
const CATALOG_PATH: &str = "/api/v1/plugins";
const TOOLS_PATH: &str = "/api/v1/tools";

pub fn tool_status_label(status: ToolStatus) -> (&'static str, &'static str) {
    match status {
        ToolStatus::Ready => ("Installé", "status-pill success"),
        ToolStatus::Installing => ("Installation…", "status-pill warning"),
        ToolStatus::NotInstalled => ("Non installé", "status-pill info"),
    }
}

fn bump(signal: RwSignal<u32>) {
    signal.update(|count| *count = count.wrapping_add(1));
}

/// Opération sur un dépôt : `(méthode, suffixe de chemin)`.
#[derive(Clone, Copy)]
enum RepoAction {
    Toggle,
    Sync,
    Delete,
}

async fn run_repo_action(id: String, action: RepoAction) -> Result<(), api::ApiError> {
    let base = format!("{REPOSITORIES_PATH}/{}", encode_segment(&id));
    match action {
        RepoAction::Toggle => api::put::<(), Value>(&format!("{base}/toggle"), &())
            .await
            .map(|_| ()),
        RepoAction::Sync => api::post::<(), Value>(&format!("{base}/sync"), None)
            .await
            .map(|_| ()),
        RepoAction::Delete => api::delete(&base).await,
    }
}

#[component]
fn Repositories(reload_catalog: RwSignal<u32>, adding: RwSignal<bool>) -> impl IntoView {
    let reload = RwSignal::new(0_u32);
    let repositories = LocalResource::new(move || {
        reload.track();
        api::get::<Vec<PluginRepository>>(REPOSITORIES_PATH)
    });
    let feedback = RwSignal::new(None::<String>);
    let act = Action::new_local(move |(id, action): &(String, RepoAction)| {
        run_repo_action(id.clone(), *action)
    });
    Effect::new(move |_| match act.value().get() {
        Some(Ok(())) => {
            feedback.set(None);
            bump(reload);
            bump(reload_catalog);
        }
        Some(Err(error)) => feedback.set(Some(error.message())),
        None => {}
    });
    let (name, url) = (RwSignal::new(String::new()), RwSignal::new(String::new()));
    let add = Action::new_local(move |request: &CreateRepositoryRequest| {
        let request = request.clone();
        async move {
            api::post::<CreateRepositoryRequest, PluginRepository>(
                REPOSITORIES_PATH,
                Some(&request),
            )
            .await
        }
    });
    Effect::new(move |_| match add.value().get() {
        Some(Ok(_)) => {
            name.set(String::new());
            url.set(String::new());
            adding.set(false);
            bump(reload);
        }
        Some(Err(error)) => feedback.set(Some(error.message())),
        None => {}
    });
    view! {
        <div class="registre">
            <div class="table-wrapper">
                <table class="data-table" aria-label="Dépôts de plugins">
                    <thead><tr>
                        <th scope="col">"Dépôt"</th><th scope="col">"Index"</th><th scope="col">"État"</th>
                        <th scope="col">"Dernière synchronisation"</th><th scope="col" class="cell-end">"Actions"</th>
                    </tr></thead>
                    <tbody>
                        {move || repositories.get().and_then(Result::ok).unwrap_or_default().into_iter().map(|repo| {
                            let (toggle_id, sync_id, delete_id) = (repo.id.clone(), repo.id.clone(), repo.id.clone());
                            view! {
                                <tr>
                                    <td class="text-strong">{repo.name.clone()}{repo.is_official.then_some(" (officiel)")}</td>
                                    <td class="text-mono truncate">{repo.index_url.clone()}</td>
                                    <td><span class=if repo.enabled { "status-pill success" } else { "status-pill info" }>
                                        {if repo.enabled { "Actif" } else { "Désactivé" }}</span></td>
                                    <td class="text-dim">{repo.last_synced_at.clone().unwrap_or_else(|| "Jamais".into())}</td>
                                    <td class="cell-end inline-actions">
                                        <button type="button" class="btn btn-secondary btn-sm" disabled=move || act.pending().get()
                                            on:click=move |_| { act.dispatch((sync_id.clone(), RepoAction::Sync)); }>"Synchroniser"</button>
                                        <button type="button" class="btn btn-secondary btn-sm" disabled=move || act.pending().get()
                                            on:click=move |_| { act.dispatch((toggle_id.clone(), RepoAction::Toggle)); }>
                                            {if repo.enabled { "Désactiver" } else { "Activer" }}</button>
                                        {(!repo.is_official).then(|| view! {
                                            <button type="button" class="btn btn-sm btn-danger-quiet" disabled=move || act.pending().get()
                                                on:click=move |_| { act.dispatch((delete_id.clone(), RepoAction::Delete)); }>"Retirer"</button>
                                        })}
                                    </td>
                                </tr>
                            }
                        }).collect_view()}
                    </tbody>
                </table>
            </div>
            <Drawer open=adding eyebrow="Plugins" title="Ajouter un dépôt".to_owned()>
            <form class="inspection-panel" on:submit=move |event| {
                event.prevent_default();
                add.dispatch(CreateRepositoryRequest { id: None, name: name.get_untracked(), index_url: url.get_untracked() });
            }>
                <div class="form-group">
                    <label for="repo-name" class="form-label">"Nom du dépôt"</label>
                    <input id="repo-name" class="form-input" required=true prop:value=move || name.get()
                        on:input=move |event| name.set(event_target_value(&event)) />
                </div>
                <div class="form-group">
                    <label for="repo-url" class="form-label">"URL de l'index"</label>
                    <input id="repo-url" class="form-input" type="url" required=true placeholder="https://…/catalogue/index.json"
                        prop:value=move || url.get() on:input=move |event| url.set(event_target_value(&event)) />
                </div>
                <div class="inspection-drawer__actions">
                    <button type="submit" class="btn btn-primary" disabled=move || add.pending().get()>"Ajouter le dépôt"</button>
                </div>
            </form>
            </Drawer>
            <p class="form-help" role="status">{move || feedback.get()}</p>
        </div>
    }
}

#[component]
fn Tools(can_install: bool) -> impl IntoView {
    let reload = RwSignal::new(0_u32);
    let tools = LocalResource::new(move || {
        reload.track();
        api::get::<Vec<ToolStatusSummary>>(TOOLS_PATH)
    });
    let feedback = RwSignal::new(None::<String>);
    let install = Action::new_local(move |id: &String| {
        let path = format!("{TOOLS_PATH}/{}/install", encode_segment(id));
        async move { api::post::<(), Value>(&path, None).await }
    });
    Effect::new(move |_| match install.value().get() {
        Some(Ok(_)) => {
            feedback.set(Some("Installation lancée.".into()));
            bump(reload);
        }
        Some(Err(error)) => feedback.set(Some(error.message())),
        None => {}
    });
    view! {
        <div class="registre">
            <div class="table-wrapper">
                <table class="data-table" aria-label="Outils distribués">
                    <thead><tr>
                        <th scope="col">"Outil"</th><th scope="col">"Version"</th><th scope="col">"Rôle"</th>
                        <th scope="col">"État"</th><th scope="col" class="cell-end">"Action"</th>
                    </tr></thead>
                    <tbody>
                        {move || { let text = query(); tools.get().and_then(Result::ok).unwrap_or_default().into_iter().filter(|tool| matches(&[&tool.name, &tool.id], &text)).map(|tool| {
                            let (label, class) = tool_status_label(tool.status);
                            let id = tool.id.clone();
                            let installable = can_install && tool.status == ToolStatus::NotInstalled;
                            view! {
                                <tr>
                                    <td class="text-strong" title=tool.description.clone()>{tool.name.clone()}</td>
                                    <td class="text-mono">{tool.version.clone()}</td>
                                    <td>{tool.capability.as_str()}{tool.current_choice.then_some(" (choix actuel)")}</td>
                                    <td><span class=class>{label}</span></td>
                                    <td class="cell-end">{installable.then(|| view! {
                                        <button type="button" class="btn btn-secondary btn-sm" disabled=move || install.pending().get()
                                            on:click=move |_| { install.dispatch(id.clone()); }>"Installer"</button>
                                    })}</td>
                                </tr>
                            }
                        }).collect_view() }}
                    </tbody>
                </table>
            </div>
            <p class="form-help" role="status">{move || feedback.get()}</p>
        </div>
    }
}

#[component]
fn Catalog(reload: RwSignal<u32>, can_configure: bool) -> impl IntoView {
    let catalog = LocalResource::new(move || {
        reload.track();
        api::get::<Vec<CatalogEntry>>(CATALOG_PATH)
    });
    let configuring = RwSignal::new(None::<String>);
    let settings_open = RwSignal::new(false);
    let title = Signal::derive(move || configuring.get().unwrap_or_default());
    view! {
        <Drawer open=settings_open eyebrow="Réglages du plugin" title=title>
            {move || configuring.get().map(|id| view! { <PluginSettings plugin_id=id /> })}
        </Drawer>
        <div class="registre">
            <div class="table-wrapper">
                <table class="data-table" aria-label="Catalogue des dépôts">
                    <thead><tr>
                        <th scope="col">"Pièce"</th><th scope="col">"Type"</th><th scope="col">"Version"</th>
                        <th scope="col" class="cell-end">"Action"</th>
                    </tr></thead>
                    <tbody>
                        {move || { let text = query(); catalog.get().and_then(Result::ok).unwrap_or_default().into_iter().filter(|entry| matches(&[&entry.name, &entry.id], &text)).map(|entry| {
                            let id = entry.id.clone();
                            let configurable = can_configure && entry.entry_type == CatalogItemType::Plugin;
                            view! {
                                <tr>
                                    <td class="text-strong">{entry.name.clone()}<div class="text-mono text-xs text-dim">{entry.id.clone()}</div></td>
                                    <td>{entry.entry_type.as_str()}</td>
                                    <td class="text-mono">{entry.version.clone()}</td>
                                    <td class="cell-end">{configurable.then(|| view! {
                                        <button type="button" class="btn btn-secondary btn-sm"
                                            on:click=move |_| { configuring.set(Some(id.clone())); settings_open.set(true); }>"Réglages"</button>
                                    })}</td>
                                </tr>
                            }
                        }).collect_view() }}
                    </tbody>
                </table>
            </div>
        </div>
    }
}

#[component]
pub fn PluginsPage(me: Me) -> impl IntoView {
    let central = me.instance_kind == InstanceKind::Central;
    let can_manage = central && me.allows(Permission::ManagePlugins);
    let reload_catalog = RwSignal::new(0_u32);
    let tab = RwSignal::new(0_usize);
    let adding = RwSignal::new(false);
    let mut tabs = vec![SubTab::new("Catalogue", "🧩"), SubTab::new("Outils", "🛠️")];
    if can_manage {
        tabs.push(SubTab::new("Dépôts", "🌐"));
    }
    let actions = can_manage.then(|| view! {
        <button type="button" class="btn btn-primary btn-sm" on:click=move |_| { tab.set(2); adding.set(true); }>"Ajouter un dépôt"</button>
    }.into_any());
    view! {
        <div class="tab-panel" role="tabpanel">
            <SubNav aria_label="Sous-navigation des plugins" tabs=tabs selected=tab actions=actions.unwrap_or_else(|| ().into_any()) />
            {move || match tab.get() {
                0 => view! { <Catalog reload=reload_catalog can_configure=can_manage /> }.into_any(),
                1 => view! { <Tools can_install=can_manage /> }.into_any(),
                _ => view! { <Repositories reload_catalog=reload_catalog adding=adding /> }.into_any(),
            }}
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_tool_state_has_a_label() {
        for status in [
            ToolStatus::Ready,
            ToolStatus::Installing,
            ToolStatus::NotInstalled,
        ] {
            assert!(!tool_status_label(status).0.is_empty());
        }
    }
}
