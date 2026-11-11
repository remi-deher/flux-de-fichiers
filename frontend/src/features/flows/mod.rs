//! Module de gestion et conception des flux (Rôle Construire).

pub mod canvas;
pub mod catalog;
pub mod inspector;
pub mod palette;
pub mod simulation;
pub mod wizard;
pub mod workspace;

pub use canvas::FlowCanvas;
pub use catalog::FlowCatalog;
pub use inspector::FlowInspector;
pub use palette::FlowPalette;
pub use simulation::FlowSimulationBar;
pub use wizard::FlowWizardDialog;
pub use workspace::FlowWorkspace;

use crate::core::{SubNav, SubNavTabItem};
use leptos::prelude::*;

#[component]
pub fn FlowsPanel() -> impl IntoView {
    let subnav_tabs = vec![
        SubNavTabItem {
            id: "btn-subtab-flows-installed",
            label: "Mes flux",
            icon: Some("⚡"),
            target_id: "subtab-flows-installed",
            is_active: true,
            badge: None,
        },
        SubNavTabItem {
            id: "btn-subtab-flows-canvas",
            label: "Éditeur visuel",
            icon: Some("🎨"),
            target_id: "subtab-flows-canvas",
            is_active: false,
            badge: None,
        },
    ];

    let subnav_actions = view! {
        <button
            type="button"
            id="btn-subnav-open-wizard"
            class="btn btn-primary btn-sm"
            aria-label="Ouvrir l'assistant de conception de flux"
        >
            "🪄 Assistant de création"
        </button>
    }
    .into_any();

    view! {
        <div id="panel-flows" class="tab-panel" role="tabpanel" aria-labelledby="tab-btn-flows" hidden>
            <SubNav
                aria_label="Sous-navigation de la section Concepteur de flux"
                tabs=subnav_tabs
                actions=subnav_actions
            />

            // Sous-vue 1 : Mes flux (Catalogue local Bare Core)
            <div id="subtab-flows-installed">
                <FlowCatalog />
            </div>

            // Sous-vue 2 : Éditeur visuel (Canvas & Workspace)
            <div id="subtab-flows-canvas" hidden>
                <FlowWorkspace />
            </div>

            <FlowWizardDialog />
        </div>
    }
}
