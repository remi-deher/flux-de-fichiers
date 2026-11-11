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
                <div style="display: flex; align-items: center; gap: 0.75rem;">
                    <button
                        type="button"
                        id="btn-back-to-library"
                        class="btn btn-secondary btn-sm"
                        aria-label="Retourner à la liste des flux"
                    >
                        "← Retour aux flux"
                    </button>
                    <h2 id="canvas-flow-title" class="section-title" style="margin: 0; font-size: 1.25rem;">
                        "Éditeur visuel : Transcodage HEVC"
                    </h2>
                    <span class="status-pill success">"Mode Édition"</span>
                </div>
                <FlowSimulationBar />
                <div style="display: flex; gap: 0.5rem;">
                    <button
                        type="button"
                        id="btn-canvas-save-flow"
                        class="btn btn-primary btn-sm"
                    >
                        "💾 Enregistrer les modifications"
                    </button>
                </div>
            </div>

            <div class="canvas-workspace-layout">
                <FlowPalette />
                <FlowCanvas />
                <FlowInspector />
            </div>
        </div>
    }
}
