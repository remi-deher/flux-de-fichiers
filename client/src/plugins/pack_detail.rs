//! Fiche d'inspection à onglets d'un pack : composants et informations ; il s'installe depuis le Store.

use super::flow_preview::FlowMiniPreview;
use super::metadata::{CommonInfoRows, MetaRow, StatusBanner, StoreTone};
use super::{CATALOG_TOPICS, SelectedItem, section_badge_label};
use crate::live::load;
use crate::route::encode_segment;
use crate::ui::{Icon, IconName, Tag, TagTone};
use forgearr_shared::plugins::vocab::CatalogItemType;
use forgearr_shared::plugins::{CatalogEntry, PackDetailView};
use leptos::prelude::*;

/// Onglets du tiroir d'un pack (composants et installation réunis dans Aperçu).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PackDrawerTab {
    Overview,
    Info,
}

impl PackDrawerTab {
    pub const TABS: &'static [&'static str] = &["Aperçu", "Informations"];

    pub const fn from_index(index: usize) -> Self {
        match index {
            0 => Self::Overview,
            _ => Self::Info,
        }
    }
}

#[component]
fn OverviewTab(
    detail: PackDetailView,
    on_open_plugin: Option<Callback<SelectedItem>>,
    on_open_tool: Option<Callback<SelectedItem>>,
    catalog_entries: Option<Signal<Vec<CatalogEntry>>>,
) -> impl IntoView {
    let spec = detail.installation;
    let components_count = spec.components.len();
    let runtime_label = match spec.runtime_implemented {
        Some(true) => "Installable depuis le Catalogue",
        Some(false) => "Non implémenté",
        None => "Pack déclaratif",
    };

    view! {
        <StatusBanner
            title=runtime_label
            text=Some(format!("{components_count} composant(s) inclus dans ce pack."))
            tone=StoreTone::Primary
        />
        {detail.description.map(|desc| view! {
            <p class="text-sm">{desc}</p>
        })}
        {spec.entrypoint.map(|entry| {
            let entry_id = entry.id.clone();
            let entry_preview = catalog_entries.and_then(|entries| {
                entries
                    .get()
                    .into_iter()
                    .find(|item| item.id == entry_id)
                    .and_then(|item| item.runtime)
                    .and_then(|rt| rt.flow_preview)
            });
            view! {
                <div class="detail-grid">
                    <MetaRow term="Point d'entrée">
                        <div class="cluster">
                            <Tag label=section_badge_label(entry.kind) tone=TagTone::Primary />
                            <span class="text-mono text-strong">{entry.id}</span>
                            <Tag label=entry.version tone=TagTone::Mono />
                        </div>
                    </MetaRow>
                </div>
                {entry_preview.map(|preview| view! {
                    <FlowMiniPreview preview=preview />
                })}
            }
        })}
        <div class="pack-components">
            <h3 class="form-label">"Composants inclus"</h3>
            {if spec.components.is_empty() {
                view! { <p class="text-dim text-sm">"Aucun composant déclaré."</p> }.into_any()
            } else {
                view! {
                    <ul class="step-list">
                        {spec.components.into_iter().map(|comp| {
                            let target_cb = match comp.kind {
                                CatalogItemType::Plugin => on_open_plugin,
                                CatalogItemType::App => on_open_tool,
                                _ => None,
                            };
                            let icon = match comp.kind {
                                CatalogItemType::App => IconName::Package,
                                _ => IconName::Plugins,
                            };
                            if let Some(cb) = target_cb {
                                let open_id = comp.id.clone();
                                let open_name = comp.id.clone();
                                view! {
                                    <li>
                                        <button
                                            type="button"
                                            class="dep-link"
                                            on:click=move |_| cb.run(SelectedItem {
                                                id: open_id.clone(),
                                                name: open_name.clone(),
                                            })
                                        >
                                            <span class="dep-link__main">
                                                <Icon name=icon size=15 />
                                                <span class="text-mono text-strong">{comp.id}</span>
                                            </span>
                                            <span class="dep-link__meta">
                                                <Tag label=section_badge_label(comp.kind) tone=TagTone::Primary />
                                                <Tag label=comp.version tone=TagTone::Mono />
                                            </span>
                                        </button>
                                    </li>
                                }
                                .into_any()
                            } else {
                                view! {
                                    <li class="step-item">
                                        <div class="step-item__head">
                                            <Tag label=section_badge_label(comp.kind) tone=TagTone::Primary />
                                            <span class="text-mono text-strong">{comp.id}</span>
                                            <Tag label=comp.version tone=TagTone::Mono />
                                        </div>
                                    </li>
                                }
                                .into_any()
                            }
                        }).collect_view()}
                    </ul>
                }.into_any()
            }}
        </div>
    }
}

#[component]
fn InfoTab(detail: PackDetailView) -> impl IntoView {
    view! {
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
            <MetaRow term="Format" mono=true>{detail.installation.kind}</MetaRow>
            {detail.category.filter(|cat| cat.shows_badge()).map(|cat| view! {
                <MetaRow term="Catégorie">
                    <Tag label=cat.as_str() tone=TagTone::Neutral />
                </MetaRow>
            })}
        </div>
    }
}

#[component]
pub fn PackDetail(
    pack_id: String,
    tab: RwSignal<usize>,
    #[prop(optional)] on_open_plugin: Option<Callback<SelectedItem>>,
    #[prop(optional)] on_open_tool: Option<Callback<SelectedItem>>,
    #[prop(optional)] catalog_entries: Option<Signal<Vec<CatalogEntry>>>,
) -> impl IntoView {
    let encoded = encode_segment(&pack_id);
    let spec_path = format!("/api/v1/packs/{encoded}");

    let detail = load::<PackDetailView>(CATALOG_TOPICS, spec_path);

    view! {
        <div class="inspection-panel">
            {move || {
                let active_tab = PackDrawerTab::from_index(tab.get());
                match detail.get() {
                    None => view! { <p class="text-dim">"Chargement du pack…"</p> }.into_any(),
                    Some(Err(error)) => view! { <p class="text-danger">{error.message()}</p> }.into_any(),
                    Some(Ok(data)) => match active_tab {
                        PackDrawerTab::Overview => view! {
                            <OverviewTab
                                detail=data
                                on_open_plugin=on_open_plugin
                                on_open_tool=on_open_tool
                                catalog_entries=catalog_entries
                            />
                        }
                        .into_any(),
                        PackDrawerTab::Info => view! { <InfoTab detail=data /> }.into_any(),
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
    fn pack_drawer_tabs_map_all_indices() {
        assert_eq!(PackDrawerTab::TABS, &["Aperçu", "Informations"]);
        assert_eq!(PackDrawerTab::from_index(0), PackDrawerTab::Overview);
        assert_eq!(PackDrawerTab::from_index(1), PackDrawerTab::Info);
        assert_eq!(PackDrawerTab::from_index(10), PackDrawerTab::Info);
    }
}
