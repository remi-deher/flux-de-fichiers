//! Tiroir d'inspection à onglets d'un plugin : blocs exposés, paramètres et informations.

use super::settings::PluginSettings;
use crate::api;
use crate::route::encode_segment;
use crate::ui::{Tag, TagTone};
use flux_shared::flows::{OwnedBlockSpec, OwnedFieldKind, OwnedFieldSpec};
use flux_shared::plugins::PluginDetailView;
use leptos::prelude::*;

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

fn field_kind_label(kind: &OwnedFieldKind) -> &'static str {
    match kind {
        OwnedFieldKind::Text => "Texte",
        OwnedFieldKind::Bool => "Booléen",
        OwnedFieldKind::Select(_) => "Sélection",
        OwnedFieldKind::Headers => "En-têtes",
    }
}

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
    view! {
        <ul class="step-list">
            {blocks.into_iter().map(|block| {
                let kind = block.kind.clone();
                let label = block.label.clone();
                let description = block.description.clone();
                let category = block.category.clone();
                let domain = block.domain.clone();
                let has_input = block.has_input;
                let terminal = block.terminal;
                let ports = block.ports.clone();
                let operation_id = block.operation_id.clone();
                let aliases = block.aliases.clone();
                let fields = block.fields.clone();
                view! {
                    <li class="step-item">
                        <div class="step-item__head">
                            <span class="text-strong">{label}</span>
                            <Tag label=kind tone=TagTone::Mono />
                            <Tag label=category tone=TagTone::Primary />
                            {domain.map(|d| view! { <Tag label=d tone=TagTone::Neutral /> })}
                            {(!has_input).then(|| view! { <Tag label="Point d'entrée" tone=TagTone::Success /> })}
                            {terminal.then(|| view! { <Tag label="Terminal" tone=TagTone::Warning /> })}
                        </div>
                        {(!description.is_empty()).then(|| view! {
                            <p class="step-item__msg text-sm">{description}</p>
                        })}
                        {(!ports.is_empty() || operation_id.is_some() || !aliases.is_empty()).then(|| view! {
                            <div class="step-item__meta">
                                {ports.into_iter().map(|(id, port_label)| view! {
                                    <Tag label=format!("Sortie : {port_label} ({id})") tone=TagTone::Neutral />
                                }).collect_view()}
                                {operation_id.map(|op| view! {
                                    <Tag label=format!("Op : {op}") tone=TagTone::Mono />
                                })}
                                {aliases.into_iter().map(|alias| view! {
                                    <Tag label=format!("Alias : {alias}") tone=TagTone::Mono />
                                }).collect_view()}
                            </div>
                        })}
                        {(!fields.is_empty()).then(|| view! {
                            <div class="step-item__meta">
                                {fields.iter().map(|field| view! {
                                    <Tag label=format_field_chip(field) tone=TagTone::Neutral />
                                }).collect_view()}
                            </div>
                        })}
                    </li>
                }
            }).collect_view()}
        </ul>
    }
    .into_any()
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
            <div class="detail-term">"Identifiant"</div>
            <div class="detail-val text-mono">{detail.id}</div>
            <div class="detail-term">"Version"</div>
            <div class="detail-val text-mono">{detail.version}</div>
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
                        PluginDrawerTab::Info => view! { <InfoTab detail=data /> }.into_any(),
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
