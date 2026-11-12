//! Explorateur de médias sources et dialogue de confirmation de traitement.

use crate::shared::ui::{RegistreFact, RegistreTable};
use leptos::prelude::*;

#[component]
pub fn FileBrowser() -> impl IntoView {
    view! {
        <div class="registre" id="library-registre">
            <input type="search" id="files-search-input" hidden />
            <RegistreTable
                aria_label="Fichiers de la bibliothèque"
                headers=vec!["Fichier", "Taille", "Action"]
                body_id="files-table-body"
                empty="Chargement des médias sources…"
            />
            <RegistreFact id="files-fact" action_id="files-fact-action" action="Lancer un flux" />
        </div>

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
