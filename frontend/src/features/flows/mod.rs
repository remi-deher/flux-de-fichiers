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

use leptos::prelude::*;

#[component]
pub fn FlowsPanel() -> impl IntoView {
    view! {
        <div id="panel-flows" class="tab-panel" role="tabpanel" aria-labelledby="tab-btn-flows" hidden>
            <FlowCatalog />
            <FlowWorkspace />
            <FlowWizardDialog />
        </div>
    }
}
