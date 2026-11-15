//! Plugins : dépôts synchronisés, outils distribués, catalogue et réglages (ADR 0010, 0014).

mod settings;

use crate::api;
use crate::route::encode_segment;
use crate::ui::search::{matches, query};
use crate::ui::topbar_action::{ActionSpec, provide_topbar_action};
use crate::ui::{Drawer, SubTab, TabbedTable, Toasts};
use flux_shared::plugins::vocab::{CatalogItemType, CatalogSection};
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
            Toasts::notify_success("Opération sur le dépôt effectuée.");
            bump(reload);
            bump(reload_catalog);
        }
        Some(Err(error)) => Toasts::set_error(feedback, error.message()),
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
            Toasts::notify_success("Dépôt ajouté.");
            bump(reload);
        }
        Some(Err(error)) => Toasts::set_error(feedback, error.message()),
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
            Toasts::set_success(feedback, "Installation lancée.");
            bump(reload);
        }
        Some(Err(error)) => Toasts::set_error(feedback, error.message()),
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

/// Pièces d'une section, regroupées par sous-titre (seule « Autre » en a) en gardant l'ordre du catalogue.
fn group_by_subtitle(entries: Vec<CatalogEntry>) -> Vec<(Option<&'static str>, Vec<CatalogEntry>)> {
    let mut groups: Vec<(Option<&'static str>, Vec<CatalogEntry>)> = Vec::new();
    for entry in entries {
        let subtitle = entry.entry_type.subtitle();
        match groups.iter_mut().find(|(known, _)| *known == subtitle) {
            Some((_, members)) => members.push(entry),
            None => groups.push((subtitle, vec![entry])),
        }
    }
    groups
}

#[component]
fn CatalogTable(
    entries: Vec<CatalogEntry>,
    can_configure: bool,
    configuring: RwSignal<Option<String>>,
    settings_open: RwSignal<bool>,
) -> impl IntoView {
    view! {
        <div class="table-wrapper">
            <table class="data-table" aria-label="Catalogue des dépôts">
                <thead><tr>
                    <th scope="col">"Pièce"</th><th scope="col">"Type"</th><th scope="col">"Version"</th>
                    <th scope="col" class="cell-end">"Action"</th>
                </tr></thead>
                <tbody>
                    {entries.into_iter().map(|entry| {
                        let id = entry.id.clone();
                        let configurable = can_configure && entry.entry_type == CatalogItemType::Plugin;
                        view! {
                            <tr>
                                <td class="text-strong">{entry.name.clone()}<div class="text-mono text-xs text-dim">{entry.id.clone()}</div></td>
                                <td><span class="repo-badge community">{entry.entry_type.as_str()}</span></td>
                                <td><span class="tool-version-badge">{entry.version.clone()}</span></td>
                                <td class="cell-end">{configurable.then(|| view! {
                                    <button type="button" class="btn btn-secondary btn-sm"
                                        on:click=move |_| { configuring.set(Some(id.clone())); settings_open.set(true); }>"Réglages"</button>
                                })}</td>
                            </tr>
                        }
                    }).collect_view()}
                </tbody>
            </table>
        </div>
    }
}

/// Une section du catalogue : jamais de blocs (l'API ne les renvoie pas, et `section()` les écarte aussi).
#[component]
fn Catalog(section: CatalogSection, reload: RwSignal<u32>, can_configure: bool) -> impl IntoView {
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
            {move || {
                let text = query();
                let entries = catalog.get().and_then(Result::ok).unwrap_or_default().into_iter()
                    .filter(|entry| entry.entry_type.section() == Some(section) && matches(&[&entry.name, &entry.id], &text))
                    .collect();
                group_by_subtitle(entries).into_iter().map(|(subtitle, members)| view! {
                    {subtitle.map(|label| view! { <h3 class="section-subtitle">{label}</h3> })}
                    <CatalogTable entries=members can_configure=can_configure configuring=configuring settings_open=settings_open />
                }).collect_view()
            }}
        </div>
    }
}

/// Onglets de la page : une section du catalogue, ou les dépôts qui l'alimentent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PluginsTab {
    Section(CatalogSection),
    Repositories,
}

impl PluginsTab {
    fn sub_tab(self) -> SubTab {
        match self {
            Self::Section(section @ CatalogSection::Applications) => {
                SubTab::new(section.label(), "📦")
            }
            Self::Section(section @ CatalogSection::Plugins) => SubTab::new(section.label(), "🧩"),
            Self::Section(section @ CatalogSection::Flows) => SubTab::new(section.label(), "🔀"),
            Self::Section(section @ CatalogSection::Other) => SubTab::new(section.label(), "🗂️"),
            Self::Section(section @ CatalogSection::Packs) => SubTab::new(section.label(), "🎁"),
            Self::Repositories => SubTab::new("Dépôts", "🌐"),
        }
    }
}

fn visible_tabs(can_manage: bool) -> Vec<PluginsTab> {
    CatalogSection::ALL
        .into_iter()
        .map(PluginsTab::Section)
        .chain(can_manage.then_some(PluginsTab::Repositories))
        .collect()
}

#[component]
pub fn PluginsPage(me: Me) -> impl IntoView {
    let central = me.instance_kind == InstanceKind::Central;
    let can_manage = central && me.allows(Permission::ManagePlugins);
    let reload_catalog = RwSignal::new(0_u32);
    let tab = RwSignal::new(0_usize);
    let adding = RwSignal::new(false);
    let tabs = visible_tabs(can_manage);
    let sub_tabs = tabs.iter().map(|tab| tab.sub_tab()).collect();
    let repositories_index = tabs.iter().position(|tab| *tab == PluginsTab::Repositories);
    if let Some(index) = repositories_index {
        provide_topbar_action(ActionSpec {
            label: "Ajouter un dépôt",
            on_click: Callback::new(move |()| {
                tab.set(index);
                adding.set(true);
            }),
        });
    }
    view! {
        <TabbedTable aria_label="Sections des plugins" tabs=sub_tabs selected=tab>
            {move || match tabs.get(tab.get()).copied() {
                Some(PluginsTab::Section(CatalogSection::Applications)) => view! { <Tools can_install=can_manage /> }.into_any(),
                Some(PluginsTab::Section(section)) => view! { <Catalog section=section reload=reload_catalog can_configure=can_manage /> }.into_any(),
                _ => view! { <Repositories reload_catalog=reload_catalog adding=adding /> }.into_any(),
            }}
        </TabbedTable>
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

    fn entry(id: &str, entry_type: CatalogItemType) -> CatalogEntry {
        CatalogEntry {
            id: id.to_owned(),
            entry_type,
            version: "1.0.0".to_owned(),
            name: id.to_owned(),
            manifest_path: format!("{id}.json"),
            capability: None,
            current_choice: None,
            distribution: None,
            category: None,
            tags: Vec::new(),
            owner: None,
        }
    }

    #[test]
    fn tabs_follow_the_catalog_sections_and_repositories_need_permission() {
        let without = visible_tabs(false);
        assert_eq!(without.len(), CatalogSection::ALL.len());
        assert!(!without.contains(&PluginsTab::Repositories));
        let with = visible_tabs(true);
        assert_eq!(with.last(), Some(&PluginsTab::Repositories));
        for tab in with {
            assert!(!tab.sub_tab().label.is_empty());
        }
    }

    #[test]
    fn other_section_is_grouped_under_one_subtitle_per_kind() {
        let groups = group_by_subtitle(vec![
            entry("img.a", CatalogItemType::ResourceImage),
            entry("mus.a", CatalogItemType::ResourceAudio),
            entry("img.b", CatalogItemType::ResourceImage),
        ]);
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].0, Some("Images"));
        assert_eq!(groups[0].1.len(), 2);
        assert_eq!(groups[1].0, Some("Musiques et sons"));
        let plain = group_by_subtitle(vec![entry("flow.a", CatalogItemType::Flow)]);
        assert_eq!(plain.len(), 1);
        assert_eq!(plain[0].0, None);
    }
}
