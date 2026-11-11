//! Shell principal de l'application : Marque, indicateurs d'état et navigation par rôles.

use super::navigation::Navigation;
use leptos::prelude::*;

#[component]
pub fn AppShell() -> impl IntoView {
    view! {
        <header class="app-header" role="banner">
            <div class="brand-wrapper">
                <span class="brand-badge">"MKV"</span>
                <div>
                    <h1 class="app-title">"Flux de Fichiers"</h1>
                    <p class="section-subtitle">"Console de pilotage et supervision de traitement"</p>
                </div>
            </div>
            <div class="header-status">
                <div id="health-status-pill" class="status-pill warning" aria-live="polite">
                    <span class="status-dot"></span>
                    <span id="health-status-text">"Vérification…"</span>
                </div>
                <div id="session-status-pill" class="status-pill warning" aria-live="polite">
                    <span class="status-dot"></span>
                    <span id="session-status-text">"Non authentifié"</span>
                </div>
                <button
                    type="button"
                    id="theme-toggle-btn"
                    class="btn btn-secondary btn-sm"
                    aria-label="Basculer le thème clair ou sombre"
                    title="Basculer le thème clair ou sombre"
                >
                    <span id="theme-toggle-icon">"🌓"</span>
                    <span id="theme-toggle-label">"Thème"</span>
                </button>
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

        <Navigation />
    }
}
