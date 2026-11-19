//! Plugins : catalogue d'extensions, outils distribués, dépôts synchronisés et réglages (ADR 0010, 0014).

mod pack_detail;
mod plugin_detail;
mod repo_detail;
mod repos;
mod settings;
mod tool_detail;
mod tools;

use crate::api;
use crate::route::encode_segment;
use crate::ui::filters::{Filters, PluginKindFilter, plugin_kind};
use crate::ui::search::{matches, query};
use crate::ui::topbar_action::{ActionSpec, provide_topbar_action};
use crate::ui::{
    Drawer, EmptyState, IconName, MetricTone, ResourceCard, StatItem, StatStrip, Tag, TagTone,
    Toasts,
};
use chrono::NaiveDate;
use forgearr_shared::format::format_day;
use forgearr_shared::plugins::vocab::{CatalogItemType, CatalogSection};
use forgearr_shared::plugins::{
    CatalogEntry, DependencyInstallResult, PluginRepository, ToolStatus, ToolStatusSummary,
};
use forgearr_shared::session::Me;
use forgearr_shared::{InstanceKind, Permission};
use leptos::prelude::*;
pub(crate) use pack_detail::{PackDetail, PackDrawerTab};
use plugin_detail::{PluginDetail, PluginDrawerTab};
use repos::ReposSection;
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

/// Compteurs affichés dans le bandeau `StatStrip` du catalogue.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CatalogCounts {
    pub total: usize,
    pub plugins: usize,
    pub packs: usize,
    pub flows: usize,
    pub scripts: usize,
    pub resources: usize,
}

/// Synthèse des entrées du catalogue par section (hors blocs et outils distribués).
pub fn summarize_catalog(entries: &[CatalogEntry]) -> CatalogCounts {
    let mut counts = CatalogCounts::default();
    for entry in entries {
        match entry.entry_type.section() {
            Some(CatalogSection::Plugins) => {
                counts.total += 1;
                counts.plugins += 1;
            }
            Some(CatalogSection::Flows) => {
                counts.total += 1;
                counts.flows += 1;
            }
            Some(CatalogSection::Scripts) => {
                counts.total += 1;
                counts.scripts += 1;
            }
            Some(CatalogSection::Packs) => {
                counts.total += 1;
                counts.packs += 1;
            }
            Some(CatalogSection::Other) => {
                counts.total += 1;
                counts.resources += 1;
            }
            Some(CatalogSection::Applications) | None => {}
        }
    }
    counts
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
        Some(CatalogSection::Scripts) => IconName::Command,
        Some(CatalogSection::Packs) => IconName::Download,
        Some(CatalogSection::Applications) => IconName::Package,
        Some(CatalogSection::Other) | None => IconName::Layers,
    }
}

/// Ligne « terme / date » d'une fiche ; rien si la date est inconnue.
pub(super) fn date_row(term: &'static str, date: Option<NaiveDate>) -> impl IntoView {
    date.map(|day| {
        view! {
            <div class="detail-term">{term}</div>
            <div class="detail-val">{format_day(day)}</div>
        }
    })
}

/// Synthèse du bilan d'installation d'un outil ou d'un plugin et de ses dépendances.
pub(super) fn dependency_install_summary(result: &DependencyInstallResult) -> (bool, String) {
    let ok = result.failed_tools.is_empty() && result.missing_dependencies.is_empty();
    let msg = if ok {
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
    (ok, msg)
}

/// Action d'installation d'une pièce (`{base}/{id}/install`) et de ses outils requis ;
/// le bilan est affiché dans `feedback` puis `on_done` est appelé.
pub(super) fn install_with_dependencies_action(
    base: &'static str,
    feedback: RwSignal<Option<String>>,
    on_done: impl Fn() + 'static,
) -> Action<String, Result<DependencyInstallResult, api::ApiError>> {
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
            on_done();
        }
        Some(Err(error)) => Toasts::set_error(feedback, error.message()),
        None => {}
    });
    action
}

/// Mention de dernière mise à jour d'une pièce du catalogue ; vide si le dépôt ne la fournit pas.
pub(super) fn updated_label(date: Option<NaiveDate>) -> String {
    date.map(|day| format!("Mis à jour le {}", format_day(day)))
        .unwrap_or_default()
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
fn card_description(entry: &CatalogEntry) -> String {
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
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct SelectedItem {
    pub id: String,
    pub name: String,
}

#[derive(Clone, PartialEq, Eq)]
enum CatalogDrawer {
    Plugin(SelectedItem),
    PackDetails(SelectedItem),
}

#[component]
fn CatalogSectionView(
    catalog: LocalResource<Result<Vec<CatalogEntry>, api::ApiError>>,
    reload_tools: RwSignal<u32>,
    can_configure: bool,
) -> impl IntoView {
    let filters = use_context::<Filters>();
    let inspecting_plugin = RwSignal::new(None::<SelectedItem>);
    let plugin_drawer_open = RwSignal::new(false);
    let plugin_tab = RwSignal::new(0_usize);
    let inspecting_pack = RwSignal::new(None::<SelectedItem>);
    let pack_drawer_open = RwSignal::new(false);
    let pack_tab = RwSignal::new(0_usize);
    let plugin_title = Signal::derive(move || {
        inspecting_plugin
            .get()
            .map(|item| item.name)
            .unwrap_or_default()
    });
    let pack_title = Signal::derive(move || {
        inspecting_pack
            .get()
            .map(|item| item.name)
            .unwrap_or_default()
    });
    let on_pack_installed = Callback::new(move |()| bump(reload_tools));
    let stats = Signal::derive(move || {
        let catalog_entries = catalog.get().and_then(Result::ok).unwrap_or_default();
        let counts = summarize_catalog(&catalog_entries);
        let active_kind = plugin_kind();
        let set_kind = move |next: PluginKindFilter| {
            Callback::new(move |()| {
                if let Some(filters) = filters {
                    filters.plugin_kind.set(next);
                }
            })
        };
        vec![
            StatItem::new("Tous", counts.total.to_string(), MetricTone::Neutral).with_filter(
                active_kind == PluginKindFilter::Any,
                set_kind(PluginKindFilter::Any),
            ),
            StatItem::new(
                "Plugins",
                counts.plugins.to_string(),
                if counts.plugins > 0 {
                    MetricTone::Success
                } else {
                    MetricTone::Neutral
                },
            )
            .with_filter(
                active_kind == PluginKindFilter::Plugins,
                set_kind(PluginKindFilter::Plugins),
            ),
            StatItem::new("Packs", counts.packs.to_string(), MetricTone::Neutral).with_filter(
                active_kind == PluginKindFilter::Packs,
                set_kind(PluginKindFilter::Packs),
            ),
            StatItem::new("Flux", counts.flows.to_string(), MetricTone::Neutral).with_filter(
                active_kind == PluginKindFilter::Flows,
                set_kind(PluginKindFilter::Flows),
            ),
            StatItem::new("Scripts", counts.scripts.to_string(), MetricTone::Neutral).with_filter(
                active_kind == PluginKindFilter::Scripts,
                set_kind(PluginKindFilter::Scripts),
            ),
            StatItem::new(
                "Ressources",
                counts.resources.to_string(),
                MetricTone::Neutral,
            )
            .with_filter(
                active_kind == PluginKindFilter::Resources,
                set_kind(PluginKindFilter::Resources),
            ),
        ]
    });

    view! {
        <StatStrip aria_label="Synthèse du catalogue" items=stats />
        <Drawer
            open=plugin_drawer_open
            eyebrow="Plugin"
            title=plugin_title
            tabs=PluginDrawerTab::tabs(can_configure).to_vec()
            tab=plugin_tab
        >
            {move || inspecting_plugin.get().map(|item| {
                view! { <PluginDetail plugin_id=item.id tab=plugin_tab can_configure=can_configure /> }
            })}
        </Drawer>
        <Drawer
            open=pack_drawer_open
            eyebrow="Pack"
            title=pack_title
            tabs=PackDrawerTab::TABS.to_vec()
            tab=pack_tab
        >
            {move || inspecting_pack.get().map(|item| {
                view! {
                    <PackDetail
                        pack_id=item.id
                        tab=pack_tab
                        can_install=can_configure
                        on_installed=on_pack_installed
                    />
                }
            })}
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
                            <div class="resource-grid resource-grid--list">
                                {shown.into_iter().map(|entry| {
                                    let id = entry.id.clone();
                                    let active_id = id.clone();
                                    let updated = updated_label(entry.updated_at);
                                    let target = match entry.entry_type {
                                        CatalogItemType::Plugin => Some((
                                            CatalogDrawer::Plugin(SelectedItem {
                                                id: id.clone(),
                                                name: entry.name.clone(),
                                            }),
                                            if can_configure { "Configurer" } else { "Détails" },
                                        )),
                                        CatalogItemType::Pack => Some((
                                            CatalogDrawer::PackDetails(SelectedItem {
                                                id: id.clone(),
                                                name: entry.name.clone(),
                                            }),
                                            if can_configure { "Voir / Installer" } else { "Détails" },
                                        )),
                                        _ => None,
                                    };
                                    let is_template = matches!(
                                        entry.entry_type,
                                        CatalogItemType::Flow | CatalogItemType::Preset
                                    );
                                    let badge_label = section_badge_label(entry.entry_type);
                                    let subtitle = card_description(&entry);
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
                                    let open_target = move |dest: CatalogDrawer| match dest {
                                        CatalogDrawer::Plugin(plugin_id) => {
                                            pack_drawer_open.set(false);
                                            inspecting_plugin.set(Some(plugin_id));
                                            plugin_tab.set(0);
                                            plugin_drawer_open.set(true);
                                        }
                                        CatalogDrawer::PackDetails(pack_id) => {
                                            plugin_drawer_open.set(false);
                                            inspecting_pack.set(Some(pack_id));
                                            pack_tab.set(0);
                                            pack_drawer_open.set(true);
                                        }
                                    };
                                    let open_drawer = target.as_ref().map(|(dest, _)| {
                                        let dest = dest.clone();
                                        Callback::new(move |()| open_target(dest.clone()))
                                    });
                                    let actions = target.as_ref().map(|(dest, label)| {
                                        let dest = dest.clone();
                                        let label = *label;
                                        view! {
                                            <button type="button" class="btn btn-secondary btn-sm"
                                                on:click=move |_| open_target(dest.clone())>
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
                                                (plugin_drawer_open.get()
                                                    && inspecting_plugin
                                                        .get()
                                                        .is_some_and(|item| item.id == active_id))
                                                    || (pack_drawer_open.get()
                                                        && inspecting_pack
                                                            .get()
                                                            .is_some_and(|item| item.id == active_id))
                                            }))
                                            on_click=open_drawer
                                            footer_meta=updated
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
                                                ().into_any()
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
    use forgearr_shared::plugins::vocab::CatalogCapability;

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
            description: None,
            capability: None,
            current_choice: None,
            distribution: None,
            category: None,
            tags: Vec::new(),
            owner: None,
            sha256: None,
            updated_at: None,
        }
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
    fn updated_label_formats_known_dates_only() {
        let date = NaiveDate::from_ymd_opt(2026, 11, 19);
        assert_eq!(updated_label(date), "Mis à jour le 19 nov. 2026");
        assert_eq!(updated_label(None), "");
    }

    #[test]
    fn dependency_install_summary_reports_failures_and_missing() {
        let mut result = DependencyInstallResult {
            item_id: "video".into(),
            installed_tools: vec!["ffmpeg".into(), "mkvtoolnix".into()],
            failed_tools: Vec::new(),
            missing_dependencies: Vec::new(),
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
    }

    #[test]
    fn summarize_and_filter_catalog_exclude_apps_and_blocks() {
        let mut plugin = entry("sonarr", CatalogItemType::Plugin);
        plugin.capability = Some(CatalogCapability::new("integration"));
        plugin.tags = vec!["arr".into(), "tv".into()];
        let flow = entry("flow.remux", CatalogItemType::Flow);
        let script = entry("script.post", CatalogItemType::Script);
        let pack = entry("pack.base", CatalogItemType::Pack);
        let preset = entry("preset.fast", CatalogItemType::Preset);
        let subtitle = entry("res.sub", CatalogItemType::ResourceSubtitle);
        let app = entry("ffmpeg", CatalogItemType::App);
        let block = entry("block.remux", CatalogItemType::Block);

        let all = vec![plugin, flow, script, pack, preset, subtitle, app, block];
        assert_eq!(
            summarize_catalog(&all),
            CatalogCounts {
                total: 6,
                plugins: 1,
                packs: 1,
                flows: 1,
                scripts: 1,
                resources: 2,
            }
        );
        assert_eq!(
            filter_catalog(all.clone(), PluginKindFilter::Any, "").len(),
            6
        );
        assert_eq!(
            filter_catalog(all.clone(), PluginKindFilter::Plugins, "arr").len(),
            1
        );
        assert_eq!(
            filter_catalog(all.clone(), PluginKindFilter::Scripts, "").len(),
            1
        );
        assert_eq!(
            filter_catalog(all.clone(), PluginKindFilter::Resources, "").len(),
            2
        );
        assert_eq!(filter_catalog(all, PluginKindFilter::Flows, "").len(), 1);
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
            section_badge_label(CatalogItemType::ResourceSubtitle),
            "Sous-titres"
        );
    }
}
