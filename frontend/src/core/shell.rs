//! Shell principal de l'application : Marque, navigation latérale et en-tête supérieur moderne.

use super::capsule::CapsuleActions;
use super::navigation::Navigation;
use crate::shared::ui::{Button, ButtonVariant, FormField, Icon, IconName, PASSWORD_MIN_LENGTH};
use leptos::prelude::*;

/// Thème de couleur : la valeur est celle conservée par le navigateur et lue par le script client.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThemeChoice {
    System,
    Light,
    Dark,
}

impl ThemeChoice {
    pub const ALL: [Self; 3] = [Self::System, Self::Light, Self::Dark];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Light => "light",
            Self::Dark => "dark",
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::System => "Auto",
            Self::Light => "Clair",
            Self::Dark => "Sombre",
        }
    }
}

/// Sélecteur de thème : trois boutons exclusifs, le script client en marque un `aria-pressed`.
#[component]
fn ThemeSegmented() -> impl IntoView {
    view! {
        <div id="theme-segmented" class="theme-segmented" role="group" aria-label="Thème de couleur">
            {ThemeChoice::ALL
                .into_iter()
                .map(|choice| view! {
                    <button
                        type="button"
                        class="theme-segmented__btn"
                        data-theme-choice=choice.as_str()
                        aria-pressed="false"
                    >
                        {choice.label()}
                    </button>
                })
                .collect_view()}
        </div>
    }
}

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
                    <ThemeSegmented />
                    <button
                        type="button"
                        id="sidebar-logout-btn"
                        class="sidebar-logout-btn"
                        aria-label="Se déconnecter"
                        title="Se déconnecter"
                    >
                        <Icon name=IconName::Logout size=16 />
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
                            <CapsuleActions />
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

#[cfg(test)]
mod tests {
    use super::*;

    fn sidebar() -> String {
        view! { <Sidebar /> }.to_html()
    }

    #[test]
    fn test_theme_choices_are_the_three_names_the_script_stores() {
        let names: Vec<&str> = ThemeChoice::ALL.iter().map(|c| c.as_str()).collect();
        assert_eq!(names, ["system", "light", "dark"]);
        let html = sidebar();
        for choice in ThemeChoice::ALL {
            assert!(html.contains(&format!("data-theme-choice=\"{}\"", choice.as_str())));
            assert!(html.contains(choice.label()));
        }
        assert!(!html.contains("theme-toggle-btn"));
    }

    #[test]
    fn test_sidebar_has_a_discreet_logout_and_no_account_menu() {
        let html = sidebar();
        assert!(html.contains("id=\"sidebar-logout-btn\""));
        assert!(html.contains("aria-label=\"Se déconnecter\""));
        for gone in [
            "account-menu",
            "account-chip",
            "account-password-form",
            "account-avatar-text",
        ] {
            assert!(!html.contains(gone), "{gone}");
        }
        assert!(!html.contains("style="));
    }
}
