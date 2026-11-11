//! Domaine Réglages : Configuration réseau et points d'accès.

use leptos::prelude::*;

#[component]
pub fn SettingsNetwork() -> impl IntoView {
    view! {
        <section class="section-panel" aria-labelledby="network-settings-title">
            <div class="section-panel-header">
                <div>
                    <h2 id="network-settings-title" class="section-title">
                        "Réseau & Points d'accès"
                    </h2>
                    <p class="section-subtitle">
                        "Configuration du serveur HTTP central et connectivité API."
                    </p>
                </div>
            </div>

            <div class="detail-grid">
                <div class="detail-term">"URL d'accès web"</div>
                <div class="detail-val">"http://localhost:3000"</div>

                <div class="detail-term">"Port HTTP unifié"</div>
                <div class="detail-val">"3000 (interface Leptos + API REST)"</div>

                <div class="detail-term">"API REST unifiée"</div>
                <div class="detail-val">"/api/v1/ (same-origin, authentifié par en-tête ou cookie)"</div>

                <div class="detail-term">"Contrat d'API"</div>
                <div class="detail-val">"OpenAPI 3.1 • docs/openapi.yaml"</div>

                <div class="detail-term">"Proxy inverse"</div>
                <div class="detail-val">"Compatible Caddy / Traefik / Nginx (en-têtes X-Forwarded-* respectés)"</div>
            </div>
        </section>
    }
}
