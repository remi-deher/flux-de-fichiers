//! Panneau des fichiers sources admissibles et déclenchement de remuxage.

use leptos::prelude::*;

#[component]
pub fn FilesPanel() -> impl IntoView {
    view! {
        <div id="panel-files" class="tab-panel" role="tabpanel" aria-labelledby="tab-btn-files" hidden>
            <section class="section-panel" aria-labelledby="files-section-title">
                <div class="section-panel-header">
                    <div>
                        <h2 id="files-section-title" class="section-title">"Fichiers sources admissibles"</h2>
                        <p class="section-subtitle">
                            "Fichiers MKV détectés dans var/input/ (montage en lecture seule). Les fichiers d'origine ne sont jamais modifiés."
                        </p>
                    </div>
                    <div id="files-counter-badge" class="status-pill info">
                        <span id="files-counter-text">"0 fichier(s)"</span>
                    </div>
                </div>

                <div class="toolbar-bar">
                    <div class="search-box">
                        <span class="search-icon" aria-hidden="true">"🔍"</span>
                        <input
                            type="search"
                            id="files-search-input"
                            class="search-input"
                            placeholder="Rechercher un fichier source…"
                            aria-label="Filtrer les fichiers sources par nom"
                        />
                    </div>
                    <div class="form-help">
                        "Pour ajouter des vidéos, déposez vos fichiers .mkv dans le dossier var/input/ du serveur."
                    </div>
                </div>

                <div class="table-wrapper">
                    <table class="data-table" aria-label="Liste des fichiers sources disponibles">
                        <thead>
                            <tr>
                                <th scope="col">"Nom du fichier"</th>
                                <th scope="col">"Taille"</th>
                                <th scope="col" style="text-align: right;">"Action"</th>
                            </tr>
                        </thead>
                        <tbody id="files-table-body">
                            <tr>
                                <td colspan="3" class="empty-state-text">
                                    "Chargement des fichiers sources…"
                                </td>
                            </tr>
                        </tbody>
                    </table>
                </div>
            </section>

            // Dialogue modal accessible de confirmation de lancement d'un traitement
            <dialog id="confirm-remux-dialog" class="app-dialog" aria-labelledby="confirm-dialog-title" aria-modal="true">
                <div class="dialog-header">
                    <h3 id="confirm-dialog-title" class="dialog-title">"Confirmer le lancement du traitement"</h3>
                    <button
                        type="button"
                        id="confirm-dialog-close-x"
                        class="btn btn-secondary btn-sm"
                        aria-label="Fermer la boîte de dialogue"
                    >
                        "✕"
                    </button>
                </div>
                <div class="dialog-body">
                    <p>
                        "Vous vous apprêtez à soumettre une tâche de traitement pour le média :"
                    </p>
                    <div class="code-snippet" id="confirm-dialog-filename">
                        "nom-du-fichier.mkv"
                    </div>
                    <div class="form-group">
                        <label for="confirm-dialog-flow-select" class="form-label">
                            "Flux de traitement à exécuter"
                        </label>
                        <select id="confirm-dialog-flow-select" class="form-select">
                            <option value="flow.default.mkv" selected="selected">"Remuxage Matroska propre (défaut)"</option>
                            <option value="flow.transcode.hevc">"Transcodage HEVC / Intel QSV & VA-API"</option>
                            <option value="flow.remux.mp4">"Optimisation Web MP4 (FastStart)"</option>
                        </select>
                        <p class="form-help">
                            "Sélectionnez la chaîne de blocs configurée dans l'onglet Flux de traitement."
                        </p>
                    </div>
                    <div class="form-group">
                        <label for="confirm-dialog-worker-select" class="form-label">
                            "Agent ou nœud cible (optionnel)"
                        </label>
                        <select id="confirm-dialog-worker-select" class="form-select">
                            <option value="">"N'importe quel agent disponible (recommandé)"</option>
                        </select>
                        <p class="form-help">
                            "Si vous ciblez un agent précis, la tâche lui restera réservée sans bascule automatique."
                        </p>
                    </div>
                    <div class="status-pill info" style="width: fit-content;">
                        "ℹ️ Le fichier d'origine reste intact. Le résultat sera placé dans le volume data."
                    </div>
                </div>
                <div class="dialog-footer">
                    <button
                        type="button"
                        id="confirm-dialog-cancel-btn"
                        class="btn btn-secondary"
                    >
                        "Annuler"
                    </button>
                    <button
                        type="button"
                        id="confirm-dialog-submit-btn"
                        class="btn btn-primary"
                    >
                        "Démarrer le traitement"
                    </button>
                </div>
            </dialog>
        </div>
    }
}
