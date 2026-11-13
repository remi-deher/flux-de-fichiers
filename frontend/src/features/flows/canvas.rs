//! Zone centrale : plan de travail graphique (liaisons SVG et blocs du flux ouvert).

use leptos::prelude::*;

#[component]
pub fn FlowCanvas() -> impl IntoView {
    view! {
        <div class="canvas-board-wrapper" id="canvas-board-area" tabindex="0" role="region" aria-label="Plan de travail graphique du flux">
            <svg class="canvas-connections-svg" id="canvas-svg-lines" aria-hidden="true">
                // Tracés SVG de liaison générés dynamiquement par le script flows.js
            </svg>

            // Blocs rendus par le script depuis la définition du flux ouvert.
            <div class="canvas-nodes-container" id="canvas-nodes-container"></div>
        </div>
    }
}
