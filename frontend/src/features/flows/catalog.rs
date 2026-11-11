//! Catalogue et bibliothèque des flux réels persistés en base de données.

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
                            "Pipelines multimédias modulaires persistés dans la base SQLite (/config/flux.db)."
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

                // Filtres par type de flux réels
                <div class="filter-bar" style="display: flex; gap: 0.5rem; flex-wrap: wrap; margin-bottom: 1.25rem;" role="toolbar" aria-label="Filtrer les flux par catégorie de média">
                    <button
                        type="button"
                        class="btn btn-secondary btn-sm flow-type-chip active"
                        data-flow-filter="all"
                        aria-pressed="true"
                    >
                        "Tous les flux (3)"
                    </button>
                    <button
                        type="button"
                        class="btn btn-secondary btn-sm flow-type-chip"
                        data-flow-filter="video"
                        aria-pressed="false"
                    >
                        "🎬 Vidéo & Remux (3)"
                    </button>
                </div>

                // Grille des flux réels
                <div class="flows-grid" id="flows-grid-container" aria-label="Liste des flux configurés">
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
                                <span class="flow-arrow" aria-hidden="true">"➔"</span>
                                <div class="flow-block-step">
                                    <span class="step-badge">"3"</span>
                                    <span class="step-name">"📁 Destination"</span>
                                    <span class="step-type">"/output"</span>
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

                    // Flow 2 : Aiguillage conditionnel intelligent & Exécution In-Place (ADR-0008)
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

                    // Flow 3 : Optimisation Web MP4 (FastStart)
                    <article class="flow-card" data-flow-id="flow.remux.mp4" data-flow-type="video">
                        <div class="flow-card-header">
                            <div>
                                <div style="display: flex; align-items: center; gap: 0.5rem;">
                                    <h3 class="flow-card-title">"Optimisation Web MP4 (FastStart)"</h3>
                                    <span class="status-pill info">"🎬 Vidéo"</span>
                                    <span class="status-pill">"MP4Box"</span>
                                </div>
                                <p class="flow-card-desc">
                                    "Convertit le conteneur en MP4 avec optimisation FastStart (moov atom en début de fichier) via MP4Box pour le streaming web."
                                </p>
                            </div>
                        </div>

                        <div class="flow-pipeline-container" aria-label="Chaîne des blocs du flux MP4">
                            <div class="flow-pipeline">
                                <div class="flow-block-step">
                                    <span class="step-badge">"1"</span>
                                    <span class="step-name">"📄 Source"</span>
                                    <span class="step-type">"Fichier source"</span>
                                </div>
                                <span class="flow-arrow" aria-hidden="true">"➔"</span>
                                <div class="flow-block-step">
                                    <span class="step-badge">"2"</span>
                                    <span class="step-name">"📦 MP4 FastStart"</span>
                                    <span class="step-type">"Remux MP4Box"</span>
                                </div>
                                <span class="flow-arrow" aria-hidden="true">"➔"</span>
                                <div class="flow-block-step">
                                    <span class="step-badge">"3"</span>
                                    <span class="step-name">"📁 Sortie"</span>
                                    <span class="step-type">"/output"</span>
                                </div>
                            </div>
                        </div>

                        <div class="flow-card-footer">
                            <span class="flow-pattern-tag">"Famille : Conteneurs MP4"</span>
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
    }
}
