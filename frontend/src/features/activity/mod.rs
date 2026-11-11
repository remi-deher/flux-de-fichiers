//! Module d'activité : Rôles Exploiter (traitements en direct) et Diagnostiquer (historique et échecs).

pub mod diagnostic;
pub mod operations;

pub use diagnostic::ActivityDiagnostic;
pub use operations::ActivityOperations;

use leptos::prelude::*;

#[component]
pub fn JobsPanel() -> impl IntoView {
    view! {
        <div id="panel-jobs" class="tab-panel" role="tabpanel" aria-labelledby="tab-btn-jobs" hidden>
            <section class="section-panel" aria-labelledby="jobs-section-title">
                <div class="section-panel-header">
                    <div>
                        <h2 id="jobs-section-title" class="section-title">"Poste d'activité et supervision"</h2>
                        <p class="section-subtitle">
                            "Suivi temps réel des traitements actifs (Exploiter) et analyse approfondie des exécutions antérieures (Diagnostiquer)."
                        </p>
                    </div>
                    <div id="sse-status-badge" class="status-pill info" title="Mode de rafraîchissement des tâches" aria-live="polite">
                        <span class="status-dot"></span>
                        <span id="sse-status-text">"Sondage périodique"</span>
                    </div>
                </div>

                // Sous-navigation par rôle métier (Exploiter vs Diagnostiquer)
                <div class="activity-role-tabs" role="tablist" aria-label="Rôles de gestion d'activité" style="display: flex; gap: 0.5rem; margin-bottom: 1rem; border-bottom: 1px solid var(--border-color); padding-bottom: 0.5rem;">
                    <button
                        type="button"
                        id="tab-role-operations"
                        class="tab-btn active"
                        role="tab"
                        aria-selected="true"
                        style="padding: 0.4rem 0.85rem; font-size: 0.875rem;"
                    >
                        "⚡ Exploiter (File & Direct)"
                    </button>
                    <button
                        type="button"
                        id="tab-role-diagnostic"
                        class="tab-btn"
                        role="tab"
                        aria-selected="false"
                        style="padding: 0.4rem 0.85rem; font-size: 0.875rem;"
                    >
                        "🔬 Diagnostiquer (Historique & Erreurs)"
                    </button>
                </div>

                <ActivityOperations />
                <ActivityDiagnostic />
            </section>
        </div>
    }
}
