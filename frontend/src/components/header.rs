//! Composant d'en-tête de navigation, statut global et onglets.

use leptos::prelude::*;

#[component]
pub fn Header() -> impl IntoView {
    view! {
        <header class="app-header" role="banner">
            <div class="brand-wrapper">
                <span class="brand-badge">"MKV"</span>
                <div>
                    <h1 class="app-title">"Flux de Fichiers"</h1>
                    <p class="section-subtitle">"Poste de pilotage et supervision de remuxage"</p>
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

        <nav class="tab-navigation" aria-label="Sections du poste de pilotage">
            <div class="tab-list" role="tablist">
                <button
                    type="button"
                    role="tab"
                    id="tab-btn-dashboard"
                    class="tab-btn"
                    aria-selected="true"
                    aria-controls="panel-dashboard"
                    tabindex="0"
                >
                    "Vue d'ensemble"
                </button>
                <button
                    type="button"
                    role="tab"
                    id="tab-btn-files"
                    class="tab-btn"
                    aria-selected="false"
                    aria-controls="panel-files"
                    tabindex="-1"
                >
                    "Fichiers sources"
                </button>
                <button
                    type="button"
                    role="tab"
                    id="tab-btn-jobs"
                    class="tab-btn"
                    aria-selected="false"
                    aria-controls="panel-jobs"
                    tabindex="-1"
                >
                    "Tâches de remuxage"
                </button>
                <button
                    type="button"
                    role="tab"
                    id="tab-btn-workers"
                    class="tab-btn"
                    aria-selected="false"
                    aria-controls="panel-workers"
                    tabindex="-1"
                >
                    "Nœuds & Serveurs"
                </button>
                <button
                    type="button"
                    role="tab"
                    id="tab-btn-plugins"
                    class="tab-btn"
                    aria-selected="false"
                    aria-controls="panel-plugins"
                    tabindex="-1"
                >
                    "Plugins & Outils"
                </button>
                <button
                    type="button"
                    role="tab"
                    id="tab-btn-settings"
                    class="tab-btn"
                    aria-selected="false"
                    aria-controls="panel-settings"
                    tabindex="-1"
                >
                    "Paramètres & Session"
                </button>
            </div>
        </nav>
    }
}
