//! Composant d'en-tête de navigation et statut global.

use leptos::prelude::*;

#[component]
pub fn Header() -> impl IntoView {
    view! {
        <header class="app-header" role="banner">
            <div class="brand-wrapper">
                <span class="brand-badge">"MKV"</span>
                <div>
                    <h1 class="app-title">"Flux de Fichiers"</h1>
                    <p class="section-subtitle">"Console de pilotage et supervision de remuxage"</p>
                </div>
            </div>
            <div class="header-status">
                <div id="health-status-pill" class="status-pill warning" aria-live="polite">
                    <span class="status-dot"></span>
                    <span id="health-status-text">"Vérification…"</span>
                </div>
                <button
                    type="button"
                    id="refresh-all-btn"
                    class="btn btn-secondary btn-sm"
                    aria-label="Actualiser les données du coordinateur"
                >
                    "Actualiser"
                </button>
            </div>
        </header>
    }
}
