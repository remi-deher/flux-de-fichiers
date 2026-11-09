//! Composant d'affichage des métriques du cluster.

use leptos::prelude::*;

#[component]
pub fn MetricsBar() -> impl IntoView {
    view! {
        <section class="metrics-grid" aria-label="Indicateurs de fonctionnement">
            <div class="metric-card">
                <span class="metric-label">"Coordinateur"</span>
                <span id="metric-health" class="metric-value">"—"</span>
                <span class="metric-desc">"Sonde /health"</span>
            </div>
            <div class="metric-card">
                <span class="metric-label">"Agents enregistrés"</span>
                <span id="metric-workers" class="metric-value">"—"</span>
                <span class="metric-desc">"Enregistrés en base"</span>
            </div>
            <div class="metric-card">
                <span class="metric-label">"Fichiers sources"</span>
                <span id="metric-files" class="metric-value">"—"</span>
                <span class="metric-desc">"Dans INPUT_DIR"</span>
            </div>
            <div class="metric-card">
                <span class="metric-label">"Tâches enregistrées"</span>
                <span id="metric-jobs" class="metric-value">"—"</span>
                <span class="metric-desc">"Historique et file"</span>
            </div>
        </section>
    }
}
