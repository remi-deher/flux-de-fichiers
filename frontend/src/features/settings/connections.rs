//! Domaine Réglages : Préférences d'affichage et intégrations d'outils.

use leptos::prelude::*;

#[component]
pub fn SettingsConnections() -> impl IntoView {
    view! {
        <section class="section-panel" aria-labelledby="theme-settings-title">
            <div class="section-panel-header">
                <div>
                    <h2 id="theme-settings-title" class="section-title">
                        "Préférences d'affichage"
                    </h2>
                    <p class="section-subtitle">
                        "Personnalisez l'apparence selon vos conditions de visionnage."
                    </p>
                </div>
            </div>
            <div class="form-group" style="max-width: 320px;">
                <label for="theme-select" class="form-label">
                    "Thème de couleur"
                </label>
                <select id="theme-select" class="form-select">
                    <option value="system">"Système (automatique)"</option>
                    <option value="light">"Clair (contrastes renforcés)"</option>
                    <option value="dark">"Sombre (haute lisibilité)"</option>
                </select>
                <p class="form-help">
                    "Respecte automatiquement le paramètre prefers-color-scheme de votre système d'exploitation."
                </p>
            </div>
        </section>

        <section class="section-panel" aria-labelledby="tools-settings-title">
            <div class="section-panel-header">
                <div>
                    <h2 id="tools-settings-title" class="section-title">
                        "Intégrations & Outils système"
                    </h2>
                    <p class="section-subtitle">
                        "Connecteurs de traitement et notifications d'événements."
                    </p>
                </div>
            </div>

            <div class="detail-grid">
                <div class="detail-term">"Moteur de remuxage"</div>
                <div class="detail-val">"MKVToolNix (mkvmerge) natif"</div>

                <div class="detail-term">"Diffusion temps réel"</div>
                <div class="detail-val">"Server-Sent Events (SSE) sur /api/v1/jobs/events"</div>

                <div class="detail-term">"Alertes externes"</div>
                <div class="detail-val">"Support webhooks HTTP sur fin de traitement (succès / échec)"</div>
            </div>
        </section>
    }
}
