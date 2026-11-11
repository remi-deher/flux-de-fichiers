//! Catalogue et bibliothèque des flux configurés regroupés par type de média (Rôle Construire).

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
                            "Pipelines multimédias modulaires classés par famille de traitement (Vidéo, Audio, Image, Système)."
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

                // Filtres par type de flux (inspiré d'encoding-central)
                <div class="filter-bar" style="display: flex; gap: 0.5rem; flex-wrap: wrap; margin-bottom: 1.25rem;" role="toolbar" aria-label="Filtrer les flux par catégorie de média">
                    <button
                        type="button"
                        class="btn btn-secondary btn-sm flow-type-chip active"
                        data-flow-filter="all"
                        aria-pressed="true"
                    >
                        "Tous les types (6)"
                    </button>
                    <button
                        type="button"
                        class="btn btn-secondary btn-sm flow-type-chip"
                        data-flow-filter="video"
                        aria-pressed="false"
                    >
                        "🎬 Vidéo (3)"
                    </button>
                    <button
                        type="button"
                        class="btn btn-secondary btn-sm flow-type-chip"
                        data-flow-filter="audio"
                        aria-pressed="false"
                    >
                        "🎵 Musique & Audio (1)"
                    </button>
                    <button
                        type="button"
                        class="btn btn-secondary btn-sm flow-type-chip"
                        data-flow-filter="image"
                        aria-pressed="false"
                    >
                        "🖼️ Images & Vignettes (1)"
                    </button>
                    <button
                        type="button"
                        class="btn btn-secondary btn-sm flow-type-chip"
                        data-flow-filter="system"
                        aria-pressed="false"
                    >
                        "⚙️ Système & Outils (1)"
                    </button>
                </div>

                // Grille des flux existants
                <div class="flows-grid" id="flows-grid-container" aria-label="Liste des flux configurés">
                    // --- SECTION VIDÉO ---
                    // Flow 1 : Remux MKV par défaut
                    <article class="flow-card active-flow" data-flow-id="flow.default.mkv" data-flow-type="video">
                        <div class="flow-card-header">
                            <div>
                                <div style="display: flex; align-items: center; gap: 0.5rem;">
                                    <h3 class="flow-card-title">"Remuxage Matroska propre"</h3>
                                    <span class="status-pill success" title="Flux actif par défaut">
                                        <span class="status-dot"></span>
                                        "Défaut"
                                    </span>
                                    <span class="status-pill info">"🎬 Vidéo"</span>
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
                                    <span class="step-name">"📄 Source"</span>
                                    <span class="step-type">"Détection auto"</span>
                                </div>
                                <span class="flow-arrow" aria-hidden="true">"➔"</span>
                                <div class="flow-block-step">
                                    <span class="step-badge">"2"</span>
                                    <span class="step-name">"🎬 MKV Remux"</span>
                                    <span class="step-type">"Copie directe"</span>
                                </div>
                            </div>
                        </div>

                        <div class="flow-card-footer">
                            <span class="flow-pattern-tag">"Famille : Conteneurs Matroska"</span>
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
                    <article class="flow-card" data-flow-id="flow.transcode.hevc" data-flow-type="video">
                        <div class="flow-card-header">
                            <div>
                                <div style="display: flex; align-items: center; gap: 0.5rem;">
                                    <h3 class="flow-card-title">"Transcodage HEVC / Intel QSV & VA-API"</h3>
                                    <span class="status-pill info">"🎬 Vidéo"</span>
                                    <span class="status-pill">"Accélération GPU"</span>
                                </div>
                                <p class="flow-card-desc">
                                    "Réencodage vidéo haute efficacité HEVC avec paramètres macro-blocs et conversion audio AAC."
                                </p>
                            </div>
                        </div>

                        <div class="flow-pipeline-container" aria-label="Chaîne des blocs du flux">
                            <div class="flow-pipeline">
                                <div class="flow-block-step">
                                    <span class="step-badge">"1"</span>
                                    <span class="step-name">"📄 Source"</span>
                                    <span class="step-type">"Détection"</span>
                                </div>
                                <span class="flow-arrow" aria-hidden="true">"➔"</span>
                                <div class="flow-block-step">
                                    <span class="step-badge">"2"</span>
                                    <span class="step-name">"🎞️ Encodeur HEVC"</span>
                                    <span class="step-type">"QSV • Q: 6"</span>
                                </div>
                                <span class="flow-arrow" aria-hidden="true">"➔"</span>
                                <div class="flow-block-step">
                                    <span class="step-badge">"3"</span>
                                    <span class="step-name">"🔊 AAC Audio"</span>
                                    <span class="step-type">"192 kbps"</span>
                                </div>
                                <span class="flow-arrow" aria-hidden="true">"➔"</span>
                                <div class="flow-block-step">
                                    <span class="step-badge">"4"</span>
                                    <span class="step-name">"🎬 MKV Remux"</span>
                                    <span class="step-type">"Assemblage"</span>
                                </div>
                            </div>
                        </div>

                        <div class="flow-card-footer">
                            <span class="flow-pattern-tag">"Famille : Compression haute efficacité"</span>
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

                    // Flow 3 : Aiguillage conditionnel intelligent & Exécution In-Place (ADR-0008)
                    <article class="flow-card" data-flow-id="flow.smart.adr0008" data-flow-type="video">
                        <div class="flow-card-header">
                            <div>
                                <div style="display: flex; align-items: center; gap: 0.5rem; flex-wrap: wrap;">
                                    <h3 class="flow-card-title">"Aiguillage conditionnel & In-Place (ADR-0008)"</h3>
                                    <span class="status-pill info">"🎬 Vidéo"</span>
                                    <span class="status-pill success">"Tri-sorties & Zéro copie"</span>
                                </div>
                                <p class="flow-card-desc">
                                    "Inspecte automatiquement le média : si déjà HEVC, applique les drapeaux en-têtes directement en place sans copie ; sinon transcode par GPU avec repli CPU."
                                </p>
                            </div>
                        </div>

                        <div class="flow-pipeline-container" aria-label="Chaîne des blocs du flux ADR-0008">
                            <div class="flow-pipeline">
                                <div class="flow-block-step">
                                    <span class="step-badge">"1"</span>
                                    <span class="step-name">"📄 Source"</span>
                                    <span class="step-type">"MediaInfo"</span>
                                </div>
                                <span class="flow-arrow" aria-hidden="true">"➔"</span>
                                <div class="flow-block-step condition-step">
                                    <span class="step-badge" style="background: var(--warning-border);">"2"</span>
                                    <span class="step-name">"🔀 Codec == HEVC ?"</span>
                                    <span class="step-type">"Condition"</span>
                                </div>
                                <span class="flow-arrow" aria-hidden="true">"➔"</span>
                                <div class="flow-block-step in-place-step">
                                    <span class="step-badge" style="background: #10b981;">"✓"</span>
                                    <span class="step-name">"⚡ In-Place"</span>
                                    <span class="step-type">"En-têtes directes"</span>
                                </div>
                            </div>
                        </div>

                        <div class="flow-card-footer">
                            <span class="flow-pattern-tag">"Famille : Optimisation adaptative"</span>
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

                    // --- SECTION AUDIO & MUSIQUE ---
                    // Flow 4 : Extraction & Normalisation EBU R128
                    <article class="flow-card" data-flow-id="flow.audio.r128" data-flow-type="audio">
                        <div class="flow-card-header">
                            <div>
                                <div style="display: flex; align-items: center; gap: 0.5rem;">
                                    <h3 class="flow-card-title">"Extraction & Normalisation Audio EBU R128"</h3>
                                    <span class="status-pill" style="background: #3b2d54; color: #d8b4fe;">"🎵 Musique & Audio"</span>
                                    <span class="status-pill success">"Loudness -23 LUFS"</span>
                                </div>
                                <p class="flow-card-desc">
                                    "Extrait les pistes musicales, applique une normalisation de volume conforme au standard EBU R128 et encode en FLAC ou AAC haute fidélité."
                                </p>
                            </div>
                        </div>

                        <div class="flow-pipeline-container" aria-label="Chaîne des blocs du flux audio">
                            <div class="flow-pipeline">
                                <div class="flow-block-step">
                                    <span class="step-badge">"1"</span>
                                    <span class="step-name">"📄 Source audio"</span>
                                    <span class="step-type">"Détection flux"</span>
                                </div>
                                <span class="flow-arrow" aria-hidden="true">"➔"</span>
                                <div class="flow-block-step">
                                    <span class="step-badge">"2"</span>
                                    <span class="step-name">"🎚️ Normalisation R128"</span>
                                    <span class="step-type">"Target: -23 LUFS"</span>
                                </div>
                                <span class="flow-arrow" aria-hidden="true">"➔"</span>
                                <div class="flow-block-step">
                                    <span class="step-badge">"3"</span>
                                    <span class="step-name">"🎼 Encodage FLAC"</span>
                                    <span class="step-type">"24-bit • Sans perte"</span>
                                </div>
                            </div>
                        </div>

                        <div class="flow-card-footer">
                            <span class="flow-pattern-tag">"Famille : Traitement audio & Discographie"</span>
                            <div style="display: flex; gap: 0.5rem;">
                                <button
                                    type="button"
                                    class="btn btn-primary btn-sm open-canvas-btn"
                                    data-flow="flow.audio.r128"
                                >
                                    "Ouvrir l'éditeur visuel (Canvas)"
                                </button>
                            </div>
                        </div>
                    </article>

                    // --- SECTION IMAGE & VIGNETTES ---
                    // Flow 5 : Génération de miniatures et posters WebP
                    <article class="flow-card" data-flow-id="flow.image.thumbnails" data-flow-type="image">
                        <div class="flow-card-header">
                            <div>
                                <div style="display: flex; align-items: center; gap: 0.5rem;">
                                    <h3 class="flow-card-title">"Miniatures chapitrées & Posters WebP"</h3>
                                    <span class="status-pill" style="background: #14382d; color: #6ee7b7;">"🖼️ Images"</span>
                                    <span class="status-pill info">"WebP / AVIF"</span>
                                </div>
                                <p class="flow-card-desc">
                                    "Génère automatiquement une planche de contact visuelle et extrait les affiches d'illustrations optimisées pour interfaces légères."
                                </p>
                            </div>
                        </div>

                        <div class="flow-pipeline-container" aria-label="Chaîne des blocs du flux image">
                            <div class="flow-pipeline">
                                <div class="flow-block-step">
                                    <span class="step-badge">"1"</span>
                                    <span class="step-name">"📄 Vidéo / Image"</span>
                                    <span class="step-type">"Analyse chapitres"</span>
                                </div>
                                <span class="flow-arrow" aria-hidden="true">"➔"</span>
                                <div class="flow-block-step">
                                    <span class="step-badge">"2"</span>
                                    <span class="step-name">"📸 Capture d'images"</span>
                                    <span class="step-type">"Grille 4x4"</span>
                                </div>
                                <span class="flow-arrow" aria-hidden="true">"➔"</span>
                                <div class="flow-block-step">
                                    <span class="step-badge">"3"</span>
                                    <span class="step-name">"🖼️ Export WebP"</span>
                                    <span class="step-type">"Qualité: 85%"</span>
                                </div>
                            </div>
                        </div>

                        <div class="flow-card-footer">
                            <span class="flow-pattern-tag">"Famille : Galerie & Métadonnées visuelles"</span>
                            <div style="display: flex; gap: 0.5rem;">
                                <button
                                    type="button"
                                    class="btn btn-primary btn-sm open-canvas-btn"
                                    data-flow="flow.image.thumbnails"
                                >
                                    "Ouvrir l'éditeur visuel (Canvas)"
                                </button>
                            </div>
                        </div>
                    </article>

                    // --- SECTION SYSTÈME & TÉLÉMÉTRIE ---
                    // Flow 6 : Sonde système & Archivage
                    <article class="flow-card" data-flow-id="flow.system.telemetry" data-flow-type="system">
                        <div class="flow-card-header">
                            <div>
                                <div style="display: flex; align-items: center; gap: 0.5rem;">
                                    <h3 class="flow-card-title">"Sonde de télémétrie & Audit d'intégrité"</h3>
                                    <span class="status-pill warning">"⚙️ Système"</span>
                                    <span class="status-pill">"Lecture seule"</span>
                                </div>
                                <p class="flow-card-desc">
                                    "Audit complet des flux sans altération du média : mesure de débit binaire, empreinte SHA-256 et remontée métrique coordinateur."
                                </p>
                            </div>
                        </div>

                        <div class="flow-pipeline-container" aria-label="Chaîne des blocs du flux système">
                            <div class="flow-pipeline">
                                <div class="flow-block-step">
                                    <span class="step-badge">"1"</span>
                                    <span class="step-name">"📄 Source"</span>
                                    <span class="step-type">"Tous médias"</span>
                                </div>
                                <span class="flow-arrow" aria-hidden="true">"➔"</span>
                                <div class="flow-block-step">
                                    <span class="step-badge">"2"</span>
                                    <span class="step-name">"📊 Sonde système"</span>
                                    <span class="step-type">"tool.internal.telemetry"</span>
                                </div>
                                <span class="flow-arrow" aria-hidden="true">"➔"</span>
                                <div class="flow-block-step">
                                    <span class="step-badge">"3"</span>
                                    <span class="step-name">"🛡️ Audit SQLite"</span>
                                    <span class="step-type">"Journalisation WAL"</span>
                                </div>
                            </div>
                        </div>

                        <div class="flow-card-footer">
                            <span class="flow-pattern-tag">"Famille : Supervision & Diagnostic"</span>
                            <div style="display: flex; gap: 0.5rem;">
                                <button
                                    type="button"
                                    class="btn btn-primary btn-sm open-canvas-btn"
                                    data-flow="flow.system.telemetry"
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
