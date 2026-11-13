//! Racine de l'application Leptos pour le poste de pilotage Flux de Fichiers.

use crate::{
    core::{Header, Sidebar},
    features::{
        activity::JobsPanel, auth::AuthGate, dashboard::DashboardPanel, flows::FlowsPanel,
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
        <AuthGate />
        <div id="application-shell" class="application-shell" hidden>
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
                </div>
            </div>
        </div>
    }
}
