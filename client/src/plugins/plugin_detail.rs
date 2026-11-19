//! Tiroir d'inspection à onglets d'un plugin : blocs exposés, paramètres et informations.

use super::settings::PluginSettings;
use super::{CATALOG_PATH, date_row, install_with_dependencies_action};
use crate::api;
use crate::flows::NodeDomain;
use crate::route::encode_segment;
use crate::ui::{Icon, Tag, TagTone};
use flux_shared::flows::OwnedBlockSpec;
#[cfg(test)]
use flux_shared::flows::{OwnedFieldKind, OwnedFieldSpec};
use flux_shared::plugins::PluginDetailView;
use leptos::prelude::*;

const BLOCK_CARD_ICON_SIZE: u32 = 16;

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
            &["Blocs", "Paramètres", "Informations"]
        } else {
            &["Blocs", "Informations"]
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

#[component]
fn BlocksTab(blocks: Vec<OwnedBlockSpec>) -> impl IntoView {
    if blocks.is_empty() {
        return view! {
            <p class="text-dim">"Ce plugin n'expose aucun bloc exécutable."</p>
        }
        .into_any();
    }
    let mut grouped: Vec<(String, Vec<OwnedBlockSpec>)> = Vec::new();
    for block in blocks {
        if let Some((_, items)) = grouped.iter_mut().find(|(cat, _)| cat == &block.category) {
            items.push(block);
        } else {
            grouped.push((block.category.clone(), vec![block]));
        }
    }
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
}

#[component]
fn InfoTab(detail: PluginDetailView, can_install: bool) -> impl IntoView {
    let plugin_id = detail.id.clone();
    let has_dependencies = !detail.dependencies.is_empty();
    let feedback = RwSignal::new(None::<String>);
    let install = install_with_dependencies_action(CATALOG_PATH, feedback, || {});
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
            <div class="detail-term">"Identifiant"</div>
            <div class="detail-val text-mono">{detail.id}</div>
            <div class="detail-term">"Version"</div>
            <div class="detail-val text-mono">{detail.version}</div>
            {date_row("Mise à jour", detail.updated_at)}
            {detail.capability.map(|cap| view! {
                <div class="detail-term">"Capacité"</div>
                <div class="detail-val"><Tag label=cap.as_str().to_owned() tone=TagTone::Success /></div>
            })}
            {detail.category.filter(|cat| cat.shows_badge()).map(|cat| view! {
                <div class="detail-term">"Catégorie"</div>
                <div class="detail-val"><Tag label=cat.as_str() tone=TagTone::Neutral /></div>
            })}
            {detail.author.map(|author| view! {
                <div class="detail-term">"Auteur"</div>
                <div class="detail-val">{author}</div>
            })}
            {detail.license.map(|license| view! {
                <div class="detail-term">"Licence"</div>
                <div class="detail-val text-mono">{license}</div>
            })}
            <div class="detail-term">"Blocs exposés"</div>
            <div class="detail-val">{blocks_count}</div>
            <div class="detail-term">"Paramètres"</div>
            <div class="detail-val">{settings_label}</div>
            {detail.compatibility.app_min.map(|min| view! {
                <div class="detail-term">"App minimale"</div>
                <div class="detail-val text-mono">{min}</div>
            })}
            {(!detail.compatibility.platforms.is_empty()).then(|| view! {
                <div class="detail-term">"Plateformes"</div>
                <div class="detail-val">
                    <div class="cluster">
                        {detail.compatibility.platforms.into_iter().map(|plat| view! {
                            <Tag label=plat tone=TagTone::Mono />
                        }).collect_view()}
                    </div>
                </div>
            })}
            {(!detail.tags.is_empty()).then(|| view! {
                <div class="detail-term">"Étiquettes"</div>
                <div class="detail-val">
                    <div class="cluster">
                        {detail.tags.into_iter().map(|tag| view! {
                            <Tag label=tag tone=TagTone::Neutral />
                        }).collect_view()}
                    </div>
                </div>
            })}
        </div>
        {(!detail.dependencies.is_empty()).then(|| view! {
            <div class="pack-components">
                <h3 class="form-label">"Dépendances"</h3>
                <ul class="step-list">
                    {detail.dependencies.into_iter().map(|dep| view! {
                        <li class="step-item">
                            <div class="step-item__head">
                                <span class="text-mono text-strong">{dep.id}</span>
                                {dep.version_range.map(|range| view! {
                                    <Tag label=range tone=TagTone::Mono />
                                })}
                                {dep.kind.map(|kind| view! {
                                    <Tag label=kind tone=TagTone::Neutral />
                                })}
                                <Tag
                                    label=if dep.optional { "Optionnelle" } else { "Requise" }
                                    tone=if dep.optional { TagTone::Neutral } else { TagTone::Primary }
                                />
                            </div>
                        </li>
                    }).collect_view()}
                </ul>
                {(can_install && has_dependencies).then(|| view! {
                    <button
                        type="button"
                        class="btn btn-primary btn-sm"
                        disabled=move || install.pending().get()
                        on:click=move |_| { install.dispatch(plugin_id.clone()); }
                    >
                        {move || if install.pending().get() {
                            "Installation…"
                        } else {
                            "Installer les outils requis"
                        }}
                    </button>
                })}
                <p class="form-help" role="status">{move || feedback.get()}</p>
            </div>
        })}
    }
}

#[component]
pub fn PluginDetail(plugin_id: String, tab: RwSignal<usize>, can_configure: bool) -> impl IntoView {
    let path = format!("/api/v1/plugins/{}", encode_segment(&plugin_id));
    let detail = LocalResource::new(move || {
        let path = path.clone();
        async move { api::get::<PluginDetailView>(&path).await }
    });
    let settings_id = plugin_id.clone();

    view! {
        <div class="inspection-panel">
            {move || {
                let active_tab = PluginDrawerTab::from_index(tab.get(), can_configure);
                match detail.get() {
                    None => view! { <p class="text-dim">"Chargement…"</p> }.into_any(),
                    Some(Err(error)) => view! { <p class="text-danger">{error.message()}</p> }.into_any(),
                    Some(Ok(data)) => match active_tab {
                        PluginDrawerTab::Blocks => view! { <BlocksTab blocks=data.blocks /> }.into_any(),
                        PluginDrawerTab::Settings => {
                            if data.has_settings {
                                view! { <PluginSettings plugin_id=settings_id.clone() /> }.into_any()
                            } else {
                                view! {
                                    <p class="text-dim">
                                        "Ce plugin n'expose aucun réglage global. Ses options se configurent directement sur chaque bloc dans l'éditeur de flux."
                                    </p>
                                }
                                .into_any()
                            }
                        }
                        PluginDrawerTab::Info => view! { <InfoTab detail=data can_install=can_configure /> }.into_any(),
                    },
                }
            }}
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
            &["Blocs", "Paramètres", "Informations"]
        );
        assert_eq!(PluginDrawerTab::tabs(false), &["Blocs", "Informations"]);

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
