//! Panneau de gestion et de suivi de la file des tâches.

use leptos::prelude::*;

#[component]
pub fn JobsPanel() -> impl IntoView {
    view! {
        <div id="panel-jobs" class="tab-panel" role="tabpanel" aria-labelledby="tab-btn-jobs" hidden>
            <section class="section-panel" aria-labelledby="jobs-section-title">
                <div class="section-panel-header">
                    <div>
                        <h2 id="jobs-section-title" class="section-title">"Gestionnaire de tâches d'exécution"</h2>
                        <p class="section-subtitle">
                            "Supervision temps réel des tâches de traitement et d'exécution des flux (en attente, en cours, succès, échecs) sur l'ensemble des nœuds."
                        </p>
                    </div>
                </div>

                <div class="toolbar-bar">
                    <div class="search-box">
                        <span class="search-icon" aria-hidden="true">"🔍"</span>
                        <input
                            type="search"
                            id="jobs-search-input"
                            class="search-input"
                            placeholder="Rechercher par ID ou nom de fichier…"
                            aria-label="Filtrer les tâches par ID ou nom de fichier"
                        />
                    </div>
                    <div class="filter-group" role="group" aria-label="Filtrer les tâches par statut">
                        <button
                            type="button"
                            class="filter-chip"
                            data-filter="all"
                            aria-pressed="true"
                        >
                            "Tous" <span id="count-filter-all" style="opacity: 0.8;">"(0)"</span>
                        </button>
                        <button
                            type="button"
                            class="filter-chip"
                            data-filter="queued"
                            aria-pressed="false"
                        >
                            "En attente" <span id="count-filter-queued" style="opacity: 0.8;">"(0)"</span>
                        </button>
                        <button
                            type="button"
                            class="filter-chip"
                            data-filter="running"
                            aria-pressed="false"
                        >
                            "En cours" <span id="count-filter-running" style="opacity: 0.8;">"(0)"</span>
                        </button>
                        <button
                            type="button"
                            class="filter-chip"
                            data-filter="succeeded"
                            aria-pressed="false"
                        >
                            "Succès" <span id="count-filter-succeeded" style="opacity: 0.8;">"(0)"</span>
                        </button>
                        <button
                            type="button"
                            class="filter-chip"
                            data-filter="failed"
                            aria-pressed="false"
                        >
                            "Échecs" <span id="count-filter-failed" style="opacity: 0.8;">"(0)"</span>
                        </button>
                    </div>
                </div>

                <div class="table-wrapper">
                    <table class="data-table" aria-label="Liste filtrée des tâches d'exécution">
                        <thead>
                            <tr>
                                <th scope="col">"Identifiant"</th>
                                <th scope="col">"Source"</th>
                                <th scope="col">"Statut"</th>
                                <th scope="col">"Création"</th>
                                <th scope="col" style="text-align: right;">"Actions"</th>
                            </tr>
                        </thead>
                        <tbody id="jobs-table-body">
                            <tr>
                                <td colspan="5" class="empty-state-text">
                                    "Chargement des tâches…"
                                </td>
                            </tr>
                        </tbody>
                    </table>
                </div>

                <div style="margin-top: 1rem; padding: 0.75rem 1rem; background: var(--bg-subtle); border-radius: var(--radius-sm); border-left: 4px solid var(--primary); font-size: 0.8125rem; color: var(--text-dim);">
                    <strong>"Téléchargement en flux direct :"</strong> " Lorsqu'une session web est active, le téléchargement s'effectue en flux continu directement sur disque sans bufferisation mémoire. Pour les médias de plusieurs gigaoctets via jeton Bearer, la commande curl prête à l'emploi est disponible dans le volet Détails."
                </div>
            </section>

            // Dialogue modal d'inspection détaillée d'une tâche
            <dialog id="job-detail-dialog" class="app-dialog" aria-labelledby="job-dialog-title" aria-modal="true">
                <div class="dialog-header">
                    <h3 id="job-dialog-title" class="dialog-title">"Détails de la tâche"</h3>
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
        </div>
    }
}
