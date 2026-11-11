//! Gabarit partagé de sous-navigation horizontale (inspiré de l'ADR 0020).
//!
//! Fournit une barre d'onglets de second niveau accessible (WAI-ARIA `tablist` / `tab`)
//! avec support optionnel d'actions contextuelles à droite.

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
        <nav class="page-subnav" aria-label=aria_label>
            <div class="subnav-container">
                <div class="subnav-info">
                    {info}
                </div>

                <div class="subnav-tabs" role="tablist">
                    {tabs.into_iter().map(|item| {
                        let active_class = if item.is_active { "subnav-tab active" } else { "subnav-tab" };
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
                                    <span class="subnav-tab-icon" aria-hidden="true">{ic}</span>
                                })}
                                <span class="subnav-tab-label">{item.label}</span>
                                {item.badge.map(|bg| view! {
                                    <span class="subnav-tab-badge">{bg}</span>
                                })}
                            </button>
                        }
                    }).collect_view()}
                </div>

                <div class="subnav-actions">
                    {actions}
                </div>
            </div>
        </nav>
    }
}
