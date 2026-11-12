//! Shell principal de l'application : Marque, navigation latérale et en-tête supérieur moderne.

use super::navigation::Navigation;
use super::panel::active_panel;
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
                </div>
            </div>
        </aside>
    }
}

#[component]
pub fn Header() -> impl IntoView {
    view! {
        <header class="app-topbar" id="app-topbar" role="banner">
            <div class="topbar-container">
                // Contexte de page & Fil d'Ariane
                <div class="topbar-context">
                    <span class="breadcrumb-item">"Central"</span>
                    <span class="breadcrumb-separator" aria-hidden="true">"/"</span>
                    <span class="breadcrumb-title" id="current-section-title">{active_panel().label}</span>
                </div>

                // Capsule unique : recherche, actions de la page, puis filtres.
                // La feuille sort du champ, comme la BarSheet de Watchdeck.
                <div class="topbar-search-anchor" id="search-anchor">
                    <div class="topbar-search" id="search-capsule" role="search">
                        <div class="search-capsule__query">
                            <Icon name=IconName::Search size=15 class="search-icon" />
                            <input
                                type="search"
                                id="global-search-input"
                                class="topbar-search-input"
                                placeholder="Rechercher…"
                                autocomplete="off"
                                spellcheck="false"
                                enterkeyhint="search"
                                aria-label="Recherche contextuelle"
                            />
                            <span id="search-match-count" class="search-match-count" aria-live="polite"></span>
                        </div>
                        <div class="search-capsule__tools">
                            <div class="search-capsule__actions" id="search-capsule-actions">
                                <button
                                    type="button"
                                    id="btn-open-flow-wizard"
                                    class="search-capsule__action"
                                    data-capsule-tab="flows"
                                    hidden
                                >
                                    <Icon name=IconName::Plus size=14 />
                                    "Nouveau flux"
                                </button>
                                <button
                                    type="button"
                                    id="btn-open-add-repo-dialog"
                                    class="search-capsule__action"
                                    data-capsule-tab="plugins"
                                    aria-label="Ajouter un nouveau dépôt de plugins"
                                    hidden
                                >
                                    <Icon name=IconName::Plus size=14 />
                                    "Ajouter un dépôt"
                                </button>
                            </div>
                            <button
                                type="button"
                                id="search-filter-btn"
                                class="search-capsule__filter"
                                aria-expanded="false"
                                aria-controls="filter-sheet"
                                hidden
                            >
                                <Icon name=IconName::Filters size=14 />
                                <span>"Filtres"</span>
                                <strong id="search-filter-count" class="search-capsule__count" hidden>"0"</strong>
                            </button>
                        </div>
                    </div>
                    <div id="filter-sheet" class="filter-sheet" role="dialog" aria-label="Filtres" hidden>
                        <div class="filter-sheet__head">
                            <span class="filter-sheet__title">"Filtres"</span>
                            <button type="button" id="filter-sheet-clear" class="filter-sheet__text-btn">"Tout effacer"</button>
                            <button type="button" id="filter-sheet-close" class="topbar-search-clear" aria-label="Fermer les filtres">
                                <Icon name=IconName::Close size=14 />
                            </button>
                        </div>
                        <div data-filter-page="jobs" hidden>
                            <p class="filter-sheet__label">"État des tâches"</p>
                            <div class="filter-group" role="group" aria-label="Filtrer les tâches actives">
                                <button type="button" class="filter-chip" data-filter="all" aria-pressed="true">
                                    "Tous" <span id="count-filter-all">"(0)"</span>
                                </button>
                                <button type="button" class="filter-chip" data-filter="queued" aria-pressed="false">
                                    "En attente" <span id="count-filter-queued">"(0)"</span>
                                </button>
                                <button type="button" class="filter-chip" data-filter="running" aria-pressed="false">
                                    "En cours" <span id="count-filter-running">"(0)"</span>
                                </button>
                                <button type="button" class="filter-chip" data-filter="succeeded" aria-pressed="false">
                                    "Succès" <span id="count-filter-succeeded">"(0)"</span>
                                </button>
                                <button type="button" class="filter-chip" data-filter="failed" aria-pressed="false">
                                    "Échecs" <span id="count-filter-failed">"(0)"</span>
                                </button>
                            </div>
                        </div>
                    </div>
                </div>

                // Statut opérationnel live (central inclus)
                <div class="topbar-actions">
                    <div id="header-workers-pill" class="status-pill online" aria-live="polite" title="Nombre de workers actifs (central inclus)">
                        <span class="status-dot"></span>
                        <span id="header-workers-count-text">"1 worker en ligne"</span>
                    </div>
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
