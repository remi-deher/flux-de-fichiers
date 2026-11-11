//! Racine de l'application Leptos pour le poste de pilotage Flux de Fichiers.

use crate::{components::*, core::AppShell};
use leptos::prelude::*;

#[component]
pub fn App() -> impl IntoView {
    view! {
        <a href="#main-content" class="skip-link">
            "Aller au contenu principal"
        </a>
        <div class="app-container">
            <AppShell />
            <div
                id="live-notice"
                class="toast-notice"
                role="status"
                aria-live="polite"
            ></div>
            <main id="main-content">
                <DashboardPanel />
                <FilesPanel />
                <FlowsPanel />
                <JobsPanel />
                <WorkersPanel />
                <PluginsPanel />
                <SettingsPanel />
            </main>
            <footer style="margin-top: 3rem; text-align: center; font-size: 0.8125rem; color: var(--text-dim); border-top: 1px solid var(--border-color); padding-top: 1.5rem;">
                <p>
                    "Flux de Fichiers v0.3 — Poste de pilotage Leptos moderne et responsive."
                </p>
                <p style="margin-top: 0.25rem;">
                    "Recommandations d'accessibilité W3C suivies • Contrastes renforcés (>= 5.9:1) • Navigation clavier complète • Thèmes clair/sombre"
                </p>
                <p style="margin-top: 0.25rem;">
                    "Remuxage MKVToolNix • Mode centralisé autonome ou workers distants • SQLite /config"
                </p>
            </footer>
        </div>
    }
}
