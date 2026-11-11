//! Catalogue dynamique des flux de traitement (moteur nu Bare Core).

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
                            "Pipelines multimédias modulaires distribués via le catalogue de plugins et synchronisés en base locale."
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

                // Barre de filtrage dynamique
                <div class="filter-bar" style="display: flex; gap: 0.5rem; flex-wrap: wrap; margin-bottom: 1.25rem;" role="toolbar" aria-label="Filtrer les flux par catégorie de média">
                    <button
                        type="button"
                        class="btn btn-secondary btn-sm flow-type-chip active"
                        data-flow-filter="all"
                        aria-pressed="true"
                    >
                        "Tous les flux ("<span id="flows-count-all">"0"</span>")"
                    </button>
                    <button
                        type="button"
                        class="btn btn-secondary btn-sm flow-type-chip"
                        data-flow-filter="video"
                        aria-pressed="false"
                    >
                        "🎬 Vidéo ("<span id="flows-count-video">"0"</span>")"
                    </button>
                    <button
                        type="button"
                        class="btn btn-secondary btn-sm flow-type-chip"
                        data-flow-filter="other"
                        aria-pressed="false"
                    >
                        "⚙️ Autres ("<span id="flows-count-other">"0"</span>")"
                    </button>
                </div>

                // Grille dynamique des flux
                <div class="flows-grid" id="flows-grid-container" aria-label="Liste des flux configurés">
                    <div class="empty-state-panel" id="flows-empty-state" style="grid-column: 1 / -1; text-align: center; padding: 2.5rem 1rem; border: 1px dashed var(--border-color); border-radius: var(--radius-md); background: var(--bg-card);">
                        <div style="font-size: 2rem; margin-bottom: 0.5rem;" aria-hidden="true">"⚡"</div>
                        <h3 style="margin: 0 0 0.5rem 0; font-size: 1.125rem; font-weight: 600;">"Aucun flux de traitement installé"</h3>
                        <p style="margin: 0 auto 1.25rem auto; max-width: 500px; font-size: 0.875rem; color: var(--text-muted);">
                            "L'application démarre en moteur nu (Bare Core). Importez des flux certifiés depuis le catalogue de plugins ou concevez un pipeline sur-mesure."
                        </p>
                        <div style="display: flex; gap: 0.75rem; justify-content: center; flex-wrap: wrap;">
                            <button type="button" id="btn-empty-import-flow" class="btn btn-secondary">
                                "🔌 Consulter le catalogue de plugins"
                            </button>
                            <button type="button" id="btn-empty-create-flow" class="btn btn-primary">
                                "+ Créer un flux (Assistant)"
                            </button>
                        </div>
                    </div>
                </div>
            </section>
        </div>
    }
}
