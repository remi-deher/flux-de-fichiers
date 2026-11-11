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
                        class="filter-chip"
                        data-filter="failed"
                        aria-pressed="false"
                    >
                        "Échecs" <span id="count-filter-failed" style="opacity: 0.8;">"(0)"</span>
                    </button>
                    <button
                        type="button"
                        class="filter-chip"
                        data-filter="succeeded"
                        aria-pressed="false"
                    >
                        "Succès" <span id="count-filter-succeeded" style="opacity: 0.8;">"(0)"</span>
                    </button>
                </div>
            </div>

            <div style="margin-top: 1rem; padding: 0.75rem 1rem; background: var(--bg-subtle); border-radius: var(--radius-sm); border-left: 4px solid var(--primary); font-size: 0.8125rem; color: var(--text-dim);">
                <strong>"Diagnostic d'intégrité :"</strong> " Les exécutions terminées conservent leur empreinte SHA-256 et la validation de conteneur. En cas d'erreur de transcodage ou d'incompatibilité, la cause technique exacte est conservée pour permettre un rejeu direct après ajustement du flux."
            </div>
        </div>

        // Dialogue modal d'inspection détaillée d'une tâche
        <dialog id="job-detail-dialog" class="app-dialog" aria-labelledby="job-dialog-title" aria-modal="true">
            <div class="dialog-header">
                <h3 id="job-dialog-title" class="dialog-title">"Détails & Diagnostic de la tâche"</h3>
                <button
                    type="button"
                    id="job-dialog-close-x"
                    class="btn btn-secondary btn-sm"
                    aria-label="Fermer la boîte de dialogue"
                >
                    "✕"
                </button>
            </div>
            <div class="dialog-body">
                <div class="detail-grid">
                    <div class="detail-term">"ID de la tâche"</div>
                    <div id="detail-job-id" class="detail-val">"—"</div>

                    <div class="detail-term">"Fichier source"</div>
                    <div id="detail-job-source" class="detail-val">"—"</div>

                    <div class="detail-term">"Statut"</div>
                    <div id="detail-job-status" class="detail-val">"—"</div>

                    <div class="detail-term">"Agent assigné"</div>
                    <div id="detail-job-worker" class="detail-val">"—"</div>

                    <div class="detail-term">"Tentatives"</div>
                    <div id="detail-job-attempts" class="detail-val">"—"</div>

                    <div class="detail-term">"Création"</div>
                    <div id="detail-job-created" class="detail-val">"—"</div>

                    <div class="detail-term">"Fichier résultat"</div>
                    <div id="detail-job-output" class="detail-val">"—"</div>
                </div>

                <div id="detail-error-box" class="toast-notice error" style="display: none;">
                    <div style="font-weight: 700; margin-bottom: 0.25rem;">"Erreur signalée :"</div>
                    <div id="detail-error-content" class="code-snippet">"—"</div>
                </div>

                <div id="detail-curl-section" style="margin-top: 0.5rem; display: none;">
                    <label class="form-label">"Commande de téléchargement directe (curl) :"</label>
                    <div id="detail-curl-command" class="code-snippet">
                        "curl -H 'Authorization: Bearer <ADMIN_KEY>' http://localhost:3000/api/v1/jobs/.../output -o resultat.mkv"
                    </div>
                </div>
            </div>
            <div class="dialog-footer">
                <button
                    type="button"
                    id="job-dialog-close-btn"
                    class="btn btn-secondary"
                >
                    "Fermer"
                </button>
                <button
                    type="button"
                    id="job-dialog-download-btn"
                    class="btn btn-primary"
                    style="display: none;"
                >
                    "Télécharger le résultat"
                </button>
            </div>
        </dialog>
    }
}
