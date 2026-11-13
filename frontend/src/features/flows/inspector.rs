//! Volet latéral droit : Inspecteur et configuration du nœud sélectionné (Rôle Construire).

use leptos::prelude::*;

#[component]
pub fn FlowInspector() -> impl IntoView {
    view! {
        <aside class="canvas-inspector-sidebar" id="canvas-inspector" aria-label="Paramètres du bloc sélectionné">
            <div class="inspector-header">
                <h3 id="inspector-block-title" class="inspector-title">"Paramètres du bloc"</h3>
                <span id="inspector-block-tool" class="status-pill info">"—"</span>
            </div>

            // Nom et réglages du bloc sélectionné, injectés par le script.
            <div id="inspector-form-container">
                <p class="form-help">"Sélectionnez un bloc pour voir ses réglages."</p>
            </div>
        </aside>
    }
}
