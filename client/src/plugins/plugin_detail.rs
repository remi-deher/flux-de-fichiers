//! Tiroir d'inspection à onglets d'un plugin : aperçu orienté action, blocs, paramètres et informations.

use super::exposed_variables::ExposedVariablesSection;
use super::metadata::{CommonInfoRows, MetaRow, StatusBanner, StoreTone, tool_status_tag};
use super::settings::PluginSettings;
use super::uninstall::InstalledActions;

/// Code d'erreur d'un plugin qui n'est plus installé.
const NOT_FOUND: &str = "not_found";
use super::{CATALOG_PATH, CATALOG_TOPICS, SelectedItem, install_with_dependencies_action};
use crate::api::ApiError;
use crate::flows::NodeDomain;
use crate::live::load;
use crate::route::encode_segment;
use crate::ui::{Icon, IconName, Tag, TagTone};
use forgearr_shared::flows::OwnedBlockSpec;
#[cfg(test)]
use forgearr_shared::flows::{OwnedFieldKind, OwnedFieldSpec};
use forgearr_shared::plugins::{
    DependencyInstallResult, PluginDependency, PluginDetailView, ToolStatus,
};
use leptos::prelude::*;

const BLOCK_CARD_ICON_SIZE: u32 = 16;
const ACTION_ICON_SIZE: u32 = 14;

/// Onglets du tiroir d'un plugin, ordonnés par usage.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PluginDrawerTab {
    Blocks,
    Settings,
    Info,
}

impl PluginDrawerTab {
    pub const fn tabs(can_configure: bool) -> &'static [&'static str] {
        if can_configure {
            &["Blocs & Outils", "Paramètres", "Informations"]
        } else {
            &["Blocs & Outils", "Informations"]
        }
    }

    pub const fn from_index(index: usize, can_configure: bool) -> Self {
        if can_configure {
            match index {
                0 => Self::Blocks,
                1 => Self::Settings,
                _ => Self::Info,
            }
        } else {
            match index {
                0 => Self::Blocks,
                _ => Self::Info,
            }
        }
    }
}

#[cfg(test)]
fn field_kind_label(kind: &OwnedFieldKind) -> &'static str {
    match kind {
        OwnedFieldKind::Text => "Texte",
        OwnedFieldKind::Bool => "Booléen",
        OwnedFieldKind::Select(_) => "Sélection",
        OwnedFieldKind::Headers => "En-têtes",
        OwnedFieldKind::Worker => "Worker",
        OwnedFieldKind::Condition => "Condition",
        OwnedFieldKind::Flow => "Flux",
    }
}

#[cfg(test)]
fn format_field_chip(field: &OwnedFieldSpec) -> String {
    let required = if field.required { " *" } else { "" };
    format!(
        "{}{} ({})",
        field.label,
        required,
        field_kind_label(&field.kind)
    )
}

pub fn count_missing_dependencies(deps: &[PluginDependency]) -> usize {
    deps.iter()
        .filter(|dep| !dep.optional)
        .filter(|dep| dep.status.is_some_and(|st| st != ToolStatus::Ready))
        .count()
}

#[component]
fn PluginActionBanner(
    detail: PluginDetailView,
    can_configure: bool,
    install: Action<String, Result<DependencyInstallResult, ApiError>>,
    feedback: RwSignal<Option<String>>,
    tab: RwSignal<usize>,
) -> impl IntoView {
    let plugin_id = detail.id.clone();
    let missing = count_missing_dependencies(&detail.dependencies);
    let has_deps = !detail.dependencies.is_empty();

    let (title, text, tone) = if missing > 0 {
        (
            format!("{missing} outil(s) requis à installer"),
            Some("Installez les binaires nécessaires pour activer les blocs de ce plugin.".into()),
            StoreTone::Warning,
        )
    } else if detail.has_settings {
        (
            "Plugin prêt à configurer".to_owned(),
            Some("Renseignez ses identifiants dans l'onglet Paramètres.".into()),
            StoreTone::Primary,
        )
    } else {
        (
            "Plugin prêt à l'emploi".to_owned(),
            Some(format!(
                "{} bloc(s) disponible(s) dans l'éditeur de flux.",
                detail.blocks.len()
            )),
            StoreTone::Success,
        )
    };

    let action = if can_configure && has_deps && missing > 0 {
        let id = plugin_id.clone();
        Some(
            view! {
                <button
                    type="button"
                    class="btn btn-primary btn-sm"
                    disabled=move || install.pending().get()
                    on:click=move |_| { install.dispatch(id.clone()); }
                >
                    <Icon name=IconName::Download size=ACTION_ICON_SIZE />
                    <span>{move || if install.pending().get() { "Installation…" } else { "Installer les outils" }}</span>
                </button>
            }
            .into_any(),
        )
    } else if can_configure && detail.has_settings && tab.get_untracked() != 1 {
        Some(
            view! {
                <button
                    type="button"
                    class="btn btn-secondary btn-sm"
                    on:click=move |_| tab.set(1)
                >
                    "Configurer"
                </button>
            }
            .into_any(),
        )
    } else {
        None
    };

    view! {
        <StatusBanner title=title text=text tone=tone action=action />
        {move || feedback.get().map(|msg| view! {
            <p class="form-help" role="status">{msg}</p>
        })}
    }
}

#[component]
fn DependenciesList(
    dependencies: Vec<PluginDependency>,
    on_open_tool: Callback<SelectedItem>,
) -> impl IntoView {
    if dependencies.is_empty() {
        return ().into_any();
    }
    view! {
        <div class="pack-components">
            <h3 class="form-label">"Outils & dépendances"</h3>
            <ul class="step-list">
                {dependencies.into_iter().map(|dep| {
                    let dep_id = dep.id.clone();
                    let dep_name = dep.id.clone();
                    let is_tool = dep.status.is_some() || dep.kind.as_deref() == Some("app");
                    let status_view = dep.status.map(|st| {
                        let (label, tone, _) = tool_status_tag(st);
                        view! { <Tag label=label tone=tone /> }
                    });
                    view! {
                        <li>
                            {if is_tool {
                                view! {
                                    <button
                                        type="button"
                                        class="dep-link"
                                        on:click=move |_| on_open_tool.run(SelectedItem {
                                            id: dep_id.clone(),
                                            name: dep_name.clone(),
                                        })
                                    >
                                        <span class="dep-link__main">
                                            <Icon name=IconName::Package size=15 />
                                            <span class="text-mono text-strong">{dep.id}</span>
                                            {dep.version_range.map(|range| view! {
                                                <Tag label=range tone=TagTone::Mono />
                                            })}
                                        </span>
                                        <span class="dep-link__meta">
                                            {status_view}
                                            <Tag
                                                label=if dep.optional { "Optionnelle" } else { "Requise" }
                                                tone=if dep.optional { TagTone::Neutral } else { TagTone::Primary }
                                            />
                                        </span>
                                    </button>
                                }
                                .into_any()
                            } else {
                                view! {
                                    <div class="step-item">
                                        <div class="step-item__head">
                                            <span class="text-mono text-strong">{dep.id}</span>
                                            {dep.version_range.map(|range| view! {
                                                <Tag label=range tone=TagTone::Mono />
                                            })}
                                            <Tag
                                                label=if dep.optional { "Optionnelle" } else { "Requise" }
                                                tone=if dep.optional { TagTone::Neutral } else { TagTone::Primary }
                                            />
                                        </div>
                                    </div>
                                }
                                .into_any()
                            }}
                        </li>
                    }
                }).collect_view()}
            </ul>
        </div>
    }
    .into_any()
}

#[component]
fn BlocksTab(
    blocks: Vec<OwnedBlockSpec>,
    dependencies: Vec<PluginDependency>,
    on_open_tool: Callback<SelectedItem>,
) -> impl IntoView {
    let mut grouped: Vec<(String, Vec<OwnedBlockSpec>)> = Vec::new();
    for block in blocks {
        if let Some((_, items)) = grouped.iter_mut().find(|(cat, _)| cat == &block.category) {
            items.push(block);
        } else {
            grouped.push((block.category.clone(), vec![block]));
        }
    }
    view! {
        <DependenciesList dependencies=dependencies on_open_tool=on_open_tool />
        {if grouped.is_empty() {
            view! {
                <p class="text-dim">"Ce plugin n'expose aucun bloc exécutable."</p>
            }
            .into_any()
        } else {
            view! {
                <div class="flow-palette__groups">
                    {grouped.into_iter().map(|(category, items)| {
                        let count = items.len();
                        view! {
                            <details class="flow-palette__group" open=true>
                                <summary class="flow-palette__summary">
                                    <span>{category}</span>
                                    <span class="flow-palette__count">{count}</span>
                                </summary>
                                <div class="flow-palette__list">
                                    {items.into_iter().map(|block| {
                                        let domain = NodeDomain::from_spec(Some(&block), &block.category);
                                        let accent_style = format!("--component-accent: {}", domain.accent_var());
                                        let label = block.label.clone();
                                        let description = block.description.clone();
                                        let meta = if !description.is_empty() {
                                            description.clone()
                                        } else if !block.fields.is_empty() {
                                            format!("{} paramètre(s)", block.fields.len())
                                        } else {
                                            block.kind.clone()
                                        };
                                        view! {
                                            <div class="flow-palette__card" style=accent_style title=description>
                                                <span class="flow-palette__card-icon" aria-hidden="true">
                                                    <Icon name=domain.icon() size=BLOCK_CARD_ICON_SIZE />
                                                </span>
                                                <span class="flow-palette__card-text">
                                                    <span class="flow-palette__card-title">{label}</span>
                                                    <span class="flow-palette__card-meta">{meta}</span>
                                                </span>
                                            </div>
                                        }
                                    }).collect_view()}
                                </div>
                            </details>
                        }
                    }).collect_view()}
                </div>
            }
            .into_any()
        }}
    }
}

#[component]
fn InfoTab(detail: PluginDetailView) -> impl IntoView {
    let blocks_count = detail.blocks.len().to_string();
    let settings_label = if detail.has_settings {
        "Configurables"
    } else {
        "Aucun réglage global"
    };
    view! {
        {detail.description.map(|desc| view! {
            <p class="text-sm">{desc}</p>
        })}
        <div class="detail-grid">
            <CommonInfoRows
                id=detail.id
                version=detail.version
                updated_at=detail.updated_at
                author=detail.author
                license=detail.license
                app_min=detail.compatibility.app_min
                platforms=detail.compatibility.platforms
                tags=detail.tags
            />
            {detail.capability.map(|cap| view! {
                <MetaRow term="Capacité">
                    <Tag label=cap.as_str().to_owned() tone=TagTone::Success />
                </MetaRow>
            })}
            {detail.category.filter(|cat| cat.shows_badge()).map(|cat| view! {
                <MetaRow term="Catégorie">
                    <Tag label=cat.as_str() tone=TagTone::Neutral />
                </MetaRow>
            })}
            <MetaRow term="Blocs exposés">{blocks_count}</MetaRow>
            <MetaRow term="Paramètres">{settings_label}</MetaRow>
        </div>
        <ExposedVariablesSection exposures=detail.exposures />
    }
}

#[component]
pub fn PluginDetail(
    plugin_id: String,
    tab: RwSignal<usize>,
    can_configure: bool,
    on_open_tool: Callback<SelectedItem>,
) -> impl IntoView {
    // Hors de la fiche relue : l'installation en cours garde son état et son bilan.
    let feedback = RwSignal::new(None::<String>);
    let install = install_with_dependencies_action(CATALOG_PATH, feedback);
    let path = format!("/api/v1/plugins/{}", encode_segment(&plugin_id));
    let detail = load::<PluginDetailView>(CATALOG_TOPICS, path);
    let active_tab = Memo::new(move |_| PluginDrawerTab::from_index(tab.get(), can_configure));
    // Une fois connus, les réglages restent : une relecture, même en échec, ne retire pas la saisie.
    let has_settings = Memo::new(move |known: Option<&bool>| {
        known.copied().unwrap_or_default()
            || detail
                .read()
                .as_ref()
                .and_then(|read| read.as_ref().ok())
                .is_some_and(|data| data.has_settings)
    });
    let settings_shown =
        Memo::new(move |_| has_settings.get() && active_tab.get() == PluginDrawerTab::Settings);

    view! {
        <div class="inspection-panel">
            {move || {
                let active_tab = active_tab.get();
                match detail.get() {
                    None => view! { <p class="text-dim">"Chargement…"</p> }.into_any(),
                    Some(Err(error)) if error.code() == Some(NOT_FOUND) => view! {
                        <p class="text-dim">"Ce plugin n'est plus installé ; il reste disponible dans le Store."</p>
                    }.into_any(),
                    Some(Err(error)) => view! { <p class="text-danger">{error.message()}</p> }.into_any(),
                    Some(Ok(data)) => {
                        let banner_data = data.clone();
                        view! {
                            <PluginActionBanner
                                detail=banner_data
                                can_configure=can_configure
                                install=install
                                feedback=feedback
                                tab=tab
                            />
                            {match active_tab {
                                PluginDrawerTab::Blocks => view! {
                                    <BlocksTab
                                        blocks=data.blocks
                                        dependencies=data.dependencies
                                        on_open_tool=on_open_tool
                                    />
                                }.into_any(),
                                PluginDrawerTab::Settings => {
                                    // Le formulaire est rendu à part, après la fiche.
                                    if has_settings.get() {
                                        ().into_any()
                                    } else {
                                        view! {
                                            <p class="text-dim">
                                                "Ce plugin n'expose aucun réglage global. Ses options se configurent directement sur chaque bloc dans l'éditeur de flux."
                                            </p>
                                        }
                                        .into_any()
                                    }
                                }
                                PluginDrawerTab::Info => view! { <InfoTab detail=data /> }.into_any(),
                            }}
                        }
                        .into_any()
                    }
                }
            }}
            {
                let plugin_id = plugin_id.clone();
                move || settings_shown.get().then(|| view! {
                    <PluginSettings plugin_id=plugin_id.clone() />
                })
            }
            <InstalledActions item_id=plugin_id can_manage=can_configure />
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drawer_tabs_adapt_to_configure_permission() {
        assert_eq!(
            PluginDrawerTab::tabs(true),
            &["Blocs & Outils", "Paramètres", "Informations"]
        );
        assert_eq!(
            PluginDrawerTab::tabs(false),
            &["Blocs & Outils", "Informations"]
        );
        assert_eq!(
            PluginDrawerTab::from_index(0, true),
            PluginDrawerTab::Blocks
        );
        assert_eq!(
            PluginDrawerTab::from_index(1, true),
            PluginDrawerTab::Settings
        );
        assert_eq!(PluginDrawerTab::from_index(2, true), PluginDrawerTab::Info);
        assert_eq!(
            PluginDrawerTab::from_index(0, false),
            PluginDrawerTab::Blocks
        );
        assert_eq!(PluginDrawerTab::from_index(1, false), PluginDrawerTab::Info);
    }

    #[test]
    fn count_missing_dependencies_ignores_optional_and_ready() {
        let deps = vec![
            PluginDependency {
                id: "ffmpeg".into(),
                version_range: None,
                kind: Some("app".into()),
                optional: false,
                status: Some(ToolStatus::NotInstalled),
            },
            PluginDependency {
                id: "mkvtoolnix".into(),
                version_range: None,
                kind: Some("app".into()),
                optional: false,
                status: Some(ToolStatus::Ready),
            },
            PluginDependency {
                id: "handbrake".into(),
                version_range: None,
                kind: Some("app".into()),
                optional: true,
                status: Some(ToolStatus::NotInstalled),
            },
        ];
        assert_eq!(count_missing_dependencies(&deps), 1);
    }

    #[test]
    fn field_chip_formats_required_and_kind_labels() {
        let required = OwnedFieldSpec {
            name: "url".into(),
            label: "URL".into(),
            kind: OwnedFieldKind::Text,
            required: true,
            help: String::new(),
            default: None,
        };
        assert_eq!(format_field_chip(&required), "URL * (Texte)");

        let optional = OwnedFieldSpec {
            name: "codec".into(),
            label: "Codec".into(),
            kind: OwnedFieldKind::Select(vec![]),
            required: false,
            help: String::new(),
            default: None,
        };
        assert_eq!(format_field_chip(&optional), "Codec (Sélection)");
    }
}
