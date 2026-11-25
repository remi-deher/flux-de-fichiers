//! Onglet Installés : ce que lit le moteur, outils compris, avec filtres de type et d'état (ADR 0029 §16).

use super::metadata::{StoreCard, StoreTone, readiness_badge};
use super::subcategory_groups::group_by_subcategory;
use super::tools::{ToolGrid, filter_tools};
use super::{
    INSTALLATIONS_PATH, STORE_TOPICS, SelectedItem, card_description, section_badge_label,
    section_icon,
};
use crate::live::{Loaded, load};
use crate::ui::filters::{Filters, PluginKindFilter, plugin_kind};
use crate::ui::focus::on_activate;
use crate::ui::search::{matches, query};
use crate::ui::{
    EmptyState, Icon, IconName, MetricTone, Segmented, StatItem, StatStrip, Tag, TagTone,
};
use forgearr_shared::plugins::store::InstalledItem;
use forgearr_shared::plugins::vocab::{CatalogItemType, CatalogSection};
use forgearr_shared::plugins::{CatalogEntry, CatalogReadiness, ToolStatusSummary};
use leptos::prelude::*;
use std::collections::HashMap;

const FEATURED_PACK_ICON_SIZE: u32 = 22;

/// Filtre rapide par état opérationnel dans la vue Catalogue.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ReadinessFilter {
    #[default]
    All,
    Ready,
    ActionRequired,
    Updates,
    Damaged,
}

/// Ce que l'inscription d'un élément installé ajoute à son état.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct InstallFlags {
    pub update: bool,
    pub damaged: bool,
}

/// Mise à jour proposée et copie à réparer, par identifiant d'élément installé.
pub fn install_flags(items: &[InstalledItem]) -> HashMap<String, InstallFlags> {
    items
        .iter()
        .map(|item| {
            let flags = InstallFlags {
                update: item.update.is_some(),
                damaged: item.damaged,
            };
            (item.entry.id.clone(), flags)
        })
        .collect()
}

fn entry_readiness(entry: &CatalogEntry) -> CatalogReadiness {
    entry
        .runtime
        .as_ref()
        .map(|r| r.readiness)
        .unwrap_or(CatalogReadiness::Available)
}

impl ReadinessFilter {
    pub const ALL: &[Self] = &[
        Self::All,
        Self::Ready,
        Self::ActionRequired,
        Self::Updates,
        Self::Damaged,
    ];

    pub const fn label(self) -> &'static str {
        match self {
            Self::All => "Tous",
            Self::Ready => "Prêts",
            Self::ActionRequired => "Action requise",
            Self::Updates => "Mises à jour",
            Self::Damaged => "À réparer",
        }
    }

    pub fn accepts(self, readiness: CatalogReadiness, flags: InstallFlags) -> bool {
        match self {
            Self::Updates => flags.update,
            Self::Damaged => flags.damaged,
            Self::All => true,
            Self::Ready => matches!(
                readiness,
                CatalogReadiness::Ready | CatalogReadiness::Available
            ),
            Self::ActionRequired => matches!(
                readiness,
                CatalogReadiness::NeedsTools | CatalogReadiness::NeedsConfig
            ),
        }
    }
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

/// Filtre les entrées du catalogue par catégorie active, état opérationnel et recherche textuelle.
pub fn filter_catalog(
    entries: Vec<CatalogEntry>,
    kind: PluginKindFilter,
    readiness: ReadinessFilter,
    flags: &HashMap<String, InstallFlags>,
    text: &str,
) -> Vec<CatalogEntry> {
    entries
        .into_iter()
        .filter(|entry| {
            let entry_flags = flags.get(&entry.id).copied().unwrap_or_default();
            kind.accepts(entry)
                && readiness.accepts(entry_readiness(entry), entry_flags)
                && catalog_entry_matches(entry, text)
        })
        .collect()
}

/// Les outils s'affichent avec « Tous » et avec leur propre filtre ; les autres pièces, sauf avec « Outils ».
const fn shows_tools(kind: PluginKindFilter) -> bool {
    matches!(kind, PluginKindFilter::Any | PluginKindFilter::Tools)
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

fn primary_action_label(entry: &CatalogEntry, can_configure: bool) -> &'static str {
    let readiness = entry_readiness(entry);
    match entry.entry_type {
        CatalogItemType::Pack => {
            if can_configure {
                "Installer"
            } else {
                "Voir"
            }
        }
        CatalogItemType::Plugin => match readiness {
            CatalogReadiness::NeedsTools if can_configure => "Installer outils",
            CatalogReadiness::NeedsConfig if can_configure => "Configurer",
            _ if can_configure && entry.runtime.as_ref().is_some_and(|r| r.has_settings) => {
                "Configurer"
            }
            _ => "Ouvrir",
        },
        _ => "Aperçu",
    }
}

#[component]
fn FeaturedPackCard(
    entry: CatalogEntry,
    can_configure: bool,
    active_id: Signal<Option<String>>,
    on_open_pack: Callback<SelectedItem>,
) -> impl IntoView {
    let id = entry.id.clone();
    let check_id = id.clone();
    let name = entry.name.clone();
    let desc = card_description(&entry);
    let runtime = entry.runtime.clone().unwrap_or_default();
    let (status_label, status_tone, _) =
        readiness_badge(runtime.readiness, runtime.missing_tools_count);
    let components_label = if runtime.components_count > 0 {
        format!("{} composant(s)", runtime.components_count)
    } else {
        format!("v{}", entry.version)
    };
    let open = move || {
        on_open_pack.run(SelectedItem {
            id: id.clone(),
            name: name.clone(),
        });
    };
    let open_click = open.clone();
    let open_key = open.clone();
    let open_btn = open;
    let cta_class = if runtime.readiness == CatalogReadiness::NeedsTools && can_configure {
        "btn btn-primary btn-sm"
    } else {
        "btn btn-secondary btn-sm"
    };

    view! {
        <article
            class="featured-pack"
            class:is-open=move || active_id.get().as_deref() == Some(check_id.as_str())
            tabindex="0"
            on:click=move |_| open_click()
            on:keydown=on_activate(open_key)
        >
            <div class="featured-pack__top">
                <span class="featured-pack__icon" aria-hidden="true">
                    <Icon name=IconName::Download size=FEATURED_PACK_ICON_SIZE />
                </span>
                <Tag label=status_label tone=status_tone />
            </div>
            <div>
                <h3 class="featured-pack__title">{entry.name}</h3>
                <p class="featured-pack__desc">{desc}</p>
            </div>
            <div class="featured-pack__foot">
                <span class="store-card__version">{components_label}</span>
                <button
                    type="button"
                    class=cta_class
                    on:click=move |ev| {
                        ev.stop_propagation();
                        open_btn();
                    }
                >
                    {if can_configure { "Voir / Installer" } else { "Découvrir" }}
                </button>
            </div>
        </article>
    }
}

/// Découpage du catalogue en groupes thématiques distincts lorsque la vue « Tous » est active.
#[derive(Default)]
pub struct CatalogSections {
    pub packs: Vec<CatalogEntry>,
    pub plugins: Vec<CatalogEntry>,
    pub flows_and_scripts: Vec<CatalogEntry>,
    pub resources: Vec<CatalogEntry>,
}

pub fn partition_catalog_sections(entries: Vec<CatalogEntry>) -> CatalogSections {
    let mut sections = CatalogSections::default();
    for entry in entries {
        match entry.entry_type.section() {
            Some(CatalogSection::Packs) => sections.packs.push(entry),
            Some(CatalogSection::Plugins) => sections.plugins.push(entry),
            Some(CatalogSection::Flows | CatalogSection::Scripts) => {
                sections.flows_and_scripts.push(entry);
            }
            Some(CatalogSection::Other) => sections.resources.push(entry),
            Some(CatalogSection::Applications) | None => {}
        }
    }
    sections
}

#[component]
fn CatalogGridSection(
    #[prop(optional_no_strip)] title: Option<&'static str>,
    #[prop(optional_no_strip)] subtitle: Option<&'static str>,
    entries: Vec<CatalogEntry>,
    can_configure: bool,
    active_item_id: Signal<Option<String>>,
    on_open_plugin: Callback<SelectedItem>,
    on_open_pack: Callback<SelectedItem>,
    on_open_item: Callback<CatalogEntry>,
) -> impl IntoView {
    if entries.is_empty() {
        return ().into_any();
    }
    let count = entries.len();
    let header_subtitle = subtitle
        .map(|s| format!("{s} · {count}"))
        .unwrap_or_else(|| format!("{count} élément(s)"));
    view! {
        <div class="store-section">
            {title.map(|heading| view! {
                <div class="store-section__head">
                    <h2 class="store-section__title">{heading}</h2>
                    <p class="store-section__subtitle">{header_subtitle.clone()}</p>
                </div>
            })}
            <div class="store-grid">
                {entries.into_iter().map(|entry| {
                    let active_id = entry.id.clone();
                    let plugin_icon = entry.has_icon.then(|| entry.id.clone());
                    let desc = card_description(&entry);
                    let badge_label = section_badge_label(entry.entry_type);
                    let runtime = entry.runtime.clone().unwrap_or_default();
                    let (status_label, status_tone, card_tone) = readiness_badge(
                        runtime.readiness,
                        runtime.missing_tools_count,
                    );
                    let cta_label = primary_action_label(&entry, can_configure);
                    let is_primary_cta = matches!(
                        runtime.readiness,
                        CatalogReadiness::NeedsTools | CatalogReadiness::NeedsConfig
                    ) && can_configure;
                    let cta_class = if is_primary_cta {
                        "btn btn-primary btn-sm"
                    } else {
                        "btn btn-secondary btn-sm"
                    };

                    let status_badge = view! {
                        <Tag label=status_label tone=status_tone />
                    }.into_any();

                    let primary_context_tag = entry
                        .capability
                        .as_ref()
                        .map(|cap| cap.as_str().to_owned())
                        .unwrap_or_else(|| badge_label.to_owned());
                    let blocks_hint = (runtime.blocks_count > 0)
                        .then(|| format!("{} bloc(s)", runtime.blocks_count));
                    let context_tags = view! {
                        <Tag label=primary_context_tag tone=TagTone::Primary />
                        {blocks_hint.map(|hint| view! {
                            <Tag label=hint tone=TagTone::Neutral />
                        })}
                    }
                    .into_any();

                    let entry_for_open = entry.clone();
                    let open_entry = Callback::new(move |()| match entry_for_open.entry_type {
                        CatalogItemType::Plugin => on_open_plugin.run(SelectedItem {
                            id: entry_for_open.id.clone(),
                            name: entry_for_open.name.clone(),
                        }),
                        CatalogItemType::Pack => on_open_pack.run(SelectedItem {
                            id: entry_for_open.id.clone(),
                            name: entry_for_open.name.clone(),
                        }),
                        _ => on_open_item.run(entry_for_open.clone()),
                    });
                    let open_for_btn = open_entry;
                    let actions = view! {
                        <button
                            type="button"
                            class=cta_class
                            on:click=move |_| open_for_btn.run(())
                        >
                            {cta_label}
                        </button>
                    }.into_any();

                    let icon = section_icon(entry.entry_type);
                    let tone = if entry.entry_type == CatalogItemType::Flow {
                        StoreTone::Warning
                    } else {
                        card_tone
                    };

                    view! {
                        <StoreCard
                            focus_key=entry.id
                            title=entry.name
                            description=desc
                            icon=icon
                            plugin_icon=plugin_icon
                            tone=tone
                            status_badge=Some(status_badge)
                            context_tags=Some(context_tags)
                            footer_left=format!("v{}", entry.version)
                            footer_actions=Some(actions)
                            active=Some(Signal::derive(move || {
                                active_item_id.get().as_deref() == Some(active_id.as_str())
                            }))
                            on_open=open_entry
                        />
                    }
                }).collect_view()}
            </div>
        </div>
    }
    .into_any()
}

/// Grille de pièces regroupées par sous-catégorie officielle ; sans sous-catégorie, une seule grille.
#[component]
fn GroupedCatalogSections(
    #[prop(optional_no_strip)] title: Option<&'static str>,
    #[prop(optional_no_strip)] subtitle: Option<&'static str>,
    entries: Vec<CatalogEntry>,
    can_configure: bool,
    active_item_id: Signal<Option<String>>,
    on_open_plugin: Callback<SelectedItem>,
    on_open_pack: Callback<SelectedItem>,
    on_open_item: Callback<CatalogEntry>,
) -> impl IntoView {
    let grouped = group_by_subcategory(entries);
    view! {
        <CatalogGridSection
            title=title
            subtitle=subtitle
            entries=grouped.ungrouped
            can_configure=can_configure
            active_item_id=active_item_id
            on_open_plugin=on_open_plugin
            on_open_pack=on_open_pack
            on_open_item=on_open_item
        />
        {grouped.groups.into_iter().map(|group| view! {
            <CatalogGridSection
                title=Some(group.title)
                entries=group.entries
                can_configure=can_configure
                active_item_id=active_item_id
                on_open_plugin=on_open_plugin
                on_open_pack=on_open_pack
                on_open_item=on_open_item
            />
        }).collect_view()}
    }
}

/// Rubrique « Outils » de l'onglet ; rien si aucun outil ne passe les filtres.
#[component]
fn ToolsGridSection(
    tools: Vec<ToolStatusSummary>,
    can_install: bool,
    active_tool_id: Signal<Option<String>>,
    on_open_tool: Callback<SelectedItem>,
) -> impl IntoView {
    (!tools.is_empty()).then(|| {
        let subtitle = format!("Binaires exécutés sur les nœuds · {}", tools.len());
        view! {
            <div class="store-section">
                <div class="store-section__head">
                    <h2 class="store-section__title">"Outils"</h2>
                    <p class="store-section__subtitle">{subtitle}</p>
                </div>
                <ToolGrid
                    tools=tools
                    can_install=can_install
                    active_tool_id=active_tool_id
                    on_open_tool=on_open_tool
                />
            </div>
        }
    })
}

#[component]
pub fn CatalogSectionView(
    catalog: Loaded<Vec<CatalogEntry>>,
    tools: Loaded<Vec<ToolStatusSummary>>,
    can_configure: bool,
    active_item_id: Signal<Option<String>>,
    active_tool_id: Signal<Option<String>>,
    on_open_plugin: Callback<SelectedItem>,
    on_open_pack: Callback<SelectedItem>,
    on_open_item: Callback<CatalogEntry>,
    on_open_tool: Callback<SelectedItem>,
) -> impl IntoView {
    let filters = use_context::<Filters>();
    let readiness_filter = RwSignal::new(ReadinessFilter::All);
    let installed = load::<Vec<InstalledItem>>(STORE_TOPICS, INSTALLATIONS_PATH);
    let flags = Memo::new(move |_| {
        install_flags(&installed.get().and_then(Result::ok).unwrap_or_default())
    });
    let tool_list = Memo::new(move |_| tools.get().and_then(Result::ok).unwrap_or_default());

    let stats = Signal::derive(move || {
        let catalog_entries = catalog.get().and_then(Result::ok).unwrap_or_default();
        let counts = summarize_catalog(&catalog_entries);
        let tools_count = tool_list.read().len();
        let active_kind = plugin_kind();
        let set_kind = move |next: PluginKindFilter| {
            Callback::new(move |()| {
                if let Some(filters) = filters {
                    filters.plugin_kind.set(next);
                }
            })
        };
        vec![
            StatItem::new(
                "Tous",
                (counts.total + tools_count).to_string(),
                MetricTone::Neutral,
            )
            .with_filter(
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
            StatItem::new("Outils", tools_count.to_string(), MetricTone::Neutral).with_filter(
                active_kind == PluginKindFilter::Tools,
                set_kind(PluginKindFilter::Tools),
            ),
        ]
    });

    view! {
        <div class="store-toolbar">
            <StatStrip aria_label="Synthèse du catalogue" items=stats />
            <div class="store-toolbar__actions">
                <Segmented
                    options=ReadinessFilter::ALL
                    selected=readiness_filter
                    label=ReadinessFilter::label
                    aria_label="Filtrer par état"
                />
            </div>
        </div>
        <section class="page-section">
            {move || {
                let text = query();
                let kind = plugin_kind();
                let readiness = readiness_filter.get();
                let flags = flags.read();
                let shown_tools = if shows_tools(kind) {
                    filter_tools(tool_list.get(), readiness, &flags, &text)
                } else {
                    Vec::new()
                };
                let tools_view = view! {
                    <ToolsGridSection
                        tools=shown_tools.clone()
                        can_install=can_configure
                        active_tool_id=active_tool_id
                        on_open_tool=on_open_tool
                    />
                };
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
                        let shown = filter_catalog(all, kind, readiness, &flags, &text);
                        if shown.is_empty() && shown_tools.is_empty() {
                            let unfiltered = text.trim().is_empty()
                                && kind == PluginKindFilter::Any
                                && readiness == ReadinessFilter::All;
                            return view! {
                                <div class="empty-state-panel">
                                    <EmptyState
                                        title=if unfiltered {
                                            "Catalogue vide"
                                        } else {
                                            "Aucune extension correspondante"
                                        }
                                        message=if unfiltered {
                                            "Synchronisez un dépôt de plugins pour peupler ce catalogue."
                                        } else {
                                            "Ajustez votre recherche ou les filtres actifs pour afficher des extensions."
                                        }
                                        icon_name=IconName::Plugins
                                    />
                                </div>
                            }
                            .into_any();
                        }

                        if kind == PluginKindFilter::Tools {
                            return tools_view.into_any();
                        }
                        if kind == PluginKindFilter::Any {
                            let sections = partition_catalog_sections(shown);
                            let show_featured_packs = text.trim().is_empty()
                                && readiness == ReadinessFilter::All
                                && !sections.packs.is_empty();
                            let fallback_packs = if show_featured_packs {
                                Vec::new()
                            } else {
                                sections.packs.clone()
                            };
                            view! {
                                {show_featured_packs.then(|| view! {
                                    <div class="store-section">
                                        <div class="store-section__head">
                                            <h2 class="store-section__title">"Packs recommandés"</h2>
                                            <p class="store-section__subtitle">"Ensembles prêts à l'emploi installables en un clic"</p>
                                        </div>
                                        <div class="featured-packs">
                                            {sections.packs.into_iter().map(|pack| view! {
                                                <FeaturedPackCard
                                                    entry=pack
                                                    can_configure=can_configure
                                                    active_id=active_item_id
                                                    on_open_pack=on_open_pack
                                                />
                                            }).collect_view()}
                                        </div>
                                    </div>
                                })}
                                <CatalogGridSection
                                    title=Some("Packs")
                                    subtitle=Some("Ensembles prêts à l'emploi")
                                    entries=fallback_packs
                                    can_configure=can_configure
                                    active_item_id=active_item_id
                                    on_open_plugin=on_open_plugin
                                    on_open_pack=on_open_pack
                                    on_open_item=on_open_item
                                />
                                <GroupedCatalogSections
                                    title=Some("Plugins")
                                    subtitle=Some("Extensions ajoutant des blocs et intégrations à vos flux")
                                    entries=sections.plugins
                                    can_configure=can_configure
                                    active_item_id=active_item_id
                                    on_open_plugin=on_open_plugin
                                    on_open_pack=on_open_pack
                                    on_open_item=on_open_item
                                />
                                <CatalogGridSection
                                    title=Some("Flux & scripts")
                                    subtitle=Some("Modèles de flux prêts à l'emploi et scripts exécutables")
                                    entries=sections.flows_and_scripts
                                    can_configure=can_configure
                                    active_item_id=active_item_id
                                    on_open_plugin=on_open_plugin
                                    on_open_pack=on_open_pack
                                    on_open_item=on_open_item
                                />
                                <CatalogGridSection
                                    title=Some("Ressources")
                                    subtitle=Some("Préréglages, polices, images et fichiers partagés")
                                    entries=sections.resources
                                    can_configure=can_configure
                                    active_item_id=active_item_id
                                    on_open_plugin=on_open_plugin
                                    on_open_pack=on_open_pack
                                    on_open_item=on_open_item
                                />
                                {tools_view}
                            }
                            .into_any()
                        } else {
                            view! {
                                <GroupedCatalogSections
                                    entries=shown
                                    can_configure=can_configure
                                    active_item_id=active_item_id
                                    on_open_plugin=on_open_plugin
                                    on_open_pack=on_open_pack
                                    on_open_item=on_open_item
                                />
                            }
                            .into_any()
                        }
                    }
                }
            }}
        </section>
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use forgearr_shared::plugins::CatalogRuntimeSummary;
    use forgearr_shared::plugins::vocab::CatalogCapability;

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
    fn summarize_and_filter_catalog_exclude_apps_and_blocks() {
        let mut plugin = entry("sonarr", CatalogItemType::Plugin);
        plugin.capability = Some(CatalogCapability::new("integration"));
        plugin.tags = vec!["arr".into(), "tv".into()];
        plugin.runtime = Some(CatalogRuntimeSummary {
            readiness: CatalogReadiness::NeedsConfig,
            has_settings: true,
            configured: false,
            ..Default::default()
        });
        let flow = entry("flow.remux", CatalogItemType::Flow);
        let script = entry("script.post", CatalogItemType::Script);
        let pack = entry("pack.base", CatalogItemType::Pack);
        let preset = entry("preset.fast", CatalogItemType::Preset);
        let subtitle = entry("res.sub", CatalogItemType::ResourceSubtitle);
        let app = entry("ffmpeg", CatalogItemType::App);
        let block = entry("block.remux", CatalogItemType::Block);

        let all = vec![plugin, flow, script, pack, preset, subtitle, app, block];
        let no_flags = HashMap::new();
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
            filter_catalog(
                all.clone(),
                PluginKindFilter::Any,
                ReadinessFilter::All,
                &no_flags,
                ""
            )
            .len(),
            6
        );
        assert_eq!(
            filter_catalog(
                all.clone(),
                PluginKindFilter::Any,
                ReadinessFilter::ActionRequired,
                &no_flags,
                ""
            )
            .len(),
            1
        );
        assert_eq!(
            filter_catalog(
                all.clone(),
                PluginKindFilter::Plugins,
                ReadinessFilter::All,
                &no_flags,
                "arr"
            )
            .len(),
            1
        );
        assert_eq!(
            filter_catalog(
                all.clone(),
                PluginKindFilter::Scripts,
                ReadinessFilter::All,
                &no_flags,
                ""
            )
            .len(),
            1
        );
        assert_eq!(
            filter_catalog(
                all.clone(),
                PluginKindFilter::Resources,
                ReadinessFilter::All,
                &no_flags,
                ""
            )
            .len(),
            2
        );
        assert_eq!(
            filter_catalog(
                all,
                PluginKindFilter::Flows,
                ReadinessFilter::All,
                &no_flags,
                ""
            )
            .len(),
            1
        );
    }

    #[test]
    fn updates_and_repairs_come_from_the_installation_record() {
        let update = InstallFlags {
            update: true,
            damaged: false,
        };
        let damaged = InstallFlags {
            update: false,
            damaged: true,
        };
        assert!(ReadinessFilter::Updates.accepts(CatalogReadiness::Ready, update));
        assert!(!ReadinessFilter::Updates.accepts(CatalogReadiness::Ready, damaged));
        assert!(ReadinessFilter::Damaged.accepts(CatalogReadiness::Ready, damaged));
        assert!(!ReadinessFilter::Damaged.accepts(CatalogReadiness::NeedsTools, update));
        assert!(
            ReadinessFilter::All.accepts(CatalogReadiness::NeedsTools, InstallFlags::default())
        );
        assert!(shows_tools(PluginKindFilter::Tools));
        assert!(shows_tools(PluginKindFilter::Any));
        assert!(!shows_tools(PluginKindFilter::Plugins));
        let mut flagged = HashMap::new();
        flagged.insert("sonarr".to_owned(), update);
        let shown = filter_catalog(
            vec![
                entry("sonarr", CatalogItemType::Plugin),
                entry("radarr", CatalogItemType::Plugin),
            ],
            PluginKindFilter::Any,
            ReadinessFilter::Updates,
            &flagged,
            "",
        );
        assert_eq!(shown.len(), 1);
        assert!(
            filter_catalog(
                vec![entry("ffmpeg", CatalogItemType::App)],
                PluginKindFilter::Tools,
                ReadinessFilter::All,
                &flagged,
                "",
            )
            .is_empty()
        );
    }

    #[test]
    fn partition_catalog_sections_separates_plugins_flows_and_resources() {
        let sections = partition_catalog_sections(vec![
            entry("pack.media", CatalogItemType::Pack),
            entry("sonarr", CatalogItemType::Plugin),
            entry("flow.remux", CatalogItemType::Flow),
            entry("script.clean", CatalogItemType::Script),
            entry("res.font", CatalogItemType::ResourceFont),
            entry("ffmpeg", CatalogItemType::App),
            entry("block.remux", CatalogItemType::Block),
        ]);
        assert_eq!(sections.packs.len(), 1);
        assert_eq!(sections.plugins.len(), 1);
        assert_eq!(sections.flows_and_scripts.len(), 2);
        assert_eq!(sections.resources.len(), 1);
    }
}
