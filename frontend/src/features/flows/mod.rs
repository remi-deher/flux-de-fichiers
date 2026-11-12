//! Module de gestion et conception des flux.

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

use leptos::prelude::*;

#[component]
pub fn FlowsPanel() -> impl IntoView {
    view! {
        <div id=crate::core::panel::FLOWS.dom_id class="tab-panel" role="tabpanel" aria-labelledby=crate::core::panel::FLOWS.button_id hidden>
            <div id="flows-library-view">
                <FlowCatalog />
            </div>
            <FlowWorkspace />
            <FlowWizardDialog />
        </div>
    }
}
