//! Shell principal de l'application : Marque, navigation latérale et en-tête supérieur moderne.

use super::navigation::Navigation;
use crate::shared::ui::{Icon, IconName};
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
                        <span class="brand-version">"v0.12.0"</span>
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
                    <Icon name=IconName::ChevronLeft size=16 class="collapse-icon" />
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
                        <Icon name=IconName::Moon size=15 class="theme-toggle-icon" />
                        <span id="theme-toggle-label">"Thème"</span>
                    </button>
                    <button
                        type="button"
                        id="refresh-all-btn"
                        class="btn btn-secondary btn-sm"
                        aria-label="Actualiser les données du coordinateur"
                        title="Actualiser les données du coordinateur"
                    >
                        <Icon name=IconName::Refresh size=15 />
                        <span>"Actualiser"</span>
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
            // Fil d'Ariane épuré
            <div class="header-breadcrumb">
                <span class="breadcrumb-item">"Central"</span>
                <span class="breadcrumb-separator" aria-hidden="true">"/"</span>
                <span class="breadcrumb-title" id="current-section-title">"Poste de pilotage"</span>
            </div>

            // Barre de recherche rapide (Palette de commande moderne)
            <div class="header-search-wrapper">
                <button
                    type="button"
                    class="header-search-btn"
                    id="header-search-trigger"
                    title="Rechercher un média ou filtrer les tâches (Ctrl + K)"
                >
                    <Icon name=IconName::Search size=15 class="search-icon" />
                    <span class="search-placeholder">"Rechercher un média, une tâche… "</span>
                    <kbd class="search-kbd">"⌘K"</kbd>
                </button>
            </div>

            // Statuts de santé et actions d'en-tête rapides
            <div class="header-actions">
                <div id="health-status-pill" class="status-pill warning" aria-live="polite" title="Disponibilité de la sonde /health">
                    <span class="status-dot"></span>
                    <span id="health-status-text">"Vérification…"</span>
                </div>
                <div id="session-status-pill" class="status-pill warning" aria-live="polite" title="Statut d'authentification de la session">
                    <Icon name=IconName::Lock size=13 />
                    <span id="session-status-text">"Non authentifié"</span>
                </div>

                <button
                    type="button"
                    id="header-refresh-btn"
                    class="header-icon-btn"
                    aria-label="Actualiser les données"
                    title="Actualiser les données du coordinateur"
                >
                    <Icon name=IconName::Refresh size=16 />
                </button>
                <button
                    type="button"
                    id="header-theme-toggle-btn"
                    class="header-icon-btn"
                    aria-label="Basculer le thème clair ou sombre"
                    title="Basculer le thème"
                >
                    <Icon name=IconName::Moon size=16 />
                </button>
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
