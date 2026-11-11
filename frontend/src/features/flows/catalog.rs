//! Catalogue et bibliothèque des flux configurés (Rôle Construire).

use leptos::prelude::*;

#[component]
pub fn FlowCatalog() -> impl IntoView {
    view! {
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
                                <div class="flow-block-step">
                                    <span class="step-badge">"1"</span>
                                    <span class="step-name">"📄 Fichier source"</span>
                                    <span class="step-type">"Détection automatique"</span>
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
                                <div class="flow-block-step">
                                    <span class="step-badge">"1"</span>
                                    <span class="step-name">"📄 Fichier source"</span>
                                    <span class="step-type">"Détection automatique"</span>
                                </div>
                                <span class="flow-arrow" aria-hidden="true">"➔"</span>
                                <div class="flow-block-step">
                                    <span class="step-badge">"2"</span>
                                    <span class="step-name">"🎞️ Encodeur HEVC"</span>
                                    <span class="step-type">"Intel QSV • Qualité: 6"</span>
                                </div>
                                <span class="flow-arrow" aria-hidden="true">"➔"</span>
                                <div class="flow-block-step">
                                    <span class="step-badge">"3"</span>
                                    <span class="step-name">"🔊 AAC Audio"</span>
                                    <span class="step-type">"192 kbps • Stéréo"</span>
                                </div>
                                <span class="flow-arrow" aria-hidden="true">"➔"</span>
                                <div class="flow-block-step">
                                    <span class="step-badge">"4"</span>
                                    <span class="step-name">"🎬 MKV Remux"</span>
                                    <span class="step-type">"Assemblage final"</span>
                                </div>
                            </div>
                        </div>

                        <div class="flow-card-footer">
                            <span class="flow-pattern-tag">"Masque : *.mkv, *.mp4, *.m2ts"</span>
                            <div style="display: flex; gap: 0.5rem;">
                                <button
                                    type="button"
                                    class="btn btn-primary btn-sm open-canvas-btn"
                                    data-flow="flow.transcode.hevc"
                                >
                                    "Ouvrir l'éditeur visuel (Canvas)"
                                </button>
                            </div>
                        </div>
                    </article>

                    // Flow 3 : Optimisation FastStart MP4
                    <article class="flow-card" data-flow-id="flow.remux.mp4">
                        <div class="flow-card-header">
                            <div>
                                <div style="display: flex; align-items: center; gap: 0.5rem;">
                                    <h3 class="flow-card-title">"Optimisation Streaming MP4 FastStart"</h3>
                                    <span class="status-pill">"Web & Mobile"</span>
                                </div>
                                <p class="flow-card-desc">
                                    "Convertit le conteneur en MP4 avec optimisation FastStart (moov atom en début de fichier) via MP4Box."
                                </p>
                            </div>
                        </div>

                        <div class="flow-pipeline-container" aria-label="Chaîne des blocs du flux">
                            <div class="flow-pipeline">
                                <div class="flow-block-step">
                                    <span class="step-badge">"1"</span>
                                    <span class="step-name">"📄 Fichier source"</span>
                                    <span class="step-type">"Détection automatique"</span>
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

                    // Flow 4 : Aiguillage conditionnel intelligent & Exécution In-Place (ADR-0008)
                    <article class="flow-card" data-flow-id="flow.smart.adr0008">
                        <div class="flow-card-header">
                            <div>
                                <div style="display: flex; align-items: center; gap: 0.5rem; flex-wrap: wrap;">
                                    <h3 class="flow-card-title">"Aiguillage conditionnel & In-Place (ADR-0008)"</h3>
                                    <span class="status-pill success">"Tri-sorties & Zéro copie"</span>
                                </div>
                                <p class="flow-card-desc">
                                    "Inspecte automatiquement le média : si déjà HEVC, applique les drapeaux en-têtes directement en place (mkvpropedit) sans copie ; sinon transcode par GPU avec repli CPU garanti."
                                </p>
                            </div>
                        </div>

                        <div class="flow-pipeline-container" aria-label="Chaîne des blocs du flux ADR-0008">
                            <div class="flow-pipeline">
                                <div class="flow-block-step">
                                    <span class="step-badge">"1"</span>
                                    <span class="step-name">"📄 Détection source"</span>
                                    <span class="step-type">"MediaInfo implicite"</span>
                                </div>
                                <span class="flow-arrow" aria-hidden="true">"➔"</span>
                                <div class="flow-block-step condition-step">
                                    <span class="step-badge" style="background: var(--warning-border);">"2"</span>
                                    <span class="step-name">"🔀 Si Codec == HEVC ?"</span>
                                    <span class="step-type">"Aiguillage 3 sorties"</span>
                                </div>
                                <span class="flow-arrow" aria-hidden="true">"➔"</span>
                                <div class="flow-block-step in-place-step">
                                    <span class="step-badge" style="background: #10b981;">"✓"</span>
                                    <span class="step-name">"⚡ In-Place (mkvpropedit)"</span>
                                    <span class="step-type">"En-têtes directes sans /work"</span>
                                </div>
                                <span class="flow-arrow" aria-hidden="true">"ou"</span>
                                <div class="flow-block-step">
                                    <span class="step-badge" style="background: #f59e0b;">"✗"</span>
                                    <span class="step-name">"🎞️ Transcodage GPU"</span>
                                    <span class="step-type">"HEVC QSV (Repli: CPU)"</span>
                                </div>
                            </div>
                        </div>

                        <div class="flow-card-footer">
                            <span class="flow-pattern-tag">"Masque : *.mkv, *.mp4, *.m2ts, *.ts"</span>
                            <div style="display: flex; gap: 0.5rem;">
                                <button
                                    type="button"
                                    class="btn btn-primary btn-sm open-canvas-btn"
                                    data-flow="flow.smart.adr0008"
                                >
                                    "Ouvrir l'éditeur visuel (Canvas)"
                                </button>
                            </div>
                        </div>
                    </article>
                </div>
            </section>
        </div>
    }
}
