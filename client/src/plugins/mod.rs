//! Plugins : catalogue App Store, outils distribués, dépôts synchronisés et tiroirs croisés (ADR 0010, 0014).

mod catalog;
mod exposed_variables;
mod flow_preview;
mod install;
mod item_detail;
mod metadata;
mod pack_detail;
mod plugin_detail;
mod plugin_icon;
mod repo_detail;
mod repos;
mod script_preview;
mod settings;
mod settings_form;
mod store;
mod subcategory_groups;
mod tool_detail;
mod tools;

use crate::api;
use crate::live::{load, use_live};
use crate::route::encode_segment;
use crate::ui::{Drawer, IconName, Toasts};
use catalog::{CatalogSectionView, summarize_catalog};
pub(crate) use flow_preview::{
    FlowMiniPreview, preview_from_definition_json, preview_from_job_steps,
};
use forgearr_shared::live::ChangeTopic;
use forgearr_shared::plugins::vocab::{CatalogItemType, CatalogSection};
use forgearr_shared::plugins::{
    CatalogEntry, DependencyInstallResult, PluginRepository, ToolStatusSummary, store::StoreEntry,
};
use forgearr_shared::session::Me;
use forgearr_shared::{InstanceKind, Permission};
use install::InstallWizard;
use item_detail::CatalogItemDetail;
use leptos::prelude::*;
pub(crate) use metadata::{StatusBanner, StoreTone};
pub(crate) use pack_detail::{PackDetail, PackDrawerTab};
use plugin_detail::{PluginDetail, PluginDrawerTab};
use repos::ReposSection;
pub(crate) use script_preview::highlighted_lines;
use store::{InstallTarget, StoreSection};
use tool_detail::{ToolDetail, ToolDrawerTab};
use tools::ToolsSection;

const REPOSITORIES_PATH: &str = "/api/v1/plugins/repositories";
const CATALOG_SCRIPTS_PATH: &str = "/api/v1/catalog-scripts";
const CATALOG_PATH: &str = "/api/v1/plugins";
const TOOLS_PATH: &str = "/api/v1/tools";
const STORE_PATH: &str = "/api/v1/store";

/// Sujets lus par le catalogue et la fiche d'un plugin ou d'un pack.
pub(super) const CATALOG_TOPICS: &[ChangeTopic] = &[ChangeTopic::Catalog, ChangeTopic::Tools];
/// Sujets lus par les applications : leur liste vient aussi du catalogue.
pub(super) const TOOL_TOPICS: &[ChangeTopic] = CATALOG_TOPICS;
/// Sujets lus par le Store : une installation, une synchronisation ou un dépôt retiré le changent.
pub(super) const STORE_TOPICS: &[ChangeTopic] = &[
    ChangeTopic::Catalog,
    ChangeTopic::Tools,
    ChangeTopic::Repositories,
];
/// Sujets lus par les dépôts.
pub(super) const REPOSITORY_TOPICS: &[ChangeTopic] =
    &[ChangeTopic::Repositories, ChangeTopic::Catalog];

pub(super) fn section_icon(entry_type: CatalogItemType) -> IconName {
    match entry_type.section() {
        Some(CatalogSection::Plugins) => IconName::Plugins,
        Some(CatalogSection::Flows) => IconName::Flows,
        Some(CatalogSection::Scripts) => IconName::Command,
        Some(CatalogSection::Packs) => IconName::Download,
        Some(CatalogSection::Applications) => IconName::Package,
        Some(CatalogSection::Other) | None => IconName::Layers,
    }
}

/// Synthèse du bilan d'installation d'un outil ou d'un plugin et de ses dépendances.
pub(super) fn dependency_install_summary(result: &DependencyInstallResult) -> (bool, String) {
    let ok = result.failed_tools.is_empty() && result.missing_dependencies.is_empty();
    let mut msg = if ok {
        format!(
            "{} outil(s) installé(s) ou déjà prêt(s).",
            result.installed_tools.len()
        )
    } else {
        format!(
            "{} outil(s) prêt(s), {} en échec, {} dépendance(s) introuvable(s).",
            result.installed_tools.len(),
            result.failed_tools.len(),
            result.missing_dependencies.len()
        )
    };
    if !result.suggested_repositories.is_empty() {
        let repositories: Vec<&str> = result
            .suggested_repositories
            .iter()
            .map(|repository| repository.id.as_str())
            .collect();
        msg.push_str(&format!(
            " Dépôt(s) à ajouter vous-même pour les fournir : {}.",
            repositories.join(", ")
        ));
    }
    (ok, msg)
}

/// Action d'installation d'une pièce (`{base}/{id}/install`) et de ses outils requis ;
/// le bilan est affiché dans `feedback`, puis catalogue et applications sont relus.
pub(super) fn install_with_dependencies_action(
    base: &'static str,
    feedback: RwSignal<Option<String>>,
) -> Action<String, Result<DependencyInstallResult, api::ApiError>> {
    let live = use_live();
    let action = Action::new_local(move |id: &String| {
        let path = format!("{base}/{}/install", encode_segment(id));
        async move { api::post::<(), DependencyInstallResult>(&path, None).await }
    });
    Effect::new(move |_| match action.value().get() {
        Some(Ok(result)) => {
            let (ok, msg) = dependency_install_summary(&result);
            if ok {
                Toasts::set_success(feedback, msg);
            } else {
                Toasts::set_error(feedback, msg);
            }
            live.touch(TOOL_TOPICS);
        }
        Some(Err(error)) => Toasts::set_error(feedback, error.message()),
        None => {}
    });
    action
}

pub(super) fn section_badge_label(entry_type: CatalogItemType) -> &'static str {
    if let Some(sub) = entry_type.subtitle() {
        return sub;
    }
    match entry_type.section() {
        Some(CatalogSection::Plugins) => "Plugin",
        Some(CatalogSection::Flows) => "Flow",
        Some(CatalogSection::Scripts) => "Script",
        Some(CatalogSection::Packs) => "Pack",
        Some(CatalogSection::Applications) => "Application",
        Some(CatalogSection::Other) | None => "Autre",
    }
}

/// Ce que fait une pièce dont le manifeste ne donne pas de description.
fn section_fallback_desc(entry_type: CatalogItemType) -> &'static str {
    match entry_type.section() {
        Some(CatalogSection::Plugins) => {
            "Ajoute des blocs à vos flux : encoder, extraire, notifier ou prévenir Sonarr, Radarr et Plex."
        }
        Some(CatalogSection::Flows) => {
            "Flux complet prêt à l'emploi : il traite un fichier de bout en bout, de l'entrée à la sortie."
        }
        Some(CatalogSection::Scripts) => {
            "Script appelé par un bloc de flux pour une étape sur mesure."
        }
        Some(CatalogSection::Packs) => {
            "Installe d'un coup des plugins, flux et ressources prévus pour fonctionner ensemble."
        }
        Some(CatalogSection::Applications) => {
            "Programme installé sur les nœuds (FFmpeg, MKVToolNix…) et utilisé par les plugins."
        }
        Some(CatalogSection::Other) | None => {
            "Ressource utilisée par les flux : police, image, sous-titre, préréglage…"
        }
    }
}

/// Texte d'une carte : description du manifeste, sinon libellé du type, sinon description par section.
pub(super) fn card_description(entry: &CatalogEntry) -> String {
    entry
        .description
        .clone()
        .or_else(|| entry.entry_type.subtitle().map(str::to_owned))
        .unwrap_or_else(|| section_fallback_desc(entry.entry_type).to_owned())
}

/// Sous-onglets de l'écran Plugins, séparés par intention.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PluginsTab {
    #[default]
    Store,
    Installed,
    Tools,
    Repositories,
}

impl PluginsTab {
    pub fn label(self) -> &'static str {
        match self {
            Self::Store => "Store",
            Self::Installed => "Installés",
            Self::Tools => "Outils",
            Self::Repositories => "Dépôts",
        }
    }

    pub fn visible_tabs(can_manage: bool) -> &'static [Self] {
        if can_manage {
            &[
                Self::Store,
                Self::Installed,
                Self::Tools,
                Self::Repositories,
            ]
        } else {
            &[Self::Store, Self::Installed, Self::Tools]
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SelectedItem {
    pub id: String,
    pub name: String,
}

#[derive(Clone, PartialEq)]
enum InspectionTarget {
    Plugin(SelectedItem),
    Pack(SelectedItem),
    Tool(SelectedItem),
    Item(Box<CatalogEntry>),
}

impl InspectionTarget {
    fn id(&self) -> &str {
        match self {
            Self::Plugin(s) | Self::Pack(s) | Self::Tool(s) => &s.id,
            Self::Item(entry) => &entry.id,
        }
    }

    fn title(&self) -> String {
        match self {
            Self::Plugin(s) | Self::Pack(s) | Self::Tool(s) => s.name.clone(),
            Self::Item(entry) => entry.name.clone(),
        }
    }
}

/// Fiche à jour d'une pièce ouverte ; retirée du catalogue, elle reste affichée telle quelle.
fn current_entry(entries: &[CatalogEntry], opened: &CatalogEntry) -> CatalogEntry {
    entries
        .iter()
        .find(|entry| entry.id == opened.id)
        .unwrap_or(opened)
        .clone()
}

#[component]
pub fn PluginsPage(me: Me) -> impl IntoView {
    let central = me.instance_kind == InstanceKind::Central;
    let can_manage = central && me.allows(Permission::ManagePlugins);
    let active_tab = RwSignal::new(PluginsTab::Store);
    let installing = RwSignal::new(None::<InstallTarget>);
    let wizard_open = RwSignal::new(false);
    let open_wizard = Callback::new(move |target: InstallTarget| {
        installing.set(Some(target));
        wizard_open.set(true);
    });
    let wizard_title = Signal::derive(move || {
        installing
            .get()
            .map(|target| target.name)
            .unwrap_or_default()
    });
    let adding_repo = RwSignal::new(false);

    // Tiroir d'inspection unifié permettant la navigation croisée Plugin <-> Outil.
    let target = RwSignal::new(None::<InspectionTarget>);
    let plugin_open = RwSignal::new(false);
    let plugin_tab = RwSignal::new(0_usize);
    let pack_open = RwSignal::new(false);
    let pack_tab = RwSignal::new(0_usize);
    let tool_open = RwSignal::new(false);
    let tool_tab = RwSignal::new(0_usize);
    let item_open = RwSignal::new(false);

    let close_all_drawers = move || {
        plugin_open.set(false);
        pack_open.set(false);
        tool_open.set(false);
        item_open.set(false);
    };

    let open_plugin = Callback::new(move |item: SelectedItem| {
        close_all_drawers();
        target.set(Some(InspectionTarget::Plugin(item)));
        plugin_tab.set(0);
        plugin_open.set(true);
    });
    let open_pack = Callback::new(move |item: SelectedItem| {
        close_all_drawers();
        target.set(Some(InspectionTarget::Pack(item)));
        pack_tab.set(0);
        pack_open.set(true);
    });
    let open_tool = Callback::new(move |item: SelectedItem| {
        close_all_drawers();
        target.set(Some(InspectionTarget::Tool(item)));
        tool_tab.set(0);
        tool_open.set(true);
    });
    let open_item = Callback::new(move |entry: CatalogEntry| {
        close_all_drawers();
        target.set(Some(InspectionTarget::Item(Box::new(entry))));
        item_open.set(true);
    });

    let active_item_id = Signal::derive(move || {
        let any_open = plugin_open.get() || pack_open.get() || item_open.get();
        any_open
            .then(|| target.get().map(|t| t.id().to_owned()))
            .flatten()
    });
    let active_tool_id = Signal::derive(move || {
        tool_open
            .get()
            .then(|| target.get().map(|t| t.id().to_owned()))
            .flatten()
    });

    let drawer_title = Signal::derive(move || target.get().map(|t| t.title()).unwrap_or_default());

    let catalog = load::<Vec<CatalogEntry>>(CATALOG_TOPICS, CATALOG_PATH);
    let store = load::<Vec<StoreEntry>>(STORE_TOPICS, STORE_PATH);
    let tools = load::<Vec<ToolStatusSummary>>(TOOL_TOPICS, TOOLS_PATH);
    let repositories =
        can_manage.then(|| load::<Vec<PluginRepository>>(REPOSITORY_TOPICS, REPOSITORIES_PATH));
    let catalog_entries =
        Signal::derive(move || catalog.get().and_then(Result::ok).unwrap_or_default());

    let tab_count = move |tab: PluginsTab| -> Option<usize> {
        match tab {
            PluginsTab::Store => store.get().and_then(Result::ok).map(|list| list.len()),
            PluginsTab::Installed => catalog
                .get()
                .and_then(Result::ok)
                .map(|entries| summarize_catalog(&entries).total),
            PluginsTab::Tools => tools.get().and_then(Result::ok).map(|list| list.len()),
            PluginsTab::Repositories => repositories
                .and_then(|res| res.get())
                .and_then(Result::ok)
                .map(|list| list.len()),
        }
    };

    view! {
        <div class="tab-panel" role="tabpanel">
            <nav class="page-subnav" role="tablist" aria-label="Vues des extensions">
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
                            <span>{tab.label()}</span>
                            {move || tab_count(tab).map(|count| view! {
                                <span class="tab-btn-badge">{count}</span>
                            })}
                        </button>
                    }
                }).collect_view()}
            </nav>

            <Drawer open=wizard_open eyebrow="Installation" title=wizard_title>
                {move || installing.get().map(|target| view! {
                    <InstallWizard target=target open=wizard_open />
                })}
            </Drawer>
            <Drawer
                open=plugin_open
                eyebrow="Plugin"
                title=drawer_title
                tabs=PluginDrawerTab::tabs(can_manage).to_vec()
                tab=plugin_tab
            >
                {move || match target.get() {
                    Some(InspectionTarget::Plugin(item)) => view! {
                        <PluginDetail
                            plugin_id=item.id
                            tab=plugin_tab
                            can_configure=can_manage
                            on_open_tool=open_tool
                        />
                    }.into_any(),
                    _ => ().into_any(),
                }}
            </Drawer>
            <Drawer
                open=pack_open
                eyebrow="Pack"
                title=drawer_title
                tabs=PackDrawerTab::TABS.to_vec()
                tab=pack_tab
            >
                {move || match target.get() {
                    Some(InspectionTarget::Pack(item)) => view! {
                        <PackDetail
                            pack_id=item.id
                            tab=pack_tab
                            can_install=can_manage
                            on_open_plugin=open_plugin
                            on_open_tool=open_tool
                            catalog_entries=catalog_entries
                        />
                    }.into_any(),
                    _ => ().into_any(),
                }}
            </Drawer>
            <Drawer
                open=tool_open
                eyebrow="Outil"
                title=drawer_title
                tabs=ToolDrawerTab::TABS.to_vec()
                tab=tool_tab
            >
                {move || match target.get() {
                    Some(InspectionTarget::Tool(item)) => view! {
                        <ToolDetail
                            tool_id=item.id
                            tab=tool_tab
                            can_install=can_manage
                            on_open_plugin=open_plugin
                        />
                    }.into_any(),
                    _ => ().into_any(),
                }}
            </Drawer>
            <Drawer open=item_open eyebrow="Catalogue" title=drawer_title>
                {move || match target.get() {
                    Some(InspectionTarget::Item(opened)) => {
                        let entry = Memo::new(move |_| current_entry(&catalog_entries.read(), &opened));
                        view! { <CatalogItemDetail entry=entry can_manage=can_manage /> }.into_any()
                    }
                    _ => ().into_any(),
                }}
            </Drawer>

            {move || match active_tab.get() {
                PluginsTab::Store => view! {
                    <StoreSection store=store can_install=can_manage on_install=open_wizard />
                }.into_any(),
                PluginsTab::Installed => view! {
                    <CatalogSectionView
                        catalog=catalog
                        can_configure=can_manage
                        active_item_id=active_item_id
                        on_open_plugin=open_plugin
                        on_open_pack=open_pack
                        on_open_item=open_item
                    />
                }.into_any(),
                PluginsTab::Tools => view! {
                    <ToolsSection
                        tools=tools
                        can_install=can_manage
                        active_tool_id=active_tool_id
                        on_open_tool=open_tool
                    />
                }.into_any(),
                PluginsTab::Repositories => match repositories {
                    Some(repos) => view! {
                        <ReposSection
                            repositories=repos
                            adding=adding_repo
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

    #[test]
    fn visible_tabs_hide_repositories_without_manage_permission() {
        assert_eq!(
            PluginsTab::visible_tabs(false),
            &[PluginsTab::Store, PluginsTab::Installed, PluginsTab::Tools]
        );
        assert_eq!(
            PluginsTab::visible_tabs(true),
            &[
                PluginsTab::Store,
                PluginsTab::Installed,
                PluginsTab::Tools,
                PluginsTab::Repositories
            ]
        );
        assert_eq!(PluginsTab::Installed.label(), "Installés");
    }

    fn entry(id: &str, entry_type: CatalogItemType) -> CatalogEntry {
        CatalogEntry {
            id: id.to_owned(),
            entry_type,
            version: "1.0.0".to_owned(),
            name: id.to_owned(),
            manifest_path: format!("{id}.json"),
            description: None,
            capability: None,
            current_choice: None,
            distribution: None,
            category: None,
            subcategory: None,
            has_icon: false,
            tags: Vec::new(),
            owner: None,
            sha256: None,
            updated_at: None,
            runtime: None,
        }
    }

    #[test]
    fn an_open_item_follows_the_catalog_and_survives_its_removal() {
        let opened = entry("script.x", CatalogItemType::Script);
        let updated = CatalogEntry {
            version: "2.0.0".to_owned(),
            ..opened.clone()
        };
        let other = entry("script.y", CatalogItemType::Script);
        assert_eq!(
            current_entry(&[other.clone(), updated.clone()], &opened),
            updated
        );
        assert_eq!(current_entry(&[other], &opened), opened);
    }

    #[test]
    fn the_card_prefers_the_manifest_description() {
        let mut plugin = entry("archives", CatalogItemType::Plugin);
        assert_eq!(
            card_description(&plugin),
            section_fallback_desc(CatalogItemType::Plugin)
        );
        plugin.description = Some("Extraction des archives téléchargées.".to_owned());
        assert_eq!(
            card_description(&plugin),
            "Extraction des archives téléchargées."
        );
        assert_eq!(
            card_description(&entry("polices", CatalogItemType::ResourceFont)),
            "Polices"
        );
    }

    #[test]
    fn dependency_install_summary_reports_failures_and_missing() {
        let mut result = DependencyInstallResult {
            item_id: "video".into(),
            installed_tools: vec!["ffmpeg".into(), "mkvtoolnix".into()],
            failed_tools: Vec::new(),
            missing_dependencies: Vec::new(),
            suggested_repositories: Vec::new(),
        };
        assert_eq!(
            dependency_install_summary(&result),
            (true, "2 outil(s) installé(s) ou déjà prêt(s).".to_owned())
        );
        result
            .failed_tools
            .push(("mp4box".into(), "contrat absent".into()));
        result.missing_dependencies.push("absent".into());
        assert_eq!(
            dependency_install_summary(&result),
            (
                false,
                "2 outil(s) prêt(s), 1 en échec, 1 dépendance(s) introuvable(s).".to_owned()
            )
        );
        result
            .suggested_repositories
            .push(forgearr_shared::exposed_variables::RepositoryPointer {
                id: "repo.tiers".into(),
                index_url: "https://exemple.org/index.json".into(),
            });
        assert!(
            dependency_install_summary(&result)
                .1
                .ends_with("Dépôt(s) à ajouter vous-même pour les fournir : repo.tiers.")
        );
    }

    #[test]
    fn badge_labels_cover_all_catalog_item_types() {
        assert_eq!(section_badge_label(CatalogItemType::Plugin), "Plugin");
        assert_eq!(section_badge_label(CatalogItemType::Flow), "Flow");
        assert_eq!(section_badge_label(CatalogItemType::Script), "Script");
        assert_eq!(section_badge_label(CatalogItemType::Pack), "Pack");
        assert_eq!(
            section_badge_label(CatalogItemType::ResourceImage),
            "Images"
        );
        assert_eq!(
            section_badge_label(CatalogItemType::ResourceFont),
            "Polices"
        );
    }
}
