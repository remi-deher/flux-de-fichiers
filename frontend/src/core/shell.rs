//! Shell principal de l'application : Marque, navigation latérale et en-tête supérieur.

use super::navigation::Navigation;
use leptos::prelude::*;

#[component]
pub fn Sidebar() -> impl IntoView {
    view! {
        <aside class="application-sidebar" aria-label="Navigation principale">
            <div class="sidebar-header">
                <a href="#main-content" class="brand" title="Flux de Fichiers">
                    <span class="brand-mark">"MKV"</span>
                    <div class="brand-info">
                        <span class="brand-name">"Flux de Fichiers"</span>
                        <span class="brand-version">"v0.6"</span>
                    </div>
                </a>
            </div>

            <Navigation />

            <div class="sidebar-footer">
                <button
                    type="button"
                    id="sidebar-collapse-btn"
                    class="sidebar-collapse-btn"
                    aria-label="Replier la navigation"
                    title="Replier ou déplier la barre latérale"
                >
                    <span class="collapse-icon" aria-hidden="true">"◀"</span>
                    <span class="collapse-label">"Replier"</span>
                </button>
                <div class="sidebar-footer-tools">
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
                        title="Actualiser les données du coordinateur"
                    >
                        "Actualiser"
                    </button>
                </div>
            </div>
        </aside>
    }
}

#[component]
pub fn Header() -> impl IntoView {
    view! {
        <header class="application-header" role="banner">
            <div class="header-breadcrumb">
                <span class="breadcrumb-item">"Central"</span>
                <span class="breadcrumb-separator" aria-hidden="true">" / "</span>
                <span class="breadcrumb-title" id="current-section-title">"Poste de pilotage"</span>
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
            </div>
        </header>
    }
}

#[component]
pub fn AppShell() -> impl IntoView {
    view! {
        <Sidebar />
        <Header />
    }
}
