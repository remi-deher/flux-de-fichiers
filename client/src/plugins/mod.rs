//! Plugins : catalogue d'extensions, outils distribués, dépôts synchronisés et réglages (ADR 0010, 0014).

mod settings;

use crate::api;
use crate::route::encode_segment;
use crate::ui::filters::{Filters, PluginKindFilter, plugin_kind};
use crate::ui::search::{matches, query};
use crate::ui::topbar_action::{ActionSpec, provide_topbar_action};
use crate::ui::{
    Drawer, EmptyState, IconName, MetricTone, ResourceCard, SectionHeader, StatItem, StatStrip,
    Tag, TagTone, Toasts,
};
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

/// Opération sur un dépôt : bascule, synchronisation ou suppression.
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

/// Synthèse `(catalogue, plugins, modèles_et_packs)` des entrées exposées hors applications.
pub fn summarize_catalog(entries: &[CatalogEntry]) -> (usize, usize, usize) {
    let mut total = 0_usize;
    let mut plugins = 0_usize;
    let mut flows_and_packs = 0_usize;
    for entry in entries {
        match entry.entry_type.section() {
            Some(CatalogSection::Plugins) => {
                total += 1;
                plugins += 1;
            }
            Some(CatalogSection::Flows | CatalogSection::Packs) => {
                total += 1;
                flows_and_packs += 1;
            }
            Some(CatalogSection::Other) => {
                total += 1;
            }
            Some(CatalogSection::Applications) | None => {}
        }
    }
    (total, plugins, flows_and_packs)
}

/// Filtre les entrées du catalogue par catégorie active et recherche textuelle.
pub fn filter_catalog(
    entries: Vec<CatalogEntry>,
    kind: PluginKindFilter,
    text: &str,
) -> Vec<CatalogEntry> {
    entries
        .into_iter()
        .filter(|entry| kind.accepts(entry) && catalog_entry_matches(entry, text))
        .collect()
}

fn catalog_entry_matches(entry: &CatalogEntry, text: &str) -> bool {
    let mut fields: Vec<&str> = Vec::with_capacity(4 + entry.tags.len());
    fields.push(&entry.name);
    fields.push(&entry.id);
    if let Some(cap) = entry.capability {
        fields.push(cap.as_str());
    }
    if let Some(sub) = entry.entry_type.subtitle() {
        fields.push(sub);
    }
    for tag in &entry.tags {
        fields.push(tag.as_str());
    }
    matches(&fields, text)
}

fn section_icon(entry_type: CatalogItemType) -> IconName {
    match entry_type.section() {
        Some(CatalogSection::Plugins) => IconName::Plugins,
        Some(CatalogSection::Flows) => IconName::Flows,
        Some(CatalogSection::Packs) => IconName::Download,
        Some(CatalogSection::Applications) => IconName::Package,
        Some(CatalogSection::Other) | None => IconName::Layers,
    }
}

fn section_badge_label(entry_type: CatalogItemType) -> &'static str {
    if let Some(sub) = entry_type.subtitle() {
        return sub;
    }
    match entry_type.section() {
        Some(CatalogSection::Plugins) => "Plugin",
        Some(CatalogSection::Flows) => "Flow",
        Some(CatalogSection::Packs) => "Pack",
        Some(CatalogSection::Applications) => "Application",
        Some(CatalogSection::Other) | None => "Autre",
    }
}

fn section_fallback_desc(entry_type: CatalogItemType) -> &'static str {
    match entry_type.section() {
        Some(CatalogSection::Plugins) => "Intégration et blocs exécutables dans vos flux.",
        Some(CatalogSection::Flows) => "Modèle de pipeline nodal prêt à l'emploi.",
        Some(CatalogSection::Packs) => "Ensemble groupé d'extensions et de ressources.",
        Some(CatalogSection::Applications) => "Outil externe installé sur les nœuds.",
        Some(CatalogSection::Other) | None => "Ressource réutilisable par les pipelines.",
    }
}

#[component]
fn ReposSection(
    repositories: LocalResource<Result<Vec<PluginRepository>, api::ApiError>>,
    reload_repos: RwSignal<u32>,
    reload_catalog: RwSignal<u32>,
    adding: RwSignal<bool>,
) -> impl IntoView {
    let feedback = RwSignal::new(None::<String>);
    let act = Action::new_local(move |(id, action): &(String, RepoAction)| {
        run_repo_action(id.clone(), *action)
    });
    Effect::new(move |_| match act.value().get() {
        Some(Ok(())) => {
            feedback.set(None);
            Toasts::notify_success("Opération sur le dépôt effectuée.");
            bump(reload_repos);
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
            bump(reload_repos);
        }
        Some(Err(error)) => Toasts::set_error(feedback, error.message()),
        None => {}
    });
    view! {
        <section class="section-panel">
            <SectionHeader
                title="Dépôts de plugins"
                subtitle="Sources officielles et communautaires synchronisées par ce nœud."
            />
            <div class="registre">
                <div class="table-wrapper">
                    <table class="data-table" aria-label="Dépôts de plugins">
                        <thead><tr>
                            <th scope="col">"Dépôt"</th><th scope="col">"Index"</th><th scope="col">"État"</th>
                            <th scope="col">"Dernière synchronisation"</th><th scope="col" class="cell-end">"Actions"</th>
                        </tr></thead>
                        <tbody>
                            {move || {
                                let text = query();
                                match repositories.get() {
                                    None => view! {
                                        <tr><td colspan="5" class="empty-state-text">"Chargement des dépôts…"</td></tr>
                                    }.into_any(),
                                    Some(Err(error)) => view! {
                                        <tr><td colspan="5" class="text-danger">{error.message()}</td></tr>
                                    }.into_any(),
                                    Some(Ok(list)) => {
                                        let shown: Vec<PluginRepository> = list
                                            .into_iter()
                                            .filter(|repo| matches(&[&repo.name, &repo.index_url], &text))
                                            .collect();
                                        if shown.is_empty() {
                                            return view! {
                                                <tr><td colspan="5" class="empty-state-text">"Aucun dépôt correspondant."</td></tr>
                                            }.into_any();
                                        }
                                        shown.into_iter().map(|repo| {
                                            let (toggle_id, sync_id, delete_id) = (repo.id.clone(), repo.id.clone(), repo.id.clone());
                                            view! {
                                                <tr>
                                                    <td class="text-strong">
                                                        {repo.name.clone()}
                                                        {repo.is_official.then(|| view! {
                                                            " "<span class="repo-badge official">"Officiel"</span>
                                                        })}
                                                    </td>
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
                                        }).collect_view().into_any()
                                    }
                                }
                            }}
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
        </section>
    }
}

#[component]
fn ToolsSection(
    tools: LocalResource<Result<Vec<ToolStatusSummary>, api::ApiError>>,
    reload_tools: RwSignal<u32>,
    can_install: bool,
) -> impl IntoView {
    let feedback = RwSignal::new(None::<String>);
    let install = Action::new_local(move |id: &String| {
        let path = format!("{TOOLS_PATH}/{}/install", encode_segment(id));
        async move { api::post::<(), Value>(&path, None).await }
    });
    Effect::new(move |_| match install.value().get() {
        Some(Ok(_)) => {
            Toasts::set_success(feedback, "Installation lancée.");
            bump(reload_tools);
        }
        Some(Err(error)) => Toasts::set_error(feedback, error.message()),
        None => {}
    });
    move || {
        let text = query();
        let list = tools.get().and_then(Result::ok).unwrap_or_default();
        if list.is_empty() {
            return None;
        }
        let shown: Vec<ToolStatusSummary> = list
            .into_iter()
            .filter(|tool| {
                matches(
                    &[
                        &tool.name,
                        &tool.id,
                        &tool.description,
                        tool.capability.as_str(),
                    ],
                    &text,
                )
            })
            .collect();
        if shown.is_empty() && !text.trim().is_empty() {
            return None;
        }
        Some(view! {
            <section class="page-section">
                <SectionHeader
                    title="Outils distribués"
                    subtitle="Binaires d'analyse et de remuxing gérés et mis en cache sur les nœuds."
                />
                <div class="resource-grid">
                    {shown.into_iter().map(|tool| {
                        let (label, class) = tool_status_label(tool.status);
                        let id = tool.id.clone();
                        let installable = can_install && tool.status == ToolStatus::NotInstalled;
                        let tone = match tool.status {
                            ToolStatus::Ready => MetricTone::Success,
                            ToolStatus::Installing => MetricTone::Warning,
                            ToolStatus::NotInstalled => MetricTone::Neutral,
                        };
                        let badge = view! {
                            <div class="cluster">
                                <Tag label=tool.version tone=TagTone::Mono />
                                <span class=class>{label}</span>
                            </div>
                        }
                        .into_any();
                        let actions = installable.then(|| {
                            let id = id.clone();
                            view! {
                                <button type="button" class="btn btn-secondary btn-sm" disabled=move || install.pending().get()
                                    on:click=move |_| { install.dispatch(id.clone()); }>"Installer"</button>
                            }
                            .into_any()
                        });
                        view! {
                            <ResourceCard
                                title=tool.name
                                subtitle=Some(tool.description)
                                icon=IconName::Package
                                tone=tone
                                badge=Some(badge)
                                footer_meta=id
                                footer_actions=actions
                            >
                                <div class="cluster">
                                    <Tag label=tool.capability.as_str() tone=TagTone::Primary />
                                    <Tag label=tool.arch tone=TagTone::Mono />
                                    <Tag label=tool.distribution tone=TagTone::Neutral />
                                    {tool.current_choice.then(|| view! {
                                        <Tag label="Choix actuel" tone=TagTone::Success />
                                    })}
                                </div>
                            </ResourceCard>
                        }
                    }).collect_view()}
                </div>
                <p class="form-help" role="status">{move || feedback.get()}</p>
            </section>
        })
    }
}

#[component]
fn CatalogSectionView(
    catalog: LocalResource<Result<Vec<CatalogEntry>, api::ApiError>>,
    can_configure: bool,
) -> impl IntoView {
    let configuring = RwSignal::new(None::<String>);
    let settings_open = RwSignal::new(false);
    let title = Signal::derive(move || configuring.get().unwrap_or_default());
    view! {
        <Drawer open=settings_open eyebrow="Réglages du plugin" title=title>
            {move || configuring.get().map(|id| view! { <PluginSettings plugin_id=id /> })}
        </Drawer>
        <section class="page-section">
            <SectionHeader
                title="Catalogue d'extensions"
                subtitle="Plugins de traitement, modèles de flux, packs et ressources synchronisés."
            />
            {move || {
                let text = query();
                let kind = plugin_kind();
                match catalog.get() {
                    None => view! {
                        <div class="empty-state-panel">
                            <p class="text-dim">"Chargement du catalogue…"</p>
                        </div>
                    }
                    .into_any(),
                    Some(Err(error)) => view! {
                        <div class="empty-state-panel">
                            <p class="form-error">{error.message()}</p>
                        </div>
                    }
                    .into_any(),
                    Some(Ok(all)) => {
                        let shown = filter_catalog(all, kind, &text);
                        if shown.is_empty() {
                            return view! {
                                <div class="empty-state-panel">
                                    <EmptyState
                                        title=if text.trim().is_empty() && kind == PluginKindFilter::Any {
                                            "Catalogue vide"
                                        } else {
                                            "Aucune extension correspondante"
                                        }
                                        message=if text.trim().is_empty() && kind == PluginKindFilter::Any {
                                            "Synchronisez un dépôt de plugins pour peupler ce catalogue."
                                        } else {
                                            "Ajustez votre recherche ou le filtre de catégorie pour afficher des extensions."
                                        }
                                        icon_name=IconName::Plugins
                                    />
                                </div>
                            }
                            .into_any();
                        }
                        view! {
                            <div class="resource-grid">
                                {shown.into_iter().map(|entry| {
                                    let id = entry.id.clone();
                                    let active_id = id.clone();
                                    let configurable = can_configure && entry.entry_type == CatalogItemType::Plugin;
                                    let badge_label = section_badge_label(entry.entry_type);
                                    let fallback_desc = section_fallback_desc(entry.entry_type);
                                    let subtitle = entry
                                        .entry_type
                                        .subtitle()
                                        .unwrap_or(fallback_desc)
                                        .to_owned();
                                    let tone = match entry.entry_type.section() {
                                        Some(CatalogSection::Plugins) => MetricTone::Success,
                                        Some(CatalogSection::Flows) => MetricTone::Warning,
                                        _ => MetricTone::Neutral,
                                    };
                                    let badge = view! {
                                        <div class="cluster">
                                            <Tag label=entry.version tone=TagTone::Mono />
                                            <Tag label=badge_label tone=TagTone::Primary />
                                        </div>
                                    }
                                    .into_any();
                                    let open_settings = configurable.then(|| {
                                        let id = id.clone();
                                        Callback::new(move |()| {
                                            configuring.set(Some(id.clone()));
                                            settings_open.set(true);
                                        })
                                    });
                                    let actions = configurable.then(|| {
                                        let id = id.clone();
                                        view! {
                                            <button type="button" class="btn btn-secondary btn-sm"
                                                on:click=move |_| { configuring.set(Some(id.clone())); settings_open.set(true); }>
                                                "Réglages"
                                            </button>
                                        }
                                        .into_any()
                                    });
                                    let has_meta = entry.capability.is_some()
                                        || entry.category.is_some_and(|cat| cat.shows_badge())
                                        || entry.distribution.is_some()
                                        || !entry.tags.is_empty();
                                    view! {
                                        <ResourceCard
                                            title=entry.name
                                            subtitle=Some(subtitle)
                                            icon=section_icon(entry.entry_type)
                                            tone=tone
                                            badge=Some(badge)
                                            active=Some(Signal::derive(move || {
                                                settings_open.get()
                                                    && configuring.get().as_deref() == Some(active_id.as_str())
                                            }))
                                            on_click=open_settings
                                            footer_meta=id
                                            footer_actions=actions
                                        >
                                            {if has_meta {
                                                view! {
                                                    <div class="cluster">
                                                        {entry.capability.map(|cap| view! {
                                                            <Tag label=cap.as_str() tone=TagTone::Success />
                                                        })}
                                                        {entry.category.filter(|cat| cat.shows_badge()).map(|cat| view! {
                                                            <Tag label=cat.as_str() tone=TagTone::Neutral />
                                                        })}
                                                        {entry.distribution.map(|dist| view! {
                                                            <Tag label=dist tone=TagTone::Mono />
                                                        })}
                                                        {entry.tags.into_iter().map(|tag| view! {
                                                            <Tag label=tag tone=TagTone::Neutral />
                                                        }).collect_view()}
                                                    </div>
                                                }
                                                .into_any()
                                            } else {
                                                view! { <p class="text-dim text-sm">{fallback_desc}</p> }.into_any()
                                            }}
                                        </ResourceCard>
                                    }
                                }).collect_view()}
                            </div>
                        }
                        .into_any()
                    }
                }
            }}
        </section>
    }
}

#[component]
pub fn PluginsPage(me: Me) -> impl IntoView {
    let central = me.instance_kind == InstanceKind::Central;
    let can_manage = central && me.allows(Permission::ManagePlugins);
    let filters = use_context::<Filters>();
    let reload_catalog = RwSignal::new(0_u32);
    let reload_tools = RwSignal::new(0_u32);
    let reload_repos = RwSignal::new(0_u32);
    let adding = RwSignal::new(false);

    if can_manage {
        provide_topbar_action(ActionSpec {
            label: "Ajouter un dépôt",
            on_click: Callback::new(move |()| adding.set(true)),
        });
    }

    let catalog = LocalResource::new(move || {
        reload_catalog.track();
        api::get::<Vec<CatalogEntry>>(CATALOG_PATH)
    });
    let tools = LocalResource::new(move || {
        reload_tools.track();
        api::get::<Vec<ToolStatusSummary>>(TOOLS_PATH)
    });
    let repositories = can_manage.then(|| {
        LocalResource::new(move || {
            reload_repos.track();
            api::get::<Vec<PluginRepository>>(REPOSITORIES_PATH)
        })
    });

    let stats = Signal::derive(move || {
        let catalog_entries = catalog.get().and_then(Result::ok).unwrap_or_default();
        let (total_catalog, plugins, _) = summarize_catalog(&catalog_entries);
        let flows = catalog_entries
            .iter()
            .filter(|entry| entry.entry_type.section() == Some(CatalogSection::Flows))
            .count();
        let packs = catalog_entries
            .iter()
            .filter(|entry| entry.entry_type.section() == Some(CatalogSection::Packs))
            .count();
        let active_kind = plugin_kind();
        let set_kind = move |next: PluginKindFilter| {
            Callback::new(move |()| {
                if let Some(filters) = filters {
                    filters.plugin_kind.set(next);
                }
            })
        };
        let mut items = vec![
            StatItem::new("Catalogue", total_catalog.to_string(), MetricTone::Neutral).with_filter(
                active_kind == PluginKindFilter::Any,
                set_kind(PluginKindFilter::Any),
            ),
            StatItem::new(
                "Plugins",
                plugins.to_string(),
                if plugins > 0 {
                    MetricTone::Success
                } else {
                    MetricTone::Neutral
                },
            )
            .with_filter(
                active_kind == PluginKindFilter::Plugins,
                set_kind(PluginKindFilter::Plugins),
            ),
            StatItem::new("Flux", flows.to_string(), MetricTone::Neutral).with_filter(
                active_kind == PluginKindFilter::Flows,
                set_kind(PluginKindFilter::Flows),
            ),
            StatItem::new("Packs", packs.to_string(), MetricTone::Neutral).with_filter(
                active_kind == PluginKindFilter::Packs,
                set_kind(PluginKindFilter::Packs),
            ),
        ];
        if let Some(repos_res) = repositories {
            let repos = repos_res.get().and_then(Result::ok).unwrap_or_default();
            let active = repos.iter().filter(|repo| repo.enabled).count();
            items.push(StatItem::new(
                "Dépôts actifs",
                format!("{active} / {}", repos.len()),
                if active > 0 {
                    MetricTone::Success
                } else {
                    MetricTone::Warning
                },
            ));
        } else {
            let tools_list = tools.get().and_then(Result::ok).unwrap_or_default();
            let ready = tools_list
                .iter()
                .filter(|tool| tool.status == ToolStatus::Ready)
                .count();
            items.push(StatItem::new(
                "Outils prêts",
                format!("{ready} / {}", tools_list.len()),
                if ready > 0 {
                    MetricTone::Success
                } else {
                    MetricTone::Neutral
                },
            ));
        }
        items
    });

    view! {
        <div class="tab-panel" role="tabpanel">
            <StatStrip aria_label="Synthèse des plugins et dépôts" items=stats />
            <CatalogSectionView catalog=catalog can_configure=can_manage />
            <ToolsSection tools=tools reload_tools=reload_tools can_install=can_manage />
            {repositories.map(|repos| view! {
                <ReposSection
                    repositories=repos
                    reload_repos=reload_repos
                    reload_catalog=reload_catalog
                    adding=adding
                />
            })}
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use flux_shared::plugins::vocab::CatalogCapability;

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
            sha256: None,
        }
    }

    #[test]
    fn summarize_and_filter_catalog_exclude_apps_and_blocks() {
        let mut plugin = entry("plugin.sonarr", CatalogItemType::Plugin);
        plugin.capability = Some(CatalogCapability::Integration);
        plugin.tags = vec!["arr".into(), "tv".into()];
        let flow = entry("flow.remux", CatalogItemType::Flow);
        let pack = entry("pack.base", CatalogItemType::Pack);
        let preset = entry("preset.fast", CatalogItemType::Preset);
        let app = entry("app.ffmpeg", CatalogItemType::App);
        let block = entry("block.remux", CatalogItemType::Block);

        let all = vec![plugin, flow, pack, preset, app, block];
        assert_eq!(summarize_catalog(&all), (4, 1, 2));
        assert_eq!(
            filter_catalog(all.clone(), PluginKindFilter::Any, "").len(),
            4
        );
        assert_eq!(
            filter_catalog(all.clone(), PluginKindFilter::Plugins, "arr").len(),
            1
        );
        assert_eq!(filter_catalog(all, PluginKindFilter::Flows, "").len(), 1);
    }

    #[test]
    fn badge_labels_cover_all_catalog_item_types() {
        assert_eq!(section_badge_label(CatalogItemType::Plugin), "Plugin");
        assert_eq!(section_badge_label(CatalogItemType::Flow), "Flow");
        assert_eq!(section_badge_label(CatalogItemType::Pack), "Pack");
        assert_eq!(
            section_badge_label(CatalogItemType::ResourceImage),
            "Images"
        );
    }
}
