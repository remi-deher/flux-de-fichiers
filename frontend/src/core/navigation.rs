//! Définition des rôles métier et de la navigation latérale.

use super::panel::NAV_GROUPS;
use crate::shared::ui::{Icon, IconName};
use leptos::prelude::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BusinessGroup {
    Piloter,
    Medias,
    Traitements,
    Infrastructure,
    Administration,
}

pub type BusinessRole = BusinessGroup;

/// Composant atomique d'un onglet de la barre latérale.
#[component]
fn SidebarTabButton(
    id: &'static str,
    target_panel: &'static str,
    icon: IconName,
    label: &'static str,
    title: &'static str,
    #[prop(optional)] is_active: bool,
) -> impl IntoView {
    let aria_selected = if is_active { "true" } else { "false" };
    let tab_index = if is_active { 0 } else { -1 };

    view! {
        <button
            type="button"
            role="tab"
            id=id
            class="tab-btn"
            aria-selected=aria_selected
            aria-controls=target_panel
            tabindex=tab_index
            title=title
        >
            <Icon name=icon size=18 class="tab-role-icon" />
            <span class="tab-btn-label">{label}</span>
        </button>
    }
}

#[component]
pub fn Navigation() -> impl IntoView {
    view! {
        <nav class="tab-navigation" aria-label="Navigation principale par groupes métier">
            <div class="tab-list" role="tablist">
                {NAV_GROUPS.iter().map(|group| view! {
                    <span class="nav-group-label" aria-hidden="true">{group.label}</span>
                    {group.panels.iter().map(|panel| view! {
                        <SidebarTabButton
                            id=panel.button_id
                            target_panel=panel.dom_id
                            icon=panel.icon
                            label=panel.label
                            title=panel.nav_title
                            is_active=panel.active
                        />
                    }).collect_view()}
                }).collect_view()}
            </div>
        </nav>
    }
}
