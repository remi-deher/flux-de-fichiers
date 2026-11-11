//! Définition des rôles métier et de la navigation latérale.

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

#[component]
pub fn Navigation() -> impl IntoView {
    view! {
        <nav class="tab-navigation" aria-label="Navigation principale par groupes métier">
            <div class="tab-list" role="tablist">
                // Groupe 1 : Vue générale
                <span class="nav-group-label" aria-hidden="true">"Vue générale"</span>
                <button
                    type="button"
                    role="tab"
                    id="tab-btn-dashboard"
                    class="tab-btn"
                    aria-selected="true"
                    aria-controls="panel-dashboard"
                    tabindex="0"
                    title="Groupe Piloter : Vue d'ensemble et état de santé du système"
                >
                    <span class="tab-role-icon" aria-hidden="true">"📊"</span>
                    <span class="tab-btn-label">"Piloter"</span>
                </button>

                // Groupe 2 : Médias & Bibliothèques
                <span class="nav-group-label" aria-hidden="true">"Médias"</span>
                <button
                    type="button"
                    role="tab"
                    id="tab-btn-files"
                    class="tab-btn"
                    aria-selected="false"
                    aria-controls="panel-files"
                    tabindex="-1"
                    title="Groupe Médias : Bibliothèques surveillées et fichiers sources"
                >
                    <span class="tab-role-icon" aria-hidden="true">"📁"</span>
                    <span class="tab-btn-label">"Bibliothèques"</span>
                </button>

                // Groupe 3 : Traitements & Pipelines
                <span class="nav-group-label" aria-hidden="true">"Traitements"</span>
                <button
                    type="button"
                    role="tab"
                    id="tab-btn-flows"
                    class="tab-btn"
                    aria-selected="false"
                    aria-controls="panel-flows"
                    tabindex="-1"
                    title="Groupe Traitements : Espace de conception des pipelines de traitement"
                >
                    <span class="tab-role-icon" aria-hidden="true">"⚡"</span>
                    <span class="tab-btn-label">"Flows"</span>
                </button>
                <button
                    type="button"
                    role="tab"
                    id="tab-btn-jobs"
                    class="tab-btn"
                    aria-selected="false"
                    aria-controls="panel-jobs"
                    tabindex="-1"
                    title="Groupe Traitements : Suivi en direct et analyse des exécutions"
                >
                    <span class="tab-role-icon" aria-hidden="true">"⚙️"</span>
                    <span class="tab-btn-label">"Activité"</span>
                </button>

                // Groupe 4 : Infrastructure & Agents
                <span class="nav-group-label" aria-hidden="true">"Infrastructure"</span>
                <button
                    type="button"
                    role="tab"
                    id="tab-btn-workers"
                    class="tab-btn"
                    aria-selected="false"
                    aria-controls="panel-workers"
                    tabindex="-1"
                    title="Groupe Infrastructure : État des agents de traitement et workers"
                >
                    <span class="tab-role-icon" aria-hidden="true">"🖥️"</span>
                    <span class="tab-btn-label">"Nœuds"</span>
                </button>
                <button
                    type="button"
                    role="tab"
                    id="tab-btn-plugins"
                    class="tab-btn"
                    aria-selected="false"
                    aria-controls="panel-plugins"
                    tabindex="-1"
                    title="Groupe Infrastructure : Outils média MKVToolNix, FFmpeg et extensions"
                >
                    <span class="tab-role-icon" aria-hidden="true">"🔌"</span>
                    <span class="tab-btn-label">"Plugins"</span>
                </button>

                // Groupe 5 : Administration & Système
                <span class="nav-group-label" aria-hidden="true">"Système"</span>
                <button
                    type="button"
                    role="tab"
                    id="tab-btn-settings"
                    class="tab-btn"
                    aria-selected="false"
                    aria-controls="panel-settings"
                    tabindex="-1"
                    title="Groupe Administration : Réglages du central, jetons et sécurité"
                >
                    <span class="tab-role-icon" aria-hidden="true">"🛡️"</span>
                    <span class="tab-btn-label">"Paramètres"</span>
                </button>
                <button
                    type="button"
                    role="tab"
                    id="tab-btn-updates"
                    class="tab-btn"
                    aria-selected="false"
                    aria-controls="panel-updates"
                    tabindex="-1"
                    title="Groupe Administration : Suivi des versions, conteneur et changelog"
                >
                    <span class="tab-role-icon" aria-hidden="true">"🚀"</span>
                    <span class="tab-btn-label">"Mises à jour"</span>
                </button>
            </div>
        </nav>
    }
}
