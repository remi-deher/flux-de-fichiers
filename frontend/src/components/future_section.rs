//! Composant décrivant honnêtement les fonctionnalités futures et limites actuelles.

use leptos::prelude::*;

#[component]
pub fn FutureSection() -> impl IntoView {
    view! {
        <section class="section-panel" aria-labelledby="future-section-title">
            <div class="section-panel-header">
                <div>
                    <h2 id="future-section-title" class="section-title">
                        "Évolutions et fonctionnalités futures"
                    </h2>
                    <p class="section-subtitle">
                        "État honnête du périmètre actuel : aucun bouton trompeur ni fausse télémétrie."
                    </p>
                </div>
            </div>
            <div style="display: grid; grid-template-columns: repeat(auto-fit, minmax(280px, 1fr)); gap: 1rem;">
                <div class="future-card">
                    <span class="badge-future">"Hors périmètre actuel"</span>
                    <h3 style="font-size: 1rem; font-weight: 700; margin: 0.5rem 0 0.25rem 0;">
                        "Éditeur visuel de flux"
                    </h3>
                    <p class="metric-desc">
                        "Le catalogue et l'éditeur graphique de pipelines de fichiers font l'objet d'un cadrage ultérieur dans le dépôt flux-de-fichiers-plugins."
                    </p>
                </div>
                <div class="future-card">
                    <span class="badge-future">"En réserve"</span>
                    <h3 style="font-size: 1rem; font-weight: 700; margin: 0.5rem 0 0.25rem 0;">
                        "Connecteurs de stockage distants"
                    </h3>
                    <p class="metric-desc">
                        "L'intégration directe avec des partages SMB ou compartiments S3 sera déployée lors des phases multi-machines. Actuellement, les échanges s'effectuent via les volumes Docker."
                    </p>
                </div>
                <div class="future-card">
                    <span class="badge-future">"Règle de sécurité"</span>
                    <h3 style="font-size: 1rem; font-weight: 700; margin: 0.5rem 0 0.25rem 0;">
                        "Purge automatique non destructive"
                    </h3>
                    <p class="metric-desc">
                        "Conformément à AGENTS.md, aucune suppression automatique destructive n'est déclenchée sur les répertoires de staging ou de données."
                    </p>
                </div>
            </div>
        </section>
    }
}
