//! Panneau des mises à jour applicatives et du statut de version du conteneur.

use leptos::prelude::*;

const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[component]
pub fn UpdatesPanel() -> impl IntoView {
    view! {
        <div
            id=crate::core::panel::UPDATES.dom_id
            class="tab-panel"
            role="tabpanel"
            aria-labelledby=crate::core::panel::UPDATES.button_id
            hidden
        >
            <section class="section-panel" aria-labelledby="updates-status-title">
                <div class="section-panel-header">
                    <div>
                        <h2 id="updates-status-title" class="section-title">
                            "Statut des Mises à jour & Version"
                        </h2>
                        <p class="section-subtitle">
                            "Suivi des versions publiées, intégrité de l'image Docker et compatibilité Watchtower."
                        </p>
                    </div>
                    <div class="status-pill success">
                        <span>"Système à jour"</span>
                    </div>
                </div>

                <div class="detail-grid">
                    <div class="detail-term">"Version installée"</div>
                    <div class="detail-val">
                        <code>{format!("v{}", CURRENT_VERSION)}</code>
                    </div>

                    <div class="detail-term">"Image conteneur"</div>
                    <div class="detail-val">
                        <code>"ghcr.io/remi-deher/flux-de-fichiers:latest"</code>
                    </div>

                    <div class="detail-term">"Cycle de mise à jour"</div>
                    <div class="detail-val">
                        "Automatique (Compatible Watchtower ou redémarrage de conteneur)"
                    </div>

                    <div class="detail-term">"Dépôt et releases"</div>
                    <div class="detail-val">
                        <a
                            href="https://github.com/remi-deher/flux-de-fichiers/releases"
                            target="_blank"
                            rel="noopener noreferrer"
                            style="color: var(--accent-color); text-decoration: underline;"
                        >
                            "Consulter les publications GitHub"
                        </a>
                    </div>
                </div>
            </section>

            <section class="section-panel" aria-labelledby="changelog-title">
                <div class="section-panel-header">
                    <div>
                        <h2 id="changelog-title" class="section-title">
                            "Notes de versions récentes"
                        </h2>
                        <p class="section-subtitle">
                            "Historique des évolutions déployées en production."
                        </p>
                    </div>
                </div>

                <div style="display: flex; flex-direction: column; gap: 1rem;">
                    <div class="card" style="padding: 1rem;">
                        <div style="display: flex; justify-content: space-between; align-items: baseline; margin-bottom: 0.5rem;">
                            <h3 style="margin: 0; font-size: 1rem; font-weight: 600;">
                                "v0.4.1 — Restructuration Flows & Activité"
                            </h3>
                            <span style="font-size: 0.8125rem; color: var(--text-muted);">
                                "Novembre 2026"
                            </span>
                        </div>
                        <p style="margin: 0; font-size: 0.875rem; color: var(--text-color);">
                            "Refonte modulaire inspirée d'Encoding Central : canevas réactif de construction de flux, tiroir d'inspection avec animation spring, scission de l'activité en opérations directes et diagnostic approfondi."
                        </p>
                    </div>

                    <div class="card" style="padding: 1rem;">
                        <div style="display: flex; justify-content: space-between; align-items: baseline; margin-bottom: 0.5rem;">
                            <h3 style="margin: 0; font-size: 1rem; font-weight: 600;">
                                "v0.4.0 — Moteur de pipelines unifié"
                            </h3>
                            <span style="font-size: 0.8125rem; color: var(--text-muted);">
                                "Novembre 2026"
                            </span>
                        </div>
                        <p style="margin: 0; font-size: 0.875rem; color: var(--text-color);">
                            "Chaînes de traitement par blocs composables, journalisation SQLite WAL, bascule dynamique des tâches et flux d'événements SSE."
                        </p>
                    </div>

                    <div class="card" style="padding: 1rem;">
                        <div style="display: flex; justify-content: space-between; align-items: baseline; margin-bottom: 0.5rem;">
                            <h3 style="margin: 0; font-size: 1rem; font-weight: 600;">
                                "v0.3.0 — Poste de pilotage Leptos"
                            </h3>
                            <span style="font-size: 0.8125rem; color: var(--text-muted);">
                                "Novembre 2026"
                            </span>
                        </div>
                        <p style="margin: 0; font-size: 0.875rem; color: var(--text-color);">
                            "Interface Leptos servie par Axum, conformité stricte d'accessibilité W3C, contraste renforcé et authentification sécurisée."
                        </p>
                    </div>
                </div>
            </section>
        </div>
    }
}
