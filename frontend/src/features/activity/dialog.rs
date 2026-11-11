//! Dialogue modal d'inspection détaillée d'une tâche de traitement.

use leptos::prelude::*;

#[component]
pub fn JobDetailDialog() -> impl IntoView {
    view! {
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
