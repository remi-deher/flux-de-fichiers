//! Racine de l'application Leptos pour le poste de pilotage Flux de Fichiers.

use crate::{
    core::{Header, Sidebar},
    features::{
        activity::JobsPanel, dashboard::DashboardPanel, flows::FlowsPanel,
        libraries::LibrariesPanel, plugins::PluginsPanel, settings::SettingsPanel,
        updates::UpdatesPanel, workers::WorkersPanel,
    },
};
use leptos::prelude::*;

#[component]
pub fn App() -> impl IntoView {
    view! {
        <a href="#main-content" class="skip-link">
            "Aller au contenu principal"
        </a>
        <div class="application-shell">
            <div class="application-navigation-layout">
                // Barre latérale persistante (Sidebar Rail)
                <Sidebar />

                // Région d'affichage : En-tête supérieur + Contenu principal
                <div class="page-transition-region">
                    <Header />

                    <div
                        id="live-notice"
                        class="toast-notice"
                        role="status"
                        aria-live="polite"
                    ></div>

                    <main id="main-content" class="main-content">
                        <DashboardPanel />
                        <LibrariesPanel />
                        <FlowsPanel />
                        <JobsPanel />
                        <WorkersPanel />
                        <PluginsPanel />
                        <SettingsPanel />
                        <UpdatesPanel />
                    </main>

                    <footer class="shell-footer">
                        <p>
                            "Flux de Fichiers v0.6 — Architecture et Shell inspirés d'Encoding Central."
                        </p>
                        <p style="margin-top: 0.25rem;">
                            "100% Rust & Leptos natif • Navigation Rail persistante • Contrastes renforcés WCAG AAA • Moteur Bare Core"
                        </p>
                    </footer>
                </div>
            </div>
        </div>
    }
}
