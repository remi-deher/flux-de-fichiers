//! Domaine Réglages : intégrations d'outils (le thème vit dans la barre latérale).

use leptos::prelude::*;

#[component]
pub fn SettingsIntegrations() -> impl IntoView {
    view! {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_integrations_no_longer_carry_the_theme_choice() {
        let html = view! { <SettingsIntegrations /> }.to_html();
        assert!(html.contains("tools-settings-title"));
        assert!(!html.contains("theme-select"));
        assert!(!html.contains("theme-settings-title"));
    }
}
