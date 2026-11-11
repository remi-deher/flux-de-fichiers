//! Barre d'outils de simulation interactive et banc de test (Rôle Construire).

use leptos::prelude::*;

#[component]
pub fn FlowSimulationBar() -> impl IntoView {
    view! {
        <div class="canvas-simulation-bar" role="toolbar" aria-label="Banc de test de simulation conditionnelle">
            <span class="sim-label">"Simuler le routage :"</span>
            <button
                type="button"
                class="btn btn-secondary btn-sm sim-btn"
                data-sim="true"
                title="Condition vérifiée (ex: HEVC) : routage direct In-Place sans copie"
            >
                "✓ Vrai (In-Place)"
            </button>
            <button
                type="button"
                class="btn btn-secondary btn-sm sim-btn"
                data-sim="false"
                title="Condition non vérifiée : transcodage GPU"
            >
                "✗ Faux (Transcodage)"
            </button>
            <button
                type="button"
                class="btn btn-secondary btn-sm sim-btn"
                data-sim="fallback"
                title="Échec transcodeur matériel : bascule automatique sur encodeur logiciel"
            >
                "⚡ Repli (CPU)"
            </button>
            <button
                type="button"
                class="btn btn-secondary btn-sm sim-btn"
                data-sim="reset"
            >
                "Réinitialiser"
            </button>
        </div>
    }
}
