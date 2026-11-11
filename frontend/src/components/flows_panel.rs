//! Panneau de gestion et conception des flux de traitement (Flows) type Zapier / FileFlows.

use leptos::prelude::*;

#[component]
pub fn FlowsPanel() -> impl IntoView {
    view! {
        <div id="panel-flows" class="tab-panel" role="tabpanel" aria-labelledby="tab-btn-flows" hidden>
            // --- Vue 1 : Bibliothèque des flux configurés ---
            <div id="flows-library-view">
                <section class="section-panel" aria-labelledby="flows-section-title">
                    <div class="section-panel-header">
                        <div>
                            <h2 id="flows-section-title" class="section-title">
                                "Gestionnaire de flux de traitement (Flows)"
                            </h2>
                            <p class="section-subtitle">
                                "Automatisez vos pipelines multimédias en agençant visuellement les blocs fournis par les outils (MediaInfo, FFmpeg, MKVToolNix, MP4Box)."
                            </p>
                        </div>
                        <button
                            type="button"
                            id="btn-open-flow-wizard"
                            class="btn btn-primary"
                            aria-label="Ouvrir l'assistant de création de flux"
                        >
                            "+ Nouveau flux (Assistant)"
                        </button>
                    </div>

                    // Grille des flux existants
                    <div class="flows-grid" id="flows-grid-container" aria-label="Liste des flux configurés">
                        // Flow 1 : Remux MKV par défaut
                        <article class="flow-card active-flow" data-flow-id="flow.default.mkv">
                            <div class="flow-card-header">
                                <div>
                                    <div style="display: flex; align-items: center; gap: 0.5rem;">
                                        <h3 class="flow-card-title">"Remuxage Matroska propre"</h3>
                                        <span class="status-pill success" title="Flux actif par défaut">
                                            <span class="status-dot"></span>
                                            "Défaut"
                                        </span>
                                    </div>
                                    <p class="flow-card-desc">
                                        "Inspection préalable des métadonnées, filtrage des langues audio/sous-titres voulues et remuxage sans perte."
                                    </p>
                                </div>
                            </div>

                            <div class="flow-pipeline-container" aria-label="Chaîne des blocs du flux">
                                <div class="flow-pipeline">
                                    <div class="flow-block-step implicit-step">
                                        <span class="step-badge">"1"</span>
                                        <span class="step-name">"🔍 Inspecter (MediaInfo)"</span>
                                        <span class="step-type">"Implicite • Toutes métadonnées"</span>
                                    </div>
                                    <span class="flow-arrow" aria-hidden="true">"➔"</span>
                                    <div class="flow-block-step">
                                        <span class="step-badge">"2"</span>
                                        <span class="step-name">"🎬 Remux MKV (MKVToolNix)"</span>
                                        <span class="step-type">"Langues: fre, eng • Copie directe"</span>
                                    </div>
                                </div>
                            </div>

                            <div class="flow-card-footer">
                                <span class="flow-pattern-tag">"Masque : *.mkv, *.avi, *.mp4"</span>
                                <div style="display: flex; gap: 0.5rem;">
                                    <button
                                        type="button"
                                        class="btn btn-primary btn-sm open-canvas-btn"
                                        data-flow="flow.default.mkv"
                                    >
                                        "Ouvrir l'éditeur visuel (Canvas)"
                                    </button>
                                </div>
                            </div>
                        </article>

                        // Flow 2 : Transcodage HEVC Intel QSV / NVENC
                        <article class="flow-card" data-flow-id="flow.transcode.hevc">
                            <div class="flow-card-header">
                                <div>
                                    <div style="display: flex; align-items: center; gap: 0.5rem;">
                                        <h3 class="flow-card-title">"Transcodage HEVC / Intel QSV & VA-API"</h3>
                                        <span class="status-pill info">"Accélération matérielle"</span>
                                    </div>
                                    <p class="flow-card-desc">
                                        "Réencodage vidéo haute efficacité HEVC avec paramètres macro-blocs (qualité 6/9, vitesse 3/5) et audio AAC."
                                    </p>
                                </div>
                            </div>

                            <div class="flow-pipeline-container" aria-label="Chaîne des blocs du flux">
                                <div class="flow-pipeline">
                                    <div class="flow-block-step implicit-step">
                                        <span class="step-badge">"1"</span>
                                        <span class="step-name">"🔍 Inspecter (MediaInfo)"</span>
                                        <span class="step-type">"Implicite"</span>
                                    </div>
                                    <span class="flow-arrow" aria-hidden="true">"➔"</span>
                                    <div class="flow-block-step">
                                        <span class="step-badge">"2"</span>
                                        <span class="step-name">"🎞️ Encode Vidéo (FFmpeg)"</span>
                                        <span class="step-type">"HEVC • Intel QSV • Q:6 • V:3"</span>
                                    </div>
                                    <span class="flow-arrow" aria-hidden="true">"➔"</span>
                                    <div class="flow-block-step">
                                        <span class="step-badge">"3"</span>
                                        <span class="step-name">"🔊 Encode Audio (FFmpeg)"</span>
                                        <span class="step-type">"AAC • 192 kb/s"</span>
                                    </div>
                                    <span class="flow-arrow" aria-hidden="true">"➔"</span>
                                    <div class="flow-block-step">
                                        <span class="step-badge">"4"</span>
                                        <span class="step-name">"🎬 Remux MKV (MKVToolNix)"</span>
                                        <span class="step-type">"Assemblage final"</span>
                                    </div>
                                </div>
                            </div>

                            <div class="flow-card-footer">
                                <span class="flow-pattern-tag">"Masque : *.mkv, *.mp4, *.ts"</span>
                                <div style="display: flex; gap: 0.5rem;">
                                    <button
                                        type="button"
                                        class="btn btn-primary btn-sm open-canvas-btn"
                                        data-flow="flow.transcode.hevc"
                                    >
                                        "Ouvrir l'éditeur visuel (Canvas)"
                                    </button>
                                    <button
                                        type="button"
                                        class="btn btn-secondary btn-sm set-default-flow-btn"
                                        data-flow="flow.transcode.hevc"
                                    >
                                        "Définir par défaut"
                                    </button>
                                </div>
                            </div>
                        </article>

                        // Flow 3 : Optimisation Web MP4 FastStart
                        <article class="flow-card" data-flow-id="flow.remux.mp4">
                            <div class="flow-card-header">
                                <div>
                                    <div style="display: flex; align-items: center; gap: 0.5rem;">
                                        <h3 class="flow-card-title">"Optimisation Web MP4 (FastStart)"</h3>
                                        <span class="status-pill info">"Streaming direct"</span>
                                    </div>
                                    <p class="flow-card-desc">
                                        "Convertit le conteneur en MP4 avec optimisation FastStart (moov atom en début de fichier) via MP4Box."
                                    </p>
                                </div>
                            </div>

                            <div class="flow-pipeline-container" aria-label="Chaîne des blocs du flux">
                                <div class="flow-pipeline">
                                    <div class="flow-block-step implicit-step">
                                        <span class="step-badge">"1"</span>
                                        <span class="step-name">"🔍 Inspecter (MediaInfo)"</span>
                                        <span class="step-type">"Implicite"</span>
                                    </div>
                                    <span class="flow-arrow" aria-hidden="true">"➔"</span>
                                    <div class="flow-block-step">
                                        <span class="step-badge">"2"</span>
                                        <span class="step-name">"📦 Remux MP4 (MP4Box)"</span>
                                        <span class="step-type">"FastStart: activé • Hint: false"</span>
                                    </div>
                                </div>
                            </div>

                            <div class="flow-card-footer">
                                <span class="flow-pattern-tag">"Masque : *.mp4, *.m4v, *.mov"</span>
                                <div style="display: flex; gap: 0.5rem;">
                                    <button
                                        type="button"
                                        class="btn btn-primary btn-sm open-canvas-btn"
                                        data-flow="flow.remux.mp4"
                                    >
                                        "Ouvrir l'éditeur visuel (Canvas)"
                                    </button>
                                </div>
                            </div>
                        </article>
                    </div>
                </section>
            </div>

            // --- Vue 2 : Plan de travail visuel (Canvas No-Code / Zapier style) ---
            <div id="flows-canvas-view" style="display: none;">
                <div class="canvas-header-bar">
                    <div style="display: flex; align-items: center; gap: 0.75rem;">
                        <button
                            type="button"
                            id="btn-back-to-library"
                            class="btn btn-secondary btn-sm"
                            aria-label="Retourner à la liste des flux"
                        >
                            "← Retour aux flux"
                        </button>
                        <h2 id="canvas-flow-title" class="section-title" style="margin: 0; font-size: 1.25rem;">
                            "Éditeur visuel : Transcodage HEVC"
                        </h2>
                        <span class="status-pill success">"Mode Édition"</span>
                    </div>
                    <div style="display: flex; gap: 0.5rem;">
                        <button
                            type="button"
                            id="btn-canvas-save-flow"
                            class="btn btn-primary btn-sm"
                        >
                            "💾 Enregistrer les modifications"
                        </button>
                    </div>
                </div>

                <div class="canvas-workspace-layout">
                    // Volet latéral gauche : Bibliothèque de blocs disponibles
                    <aside class="canvas-blocks-sidebar" aria-label="Bibliothèque de blocs disponibles">
                        <div class="sidebar-block-group">
                            <span class="sidebar-group-title">"🔍 Analyse technique"</span>
                            <div class="draggable-block-item" data-block-type="inspect" data-tool="mediainfo">
                                <span class="block-drag-icon">"🔍"</span>
                                <div class="block-drag-info">
                                    <strong>"Inspecter métadonnées"</strong>
                                    <span>"MediaInfo CLI • Implicite"</span>
                                </div>
                            </div>
                        </div>

                        <div class="sidebar-block-group">
                            <span class="sidebar-group-title">"🎞️ Traitement Vidéo"</span>
                            <div class="draggable-block-item addable" data-block-type="video-encode" data-tool="ffmpeg">
                                <span class="block-drag-icon">"🎞️"</span>
                                <div class="block-drag-info">
                                    <strong>"Réencoder Vidéo"</strong>
                                    <span>"FFmpeg (HEVC/QSV/NVENC)"</span>
                                </div>
                                <span class="block-add-plus">"+"</span>
                            </div>
                            <div class="draggable-block-item addable" data-block-type="remux-mkv" data-tool="mkvtoolnix">
                                <span class="block-drag-icon">"🎬"</span>
                                <div class="block-drag-info">
                                    <strong>"Remuxer MKV"</strong>
                                    <span>"MKVToolNix (mkvmerge)"</span>
                                </div>
                                <span class="block-add-plus">"+"</span>
                            </div>
                            <div class="draggable-block-item addable" data-block-type="remux-mp4" data-tool="mp4box">
                                <span class="block-drag-icon">"📦"</span>
                                <div class="block-drag-info">
                                    <strong>"Remuxer MP4"</strong>
                                    <span>"MP4Box GPAC (FastStart)"</span>
                                </div>
                                <span class="block-add-plus">"+"</span>
                            </div>
                        </div>

                        <div class="sidebar-block-group">
                            <span class="sidebar-group-title">"🔊 Traitement Audio"</span>
                            <div class="draggable-block-item addable" data-block-type="audio-encode" data-tool="ffmpeg">
                                <span class="block-drag-icon">"🔊"</span>
                                <div class="block-drag-info">
                                    <strong>"Réencoder Audio"</strong>
                                    <span>"FFmpeg (AAC/Opus/AC3)"</span>
                                </div>
                                <span class="block-add-plus">"+"</span>
                            </div>
                        </div>

                        <div class="sidebar-help-tip">
                            "💡 Cliquez sur un bloc avec « + » pour l'ajouter à la suite de votre chaîne."
                        </div>
                    </aside>

                    // Zone centrale : Plan de travail graphique (Canvas)
                    <div class="canvas-board-wrapper" id="canvas-board-area" tabindex="0" role="region" aria-label="Plan de travail graphique du flux">
                        <svg class="canvas-connections-svg" id="canvas-svg-lines" aria-hidden="true">
                            // Tracés SVG de liaison générés dynamiquement
                        </svg>

                        <div class="canvas-nodes-container" id="canvas-nodes-container">
                            // Nœud 1 : MediaInfo (Inspecter)
                            <div class="canvas-node implicit-node selected" id="node-1" data-node-id="node-1" data-block-type="inspect">
                                <div class="node-connector connector-out"></div>
                                <div class="node-header">
                                    <span class="node-icon">"🔍"</span>
                                    <div class="node-title-box">
                                        <span class="node-kind">"MediaInfo • Début de flux"</span>
                                        <h4 class="node-name">"Inspecter métadonnées"</h4>
                                    </div>
                                    <span class="status-pill info node-pill">"Implicite"</span>
                                </div>
                                <div class="node-summary">
                                    "Extraction exhaustive (vidéo, pistes audio, sous-titres, chapitres)."
                                </div>
                            </div>

                            // Nœud 2 : FFmpeg Video Encode
                            <div class="canvas-node" id="node-2" data-node-id="node-2" data-block-type="video-encode">
                                <div class="node-connector connector-in"></div>
                                <div class="node-connector connector-out"></div>
                                <div class="node-header">
                                    <span class="node-icon">"🎞️"</span>
                                    <div class="node-title-box">
                                        <span class="node-kind">"FFmpeg Builder"</span>
                                        <h4 class="node-name">"Réencodage Vidéo"</h4>
                                    </div>
                                    <button type="button" class="node-remove-btn" title="Supprimer ce bloc">"✕"</button>
                                </div>
                                <div class="node-summary" id="node-2-summary">
                                    "Codec : HEVC • Encodeur : Intel QSV • Qualité : 6 • Vitesse : 3"
                                </div>
                            </div>

                            // Nœud 3 : FFmpeg Audio Encode
                            <div class="canvas-node" id="node-3" data-node-id="node-3" data-block-type="audio-encode">
                                <div class="node-connector connector-in"></div>
                                <div class="node-connector connector-out"></div>
                                <div class="node-header">
                                    <span class="node-icon">"🔊"</span>
                                    <div class="node-title-box">
                                        <span class="node-kind">"FFmpeg Builder"</span>
                                        <h4 class="node-name">"Réencodage Audio"</h4>
                                    </div>
                                    <button type="button" class="node-remove-btn" title="Supprimer ce bloc">"✕"</button>
                                </div>
                                <div class="node-summary" id="node-3-summary">
                                    "Codec : AAC • Débit : 192 kbps • Canaux : Stéréo"
                                </div>
                            </div>

                            // Nœud 4 : MKVToolNix Remux
                            <div class="canvas-node" id="node-4" data-node-id="node-4" data-block-type="remux-mkv">
                                <div class="node-connector connector-in"></div>
                                <div class="node-header">
                                    <span class="node-icon">"🎬"</span>
                                    <div class="node-title-box">
                                        <span class="node-kind">"MKVToolNix"</span>
                                        <h4 class="node-name">"Remuxage Matroska"</h4>
                                    </div>
                                    <button type="button" class="node-remove-btn" title="Supprimer ce bloc">"✕"</button>
                                </div>
                                <div class="node-summary" id="node-4-summary">
                                    "Assemblage final MKV • Langues conservées : fre, eng"
                                </div>
                            </div>
                        </div>
                    </div>

                    // Volet latéral droit : Inspecteur et configuration du nœud sélectionné
                    <aside class="canvas-inspector-sidebar" id="canvas-inspector" aria-label="Paramètres du bloc sélectionné">
                        <div class="inspector-header">
                            <h3 id="inspector-block-title" class="inspector-title">"Paramètres du bloc"</h3>
                            <span id="inspector-block-tool" class="status-pill info">"FFmpeg"</span>
                        </div>

                        // Formulaire dynamique injecté selon le bloc sélectionné
                        <div id="inspector-form-container">
                            <div class="form-group">
                                <label for="node-cfg-codec" class="form-label">"Codec vidéo"</label>
                                <select id="node-cfg-codec" class="form-select">
                                    <option value="hevc" selected="selected">"HEVC / H.265 (Recommandé)"</option>
                                    <option value="h264">"H.264 / AVC"</option>
                                    <option value="av1">"AV1"</option>
                                    <option value="copy">"Copie directe"</option>
                                </select>
                            </div>
                            <div class="form-group">
                                <label for="node-cfg-encoder" class="form-label">"Encodeur matériel"</label>
                                <select id="node-cfg-encoder" class="form-select">
                                    <option value="auto" selected="selected">"Intel QSV (Automatique)"</option>
                                    <option value="nvenc">"Nvidia NVENC"</option>
                                    <option value="vaapi">"VA-API (Linux)"</option>
                                    <option value="cpu">"Processeur CPU (libx265)"</option>
                                </select>
                            </div>
                            <div class="form-group">
                                <label for="node-cfg-quality" class="form-label">
                                    "Qualité (1-9) : " <span id="node-quality-val" style="font-weight: 700; color: var(--primary);">"6"</span>
                                </label>
                                <input type="range" id="node-cfg-quality" min="1" max="9" value="6" style="width: 100%; accent-color: var(--primary);" />
                            </div>
                            <div class="form-group">
                                <label for="node-cfg-speed" class="form-label">
                                    "Vitesse (1-5) : " <span id="node-speed-val" style="font-weight: 700; color: var(--primary);">"3"</span>
                                </label>
                                <input type="range" id="node-cfg-speed" min="1" max="5" value="3" style="width: 100%; accent-color: var(--primary);" />
                            </div>
                            <div class="form-group">
                                <label for="node-cfg-custom" class="form-label">"Paramètres personnalisés libres"</label>
                                <input type="text" id="node-cfg-custom" class="form-input" placeholder="ex: -pix_fmt yuv420p10le" />
                            </div>
                        </div>
                    </aside>
                </div>
            </div>

            // --- Dialogue modal : Assistant de création guidée (Setup Wizard) ---
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
                        <div class="wizard-template-card active" data-template="hevc">
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
                                <p class="template-desc">"Démarre avec le bloc d'inspection initial pour composer librement vos blocs sur le canvas."</p>
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
        </div>
    }
}
