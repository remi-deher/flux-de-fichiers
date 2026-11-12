//! Gestion des bibliothèques de médias.

pub mod file_browser;
pub mod library_card;
pub mod library_resources;
pub mod node_access;

use crate::core::{SubNav, SubNavTabItem};
use file_browser::FileBrowser;
use leptos::prelude::*;

#[component]
pub fn LibrariesPanel() -> impl IntoView {
    let subnav_tabs = vec![SubNavTabItem {
        id: "btn-subtab-files-browser",
        label: "Principale",
        icon: None,
        target_id: "subtab-files-browser",
        is_active: true,
        badge: None,
    }];

    let subnav_info = view! {
        <span class="subnav-info-badge">"Volume /input"</span>
    }
    .into_any();

    view! {
        <div
            id=crate::core::panel::FILES.dom_id
            class="tab-panel"
            role="tabpanel"
            aria-labelledby=crate::core::panel::FILES.button_id
            hidden
        >
            <SubNav
                aria_label="Bibliothèques"
                tabs=subnav_tabs
                info=subnav_info
            />
            <div id="subtab-files-browser">
                <FileBrowser />
            </div>
        </div>
    }
}
