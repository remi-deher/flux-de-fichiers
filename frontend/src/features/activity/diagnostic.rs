//! Sous-vue Diagnostiquer : Historique, analyse des échecs et chronologie d'exécution (Rôle Diagnostiquer).

use leptos::prelude::*;

#[component]
pub fn ActivityDiagnostic() -> impl IntoView {
    view! {
        <div id="activity-view-diagnostic" class="activity-subview" style="display: none;">
            <div class="toolbar-bar">
                <div class="search-box">
                    <span class="search-icon" aria-hidden="true">"🔍"</span>
                    <input
                        type="search"
                        id="diagnostic-search-input"
                        class="search-input"
                        placeholder="Rechercher dans l'historique ou les erreurs…"
                        aria-label="Rechercher dans l'historique"
                    />
                </div>
                <div class="filter-group" role="group" aria-label="Filtrer l'historique">
                    <button
                        type="button"
                        class="filter-chip diag-chip"
                        data-diag-filter="failed"
                        aria-pressed="false"
                    >
                        "Échecs" <span id="diag-count-failed" style="opacity: 0.8;">"(0)"</span>
                    </button>
                    <button
                        type="button"
                        class="filter-chip diag-chip"
                        data-diag-filter="succeeded"
                        aria-pressed="false"
                    >
                        "Succès" <span id="diag-count-succeeded" style="opacity: 0.8;">"(0)"</span>
                    </button>
                </div>
            </div>

            <div style="margin-top: 1rem; padding: 0.75rem 1rem; background: var(--bg-subtle); border-radius: var(--radius-sm); border-left: 4px solid var(--primary); font-size: 0.8125rem; color: var(--text-dim);">
                <strong>"Diagnostic d'intégrité :"</strong> " Les exécutions terminées conservent leur empreinte SHA-256 et la validation de conteneur. En cas d'erreur de transcodage ou d'incompatibilité, la cause technique exacte est conservée pour permettre un rejeu direct après ajustement du flux."
            </div>
        </div>
    }
}
