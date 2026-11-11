//! Explorateur de médias sources et dialogue de confirmation de traitement.

use leptos::prelude::*;

#[component]
pub fn FileBrowser() -> impl IntoView {
    view! {
        <section class="section-panel" aria-labelledby="files-section-title">
            <div class="section-panel-header">
                <div>
                    <h2 id="files-section-title" class="section-title">
                        "Médias sources admissibles"
                    </h2>
                    <p class="section-subtitle">
                        "Fichiers détectés dans les volumes surveillés (montage en lecture seule). Les fichiers d'origine ne sont jamais modifiés."
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
                        placeholder="Rechercher un média source…"
                        aria-label="Filtrer les médias sources par nom"
                    />
                </div>
                <div class="form-help">
                    "Déposez vos médias dans les dossiers de bibliothèques surveillés. Tous formats admissibles automatiquement."
                </div>
            </div>

            <div class="table-wrapper">
                <table class="data-table" aria-label="Liste des fichiers sources disponibles">
                    <thead>
                        <tr>
                            <th scope="col">"Nom du média"</th>
                            <th scope="col">"Taille"</th>
                            <th scope="col" style="text-align: right;">"Action"</th>
                        </tr>
                    </thead>
                    <tbody id="files-table-body">
                        <tr>
                            <td colspan="3" class="empty-state-text">
                                "Chargement des médias sources…"
                            </td>
                        </tr>
                    </tbody>
                </table>
            </div>
        </section>

        // Dialogue modal accessible de confirmation de lancement d'un traitement
        <dialog id="confirm-remux-dialog" class="app-dialog" aria-labelledby="confirm-dialog-title" aria-modal="true">
            <div class="dialog-header">
                <h3 id="confirm-dialog-title" class="dialog-title">
                    "Confirmer le lancement du traitement"
                </h3>
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
                        <option value="flow.audio.r128">"Extraction & Normalisation Audio EBU R128"</option>
                        <option value="flow.image.thumbnails">"Miniatures chapitrées & Posters WebP"</option>
                        <option value="flow.remux.mp4">"Optimisation Streaming MP4 (FastStart)"</option>
                    </select>
                    <p class="form-help">
                        "Sélectionnez le pipeline à exécuter ou laissez le flux assigné à la bibliothèque."
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
    }
}
