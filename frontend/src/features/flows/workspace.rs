//! Espace de conception interactif (Canvas, Palette, Inspecteur, Simulation).

use super::{
    canvas::FlowCanvas, inspector::FlowInspector, palette::FlowPalette,
    simulation::FlowSimulationBar,
};
use leptos::prelude::*;

#[component]
pub fn FlowWorkspace() -> impl IntoView {
    view! {
        <div id="flows-canvas-view" style="display: none;">
            <div class="canvas-header-bar">
                <div class="canvas-header-bar__identity">
                    <h2 id="canvas-flow-title" class="section-title canvas-header-bar__title">
                        "Éditeur visuel"
                    </h2>
                    <span class="status-pill success">"Mode Édition"</span>
                </div>
                <FlowSimulationBar />
            </div>

            <div class="canvas-workspace-layout">
                <FlowPalette />
                <FlowCanvas />
                <FlowInspector />
            </div>
        </div>
    }
}
