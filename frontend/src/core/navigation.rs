//! Définition des rôles métier et de la barre de navigation.

use leptos::prelude::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BusinessRole {
    Piloter,
    Preparer,
    Construire,
    Exploiter,
    Maintenir,
    Administrer,
}

#[component]
pub fn Navigation() -> impl IntoView {
    view! {
        <nav class="tab-navigation" aria-label="Navigation principale par rôles métier">
            <div class="tab-list" role="tablist">
                <button
                    type="button"
                    role="tab"
                    id="tab-btn-dashboard"
                    class="tab-btn"
                    aria-selected="true"
                    aria-controls="panel-dashboard"
                    tabindex="0"
                    title="Rôle Piloter : Vue d'ensemble et état de santé du système"
                >
                    <span class="tab-role-icon" aria-hidden="true">"📊 "</span>
                    "Piloter"
                </button>
                <button
                    type="button"
                    role="tab"
                    id="tab-btn-files"
                    class="tab-btn"
                    aria-selected="false"
                    aria-controls="panel-files"
                    tabindex="-1"
                    title="Rôle Préparer : Fichiers sources et médias d'entrée"
                >
                    <span class="tab-role-icon" aria-hidden="true">"📁 "</span>
                    "Préparer"
                </button>
                <button
                    type="button"
                    role="tab"
                    id="tab-btn-flows"
                    class="tab-btn"
                    aria-selected="false"
                    aria-controls="panel-flows"
                    tabindex="-1"
                    title="Rôle Construire : Espace de conception des pipelines de traitement"
                >
                    <span class="tab-role-icon" aria-hidden="true">"⚡ "</span>
                    "Construire (Flows)"
                </button>
                <button
                    type="button"
                    role="tab"
                    id="tab-btn-jobs"
                    class="tab-btn"
                    aria-selected="false"
                    aria-controls="panel-jobs"
                    tabindex="-1"
                    title="Rôles Exploiter & Diagnostiquer : Suivi en direct et analyse des exécutions"
                >
                    <span class="tab-role-icon" aria-hidden="true">"⚙️ "</span>
                    "Activité"
                </button>
                <button
                    type="button"
                    role="tab"
                    id="tab-btn-workers"
                    class="tab-btn"
                    aria-selected="false"
                    aria-controls="panel-workers"
                    tabindex="-1"
                    title="Rôle Maintenir : État des agents de traitement et outils média"
                >
                    <span class="tab-role-icon" aria-hidden="true">"🖥️ "</span>
                    "Nœuds"
                </button>
                <button
                    type="button"
                    role="tab"
                    id="tab-btn-plugins"
                    class="tab-btn"
                    aria-selected="false"
                    aria-controls="panel-plugins"
                    tabindex="-1"
                    title="Rôle Maintenir : Outils média MKVToolNix, FFmpeg et plugins"
                >
                    <span class="tab-role-icon" aria-hidden="true">"🔌 "</span>
                    "Plugins"
                </button>
                <button
                    type="button"
                    role="tab"
                    id="tab-btn-settings"
                    class="tab-btn"
                    aria-selected="false"
                    aria-controls="panel-settings"
                    tabindex="-1"
                    title="Rôle Administrer : Réglages du central, jetons et sécurité"
                >
                    <span class="tab-role-icon" aria-hidden="true">"🛡️ "</span>
                    "Administrer"
                </button>
            </div>
        </nav>
    }
}
