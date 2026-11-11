//! Domaine Réglages : Stockage persistant et volumes montés.

use leptos::prelude::*;

#[component]
pub fn SettingsStorage() -> impl IntoView {
    view! {
        <section class="section-panel" aria-labelledby="storage-settings-title">
            <div class="section-panel-header">
                <div>
                    <h2 id="storage-settings-title" class="section-title">
                        "Stockage & Persistance"
                    </h2>
                    <p class="section-subtitle">
                        "Architecture de fichiers conteneurisée style *arr et base locale SQLite."
                    </p>
                </div>
            </div>

            <div class="detail-grid">
                <div class="detail-term">"Architecture système"</div>
                <div class="detail-val">"Modèle *arr conteneur unique autonome"</div>

                <div class="detail-term">"Base de données locale"</div>
                <div class="detail-val">"SQLite WAL (/config/flux.db) • Migrations automatiques SQLx"</div>

                <div class="detail-term">"Volume d'entrée (/input)"</div>
                <div class="detail-val">"Montage lecture seule des médias sources (intégrité garantie)"</div>

                <div class="detail-term">"Volume de sortie (/output)"</div>
                <div class="detail-val">"Écriture atomique des médias remuxés ou optimisés"</div>

                <div class="detail-term">"Volume de configuration (/config)"</div>
                <div class="detail-val">"Fichiers secrets, jetons d'amorçage et base SQLite"</div>

                <div class="detail-term">"Worker local intégré"</div>
                <div class="detail-val" id="settings-local-worker">"—"</div>

                <div class="detail-term">"Version applicative"</div>
                <div class="detail-val" id="settings-version">"—"</div>
            </div>
        </section>
    }
}
