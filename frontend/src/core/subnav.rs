//! Gabarit partagé de sous-navigation horizontale inspiré de Watchdeck (`AppSubnav`).
//!
//! Fournit une barre d'outils collante (`page-subnav`) accueillant une capsule d'onglets
//! accessible (`app-subnav`, WAI-ARIA `tablist` / `tab`), des informations de contexte
//! et d'éventuelles actions spécifiques.

use leptos::prelude::*;

/// Description d'un sous-onglet individuel.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SubNavTabItem {
    pub id: &'static str,
    pub label: &'static str,
    pub icon: Option<&'static str>,
    pub target_id: &'static str,
    pub is_active: bool,
    pub badge: Option<&'static str>,
}

#[component]
pub fn SubNav(
    aria_label: &'static str,
    tabs: Vec<SubNavTabItem>,
    #[prop(optional)] info: Option<AnyView>,
    #[prop(optional)] actions: Option<AnyView>,
) -> impl IntoView {
    view! {
        <nav class="page-subnav app-subnav-toolbar" aria-label=aria_label>
            <div class="subnav-container app-subnav-container">
                <div class="subnav-info">
                    {info}
                </div>

                <div class="app-subnav">
                    <div class="app-subnav__scroller subnav-tabs" role="tablist">
                        {tabs.into_iter().map(|item| {
                            let active_class = if item.is_active {
                                "app-subnav__item subnav-tab active"
                            } else {
                                "app-subnav__item subnav-tab"
                            };
                            let aria_selected = if item.is_active { "true" } else { "false" };
                            let tab_index = if item.is_active { "0" } else { "-1" };

                            view! {
                                <button
                                    type="button"
                                    role="tab"
                                    id=item.id
                                    class=active_class
                                    aria-selected=aria_selected
                                    aria-controls=item.target_id
                                    tabindex=tab_index
                                    data-subtab=item.target_id
                                >
                                    {item.icon.map(|ic| view! {
                                        <span class="app-subnav__icon subnav-tab-icon" aria-hidden="true">{ic}</span>
                                    })}
                                    <span class="app-subnav__label subnav-tab-label">{item.label}</span>
                                    {item.badge.map(|bg| view! {
                                        <small class="app-subnav__badge subnav-tab-badge">{bg}</small>
                                    })}
                                </button>
                            }
                        }).collect_view()}
                    </div>
                </div>

                <div class="subnav-actions">
                    {actions}
                </div>
            </div>
        </nav>
    }
}
