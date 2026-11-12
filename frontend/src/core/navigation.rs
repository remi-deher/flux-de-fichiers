//! Définition des rôles métier et de la navigation latérale.

use crate::shared::ui::{Icon, IconName};
use leptos::prelude::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BusinessGroup {
    Piloter,
    Medias,
    Traitements,
    Infrastructure,
    Administration,
}

pub type BusinessRole = BusinessGroup;

/// Composant atomique d'un onglet de la barre latérale.
#[component]
fn SidebarTabButton(
    id: &'static str,
    target_panel: &'static str,
    icon: IconName,
    label: &'static str,
    title: &'static str,
    #[prop(optional)] is_active: bool,
) -> impl IntoView {
    let aria_selected = if is_active { "true" } else { "false" };
    let tab_index = if is_active { 0 } else { -1 };

    view! {
        <button
            type="button"
            role="tab"
            id=id
            class="tab-btn"
            aria-selected=aria_selected
            aria-controls=target_panel
            tabindex=tab_index
            title=title
        >
            <Icon name=icon size=18 class="tab-role-icon" />
            <span class="tab-btn-label">{label}</span>
        </button>
    }
}

#[component]
pub fn Navigation() -> impl IntoView {
    view! {
        <nav class="tab-navigation" aria-label="Navigation principale par groupes métier">
            <div class="tab-list" role="tablist">
                // Groupe 1 : Vue générale
                <span class="nav-group-label" aria-hidden="true">"Vue générale"</span>
                <SidebarTabButton
                    id="tab-btn-dashboard"
                    target_panel="panel-dashboard"
                    icon=IconName::Dashboard
                    label="Vue d'ensemble"
                    title="Groupe Vue d'ensemble : Statistiques et poste d'accueil"
                    is_active=true
                />

                // Groupe 2 : Médias & Bibliothèques
                <span class="nav-group-label" aria-hidden="true">"Médias"</span>
                <SidebarTabButton
                    id="tab-btn-files"
                    target_panel="panel-files"
                    icon=IconName::Libraries
                    label="Bibliothèques"
                    title="Groupe Médias : Bibliothèques surveillées et fichiers sources"
                />

                // Groupe 3 : Traitements & Pipelines
                <span class="nav-group-label" aria-hidden="true">"Traitements"</span>
                <SidebarTabButton
                    id="tab-btn-flows"
                    target_panel="panel-flows"
                    icon=IconName::Flows
                    label="Flows"
                    title="Groupe Traitements : Espace de conception des pipelines de traitement"
                />
                <SidebarTabButton
                    id="tab-btn-jobs"
                    target_panel="panel-jobs"
                    icon=IconName::Activity
                    label="Activité"
                    title="Groupe Traitements : Suivi en direct et analyse des exécutions"
                />

                // Groupe 4 : Infrastructure & Agents
                <span class="nav-group-label" aria-hidden="true">"Infrastructure"</span>
                <SidebarTabButton
                    id="tab-btn-workers"
                    target_panel="panel-workers"
                    icon=IconName::Workers
                    label="Nœuds"
                    title="Groupe Infrastructure : État des agents de traitement et workers"
                />
                <SidebarTabButton
                    id="tab-btn-plugins"
                    target_panel="panel-plugins"
                    icon=IconName::Plugins
                    label="Plugins"
                    title="Groupe Infrastructure : Outils média MKVToolNix, FFmpeg et extensions"
                />

                // Groupe 5 : Administration & Système
                <span class="nav-group-label" aria-hidden="true">"Système"</span>
                <SidebarTabButton
                    id="tab-btn-settings"
                    target_panel="panel-settings"
                    icon=IconName::Settings
                    label="Paramètres"
                    title="Groupe Administration : Réglages du central, jetons et sécurité"
                />
                <SidebarTabButton
                    id="tab-btn-updates"
                    target_panel="panel-updates"
                    icon=IconName::Updates
                    label="Mises à jour"
                    title="Groupe Administration : Suivi des versions, conteneur et changelog"
                />
            </div>
        </nav>
    }
}
