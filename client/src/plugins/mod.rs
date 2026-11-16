//! Plugins : catalogue d'extensions, outils distribués, dépôts synchronisés et réglages (ADR 0010, 0014).

mod pack_detail;
mod repos;
mod settings;
mod tools;

use crate::api;
use crate::ui::filters::{Filters, PluginKindFilter, plugin_kind};
use crate::ui::search::{matches, query};
use crate::ui::topbar_action::{ActionSpec, provide_topbar_action};
use crate::ui::{
    Drawer, EmptyState, IconName, MetricTone, ResourceCard, StatItem, StatStrip, Tag, TagTone,
};
use flux_shared::plugins::vocab::{CatalogItemType, CatalogSection};
use flux_shared::plugins::{CatalogEntry, PluginRepository, ToolStatus, ToolStatusSummary};
use flux_shared::session::Me;
use flux_shared::{InstanceKind, Permission};
use leptos::prelude::*;
use pack_detail::PackDetail;
use repos::ReposSection;
use settings::PluginSettings;
use tools::ToolsSection;

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
    if let Some(cap) = entry.capability.as_ref() {
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

pub(super) fn section_badge_label(entry_type: CatalogItemType) -> &'static str {
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

/// Sous-onglets de l'écran Plugins, séparés par intention.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PluginsTab {
    #[default]
    Catalog,
    Tools,
    Repositories,
}

impl PluginsTab {
    pub fn label(self) -> &'static str {
        match self {
            Self::Catalog => "Catalogue",
            Self::Tools => "Outils",
            Self::Repositories => "Dépôts",
        }
    }

    pub fn visible_tabs(can_manage: bool) -> &'static [Self] {
        if can_manage {
            &[Self::Catalog, Self::Tools, Self::Repositories]
        } else {
            &[Self::Catalog, Self::Tools]
        }
    }
}
#[derive(Clone, PartialEq, Eq)]
enum CatalogDrawer {
    PluginSettings(String),
    PackDetails(String),
}

impl CatalogDrawer {
    fn id(&self) -> &str {
        match self {
            Self::PluginSettings(id) | Self::PackDetails(id) => id,
        }
    }
}

#[component]
fn CatalogSectionView(
    catalog: LocalResource<Result<Vec<CatalogEntry>, api::ApiError>>,
    reload_tools: RwSignal<u32>,
    can_configure: bool,
) -> impl IntoView {
    let filters = use_context::<Filters>();
    let inspecting = RwSignal::new(None::<CatalogDrawer>);
    let drawer_open = RwSignal::new(false);
    let title = Signal::derive(move || {
        inspecting
            .get()
            .map(|target| target.id().to_owned())
            .unwrap_or_default()
    });
    let on_pack_installed = Callback::new(move |()| bump(reload_tools));
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
        vec![
            StatItem::new("Tous", total_catalog.to_string(), MetricTone::Neutral).with_filter(
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
            StatItem::new("Packs", packs.to_string(), MetricTone::Neutral).with_filter(
                active_kind == PluginKindFilter::Packs,
                set_kind(PluginKindFilter::Packs),
            ),
            StatItem::new("Flux", flows.to_string(), MetricTone::Neutral).with_filter(
                active_kind == PluginKindFilter::Flows,
                set_kind(PluginKindFilter::Flows),
            ),
        ]
    });

    view! {
        <StatStrip aria_label="Synthèse du catalogue" items=stats />
        <Drawer open=drawer_open eyebrow="Catalogue" title=title>
            {move || match inspecting.get() {
                Some(CatalogDrawer::PluginSettings(id)) => {
                    view! { <PluginSettings plugin_id=id /> }.into_any()
                }
                Some(CatalogDrawer::PackDetails(id)) => {
                    view! {
                        <PackDetail
                            pack_id=id
                            can_install=can_configure
                            on_installed=on_pack_installed
                        />
                    }
                    .into_any()
                }
                None => ().into_any(),
            }}
        </Drawer>
        <section class="page-section">
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
                                    let target = match entry.entry_type {
                                        CatalogItemType::Plugin if can_configure => {
                                            Some((CatalogDrawer::PluginSettings(id.clone()), "Configurer"))
                                        }
                                        CatalogItemType::Pack => Some((
                                            CatalogDrawer::PackDetails(id.clone()),
                                            if can_configure { "Voir / Installer" } else { "Détails" },
                                        )),
                                        _ => None,
                                    };
                                    let is_template = matches!(
                                        entry.entry_type,
                                        CatalogItemType::Flow | CatalogItemType::Preset
                                    );
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
                                    let open_drawer = target.as_ref().map(|(dest, _)| {
                                        let dest = dest.clone();
                                        Callback::new(move |()| {
                                            inspecting.set(Some(dest.clone()));
                                            drawer_open.set(true);
                                        })
                                    });
                                    let actions = target.as_ref().map(|(dest, label)| {
                                        let dest = dest.clone();
                                        let label = *label;
                                        view! {
                                            <button type="button" class="btn btn-secondary btn-sm"
                                                on:click=move |_| {
                                                    inspecting.set(Some(dest.clone()));
                                                    drawer_open.set(true);
                                                }>
                                                {label}
                                            </button>
                                        }
                                        .into_any()
                                    });
                                    let has_meta = is_template
                                        || entry.capability.is_some()
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
                                                drawer_open.get()
                                                    && inspecting
                                                        .get()
                                                        .is_some_and(|item| item.id() == active_id)
                                            }))
                                            on_click=open_drawer
                                            footer_meta=id
                                            footer_actions=actions
                                        >
                                            {if has_meta {
                                                view! {
                                                    <div class="cluster">
                                                        {is_template.then(|| view! {
                                                            <Tag label="Modèle prêt à l'emploi" tone=TagTone::Success />
                                                        })}
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
    let active_tab = RwSignal::new(PluginsTab::Catalog);
    let reload_catalog = RwSignal::new(0_u32);
    let reload_tools = RwSignal::new(0_u32);
    let reload_repos = RwSignal::new(0_u32);
    let adding = RwSignal::new(false);

    if can_manage {
        provide_topbar_action(ActionSpec {
            label: "Ajouter un dépôt",
            on_click: Callback::new(move |()| {
                active_tab.set(PluginsTab::Repositories);
                adding.set(true);
            }),
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

    view! {
        <div class="tab-panel" role="tabpanel">
            <nav class="page-subnav" role="tablist" aria-label="Vues des plugins">
                {PluginsTab::visible_tabs(can_manage).iter().copied().map(|tab| {
                    let is_active = move || active_tab.get() == tab;
                    view! {
                        <button
                            type="button"
                            role="tab"
                            class="page-subnav__tab"
                            class:is-active=is_active
                            aria-selected=move || is_active().to_string()
                            on:click=move |_| active_tab.set(tab)
                        >
                            {tab.label()}
                        </button>
                    }
                }).collect_view()}
            </nav>
            {move || match active_tab.get() {
                PluginsTab::Catalog => view! {
                    <CatalogSectionView
                        catalog=catalog
                        reload_tools=reload_tools
                        can_configure=can_manage
                    />
                }.into_any(),
                PluginsTab::Tools => view! {
                    <ToolsSection
                        tools=tools
                        reload_tools=reload_tools
                        can_install=can_manage
                    />
                }.into_any(),
                PluginsTab::Repositories => match repositories {
                    Some(repos) => view! {
                        <ReposSection
                            repositories=repos
                            reload_repos=reload_repos
                            reload_catalog=reload_catalog
                            adding=adding
                        />
                    }.into_any(),
                    None => ().into_any(),
                },
            }}
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use flux_shared::plugins::vocab::CatalogCapability;

    #[test]
    fn visible_tabs_hide_repositories_without_manage_permission() {
        assert_eq!(
            PluginsTab::visible_tabs(false),
            &[PluginsTab::Catalog, PluginsTab::Tools]
        );
        assert_eq!(
            PluginsTab::visible_tabs(true),
            &[
                PluginsTab::Catalog,
                PluginsTab::Tools,
                PluginsTab::Repositories
            ]
        );
    }

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
        plugin.capability = Some(CatalogCapability::new("integration"));
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
