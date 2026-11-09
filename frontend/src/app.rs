//! Racine de l'application Leptos pour Flux de Fichiers.

use crate::components::*;
use leptos::prelude::*;

#[component]
pub fn App() -> impl IntoView {
    view! {
        <a href="#main-content" class="skip-link">
            "Aller au contenu principal"
        </a>
        <div class="app-container">
            <Header />
            <div
                id="live-notice"
                class="toast-notice"
                role="status"
                aria-live="polite"
            ></div>
            <main id="main-content">
                <MetricsBar />
                <AuthCard />
                <FilesTable />
                <JobsTable />
                <WorkersTable />
                <FutureSection />
            </main>
            <footer style="margin-top: 3rem; text-align: center; font-size: 0.8125rem; color: var(--text-dim); border-top: 1px solid var(--border-color); padding-top: 1.5rem;">
                <p>
                    "Flux de Fichiers v0.2 — Interface de pilotage Leptos moderne et responsive."
                </p>
                <p style="margin-top: 0.25rem;">
                    "Conçue selon les recommandations d'accessibilité (contrastes renforcés, navigation clavier, repères W3C)."
                </p>
                <p style="margin-top: 0.25rem;">
                    "Remuxage sans réencodage MKVToolNix • Coordinateur Axum / Tokio • PostgreSQL local"
                </p>
            </footer>
        </div>
    }
}
