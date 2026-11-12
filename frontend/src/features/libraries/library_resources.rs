//! Synthèse des Flows et Plugins associés aux bibliothèques de traitement.

use leptos::prelude::*;

#[component]
pub fn LibraryResources() -> impl IntoView {
    view! {
        <section class="section-panel" aria-labelledby="resources-section-title" style="margin-top: 1.5rem;">
            <div class="section-panel-header">
                <div>
                    <h2 id="resources-section-title" class="section-title">
                        "Flows & Plugins associés aux Bibliothèques"
                    </h2>
                    <p class="section-subtitle">
                        "Vue d'ensemble des chaînes de traitement automatisées et des outils média mobilisés."
                    </p>
                </div>
            </div>

            <div style="display: grid; grid-template-columns: repeat(auto-fit, minmax(300px, 1fr)); gap: 1rem;">
                // Fiche Flow assigné
                <div class="card" style="padding: 1rem; border-left: 4px solid var(--primary);">
                    <div style="display: flex; justify-content: space-between; align-items: center; margin-bottom: 0.5rem;">
                        <h3 style="margin: 0; font-size: 1rem; font-weight: 600;">
                            "⚡ Pipelines d'ingestion (Flows)"
                        </h3>
                        <span class="status-pill success">"2 actifs"</span>
                    </div>
                    <ul style="margin: 0; padding-left: 1.25rem; font-size: 0.875rem; color: var(--text-color); line-height: 1.6;">
                        <li>
                            <strong>"Remuxage Matroska propre"</strong>
                            " : appliqué à la Bibliothèque Principale"
                        </li>
                        <li>
                            <strong>"Extraction & Normalisation R128"</strong>
                            " : appliqué à la Médiathèque Musique"
                        </li>
                    </ul>
                </div>

                // Fiche Plugins requis
                <div class="card" style="padding: 1rem; border-left: 4px solid #10b981;">
                    <div style="display: flex; justify-content: space-between; align-items: center; margin-bottom: 0.5rem;">
                        <h3 style="margin: 0; font-size: 1rem; font-weight: 600;">
                            "🔌 Plugins & Outils système requis"
                        </h3>
                        <span class="status-pill info">"3 connectés"</span>
                    </div>
                    <ul style="margin: 0; padding-left: 1.25rem; font-size: 0.875rem; color: var(--text-color); line-height: 1.6;">
                        <li>
                            <code>"app.internal.telemetry"</code>
                            " : Sonde système coordinateur"
                        </li>
                        <li>
                            <code>"app.mkvtoolnix.mkvmerge"</code>
                            " : Moteur de remuxage sans transcodage"
                        </li>
                        <li>
                            <code>"app.ffmpeg.mediainfo"</code>
                            " : Détection et inspection des métadonnées"
                        </li>
                    </ul>
                </div>
            </div>
        </section>
    }
}
