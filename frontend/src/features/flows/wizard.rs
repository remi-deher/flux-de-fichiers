//! Dialogue modal : Assistant de création guidée (Setup Wizard).

use leptos::prelude::*;

#[component]
pub fn FlowWizardDialog() -> impl IntoView {
    view! {
        <dialog id="flow-wizard-dialog" class="app-dialog" aria-labelledby="flow-wizard-title" aria-modal="true" style="max-width: 820px;">
            <div class="dialog-header">
                <h3 id="flow-wizard-title" class="dialog-title">"Assistant de configuration : Nouveau flux"</h3>
                <button
                    type="button"
                    id="flow-wizard-close-x"
                    class="btn btn-secondary btn-sm"
                    aria-label="Fermer l'assistant"
                >
                    "✕"
                </button>
            </div>
            <div class="dialog-body">
                <p class="section-subtitle" style="margin-top: 0;">
                    "Choisissez un modèle prêt à l'emploi. L'assistant pré-configurera et reliera les blocs sur votre plan de travail."
                </p>

                // Étape 1 : Cartes de modèles de départ
                <div class="wizard-templates-grid">
                    <div class="wizard-template-card active" data-template="smart-hevc">
                        <span class="template-icon">"🔀"</span>
                        <div>
                            <h4 class="template-title">"Flux Intelligent : Aiguillage HEVC & Repli"</h4>
                            <p class="template-desc">"Saut direct sans réencodage si déjà en HEVC, encodage GPU Intel QSV sinon, avec repli CPU automatique."</p>
                        </div>
                    </div>

                    <div class="wizard-template-card" data-template="hevc">
                        <span class="template-icon">"🎬"</span>
                        <div>
                            <h4 class="template-title">"Transcodage HEVC Haute Efficacité"</h4>
                            <p class="template-desc">"Réencode en HEVC avec accélération GPU (Intel QSV / VA-API), audio AAC stéréo et remuxage MKV."</p>
                        </div>
                    </div>

                    <div class="wizard-template-card" data-template="mkv-remux">
                        <span class="template-icon">"📦"</span>
                        <div>
                            <h4 class="template-title">"Remuxage sans perte Matroska"</h4>
                            <p class="template-desc">"Aucun réencodage. Conserve les langues sélectionnées (fre, eng) et nettoie les pistes superflues."</p>
                        </div>
                    </div>

                    <div class="wizard-template-card" data-template="mp4-web">
                        <span class="template-icon">"🌐"</span>
                        <div>
                            <h4 class="template-title">"Optimisation Streaming Web (MP4 FastStart)"</h4>
                            <p class="template-desc">"Remuxe vers conteneur MP4 avec atome moov en tête de fichier pour streaming immédiat."</p>
                        </div>
                    </div>

                    <div class="wizard-template-card" data-template="blank">
                        <span class="template-icon">"⬜"</span>
                        <div>
                            <h4 class="template-title">"Flux vierge personnalisé"</h4>
                            <p class="template-desc">"Démarre avec le fichier source pour composer librement vos blocs sur le canvas."</p>
                        </div>
                    </div>
                </div>

                // Étape 2 : Pré-réglages rapides
                <div class="wizard-quick-settings" style="margin-top: 1.5rem; padding: 1rem; background: var(--bg-subtle); border-radius: var(--radius-sm); border: 1px solid var(--border-color);">
                    <h4 style="margin: 0 0 0.75rem 0; font-size: 0.9375rem; color: var(--text-main);">
                        "Paramètres initiaux du modèle sélectionné"
                    </h4>
                    <div style="display: grid; grid-template-columns: 1fr 1fr; gap: 0.75rem;">
                        <div class="form-group">
                            <label for="wizard-flow-name" class="form-label">"Nom du flux"</label>
                            <input type="text" id="wizard-flow-name" class="form-input" value="Transcodage HEVC Intel QSV" />
                        </div>
                        <div class="form-group">
                            <label for="wizard-flow-mask" class="form-label">"Masque de fichiers entrants"</label>
                            <input type="text" id="wizard-flow-mask" class="form-input" value="*.mkv, *.mp4, *.avi" />
                        </div>
                    </div>
                    <div style="display: grid; grid-template-columns: 1fr 1fr; gap: 0.75rem; margin-top: 0.5rem;">
                        <div class="form-group">
                            <label for="wizard-flow-langs" class="form-label">"Langues conservées (audio & sous-titres)"</label>
                            <input type="text" id="wizard-flow-langs" class="form-input" value="fre, eng" />
                        </div>
                        <div class="form-group">
                            <label for="wizard-flow-encoder" class="form-label">"Encodeur matériel préféré"</label>
                            <select id="wizard-flow-encoder" class="form-select">
                                <option value="qsv" selected="selected">"Intel QSV (Recommandé sur TrueNAS)"</option>
                                <option value="vaapi">"VA-API Linux générique"</option>
                                <option value="nvenc">"Nvidia NVENC"</option>
                                <option value="cpu">"CPU uniquement"</option>
                            </select>
                        </div>
                    </div>
                </div>
            </div>
            <div class="dialog-footer" style="display: flex; justify-content: flex-end; gap: 0.75rem; padding-top: 1rem; border-top: 1px solid var(--border-color); margin-top: 1rem;">
                <button
                    type="button"
                    id="flow-wizard-cancel-btn"
                    class="btn btn-secondary"
                >
                    "Annuler"
                </button>
                <button
                    type="button"
                    id="flow-wizard-generate-btn"
                    class="btn btn-primary"
                >
                    "🚀 Générer et ouvrir dans le Canvas"
                </button>
            </div>
        </dialog>
    }
}
