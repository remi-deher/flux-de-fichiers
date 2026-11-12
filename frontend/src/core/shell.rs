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

            // Barre de recherche contextuelle moderne (Command & Context Filter)
            <div class="header-search-wrapper" role="search">
                <div class="header-search-box">
                    <Icon name=IconName::Search size=15 class="search-icon" />
                    <input
                        type="search"
                        id="global-search-input"
                        class="header-search-input"
                        placeholder="Rechercher…"
                        autocomplete="off"
                        spellcheck="false"
                        aria-label="Recherche contextuelle"
                    />
                    <button
                        type="button"
                        id="header-search-clear-btn"
                        class="header-search-clear-btn"
                        aria-label="Effacer la recherche"
                        title="Effacer la recherche (Échap)"
                        hidden
                    >
                        <Icon name=IconName::Close size=14 />
                    </button>
                    <kbd class="search-kbd" id="search-kbd-hint">"⌘K"</kbd>
                </div>
            </div>

            // Actions d'en-tête : statut des workers (central inclus) et actualisation rapide
            <div class="header-actions">
                <div id="header-workers-pill" class="status-pill online" aria-live="polite" title="Nombre de workers actifs (central inclus)">
                    <span class="status-dot"></span>
                    <span id="header-workers-count-text">"1 worker en ligne"</span>
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
