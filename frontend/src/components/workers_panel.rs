//! Panneau de supervision des serveurs et agents de remuxage.

use leptos::prelude::*;

#[component]
pub fn WorkersPanel() -> impl IntoView {
    view! {
        <div id="panel-workers" class="tab-panel" role="tabpanel" aria-labelledby="tab-btn-workers" hidden>
            <section class="section-panel" aria-labelledby="workers-section-title">
                <div class="section-panel-header">
                    <div>
                        <h2 id="workers-section-title" class="section-title">"Serveurs et agents de traitement"</h2>
                        <p class="section-subtitle">
                            "Agents de calcul enregistrés via l'API. Chaque agent traite les baux sur MKVToolNix sans connexion directe à PostgreSQL."
                        </p>
                    </div>
                </div>

                <div class="table-wrapper">
                    <table class="data-table" aria-label="Liste des agents et serveurs de traitement">
                        <thead>
                            <tr>
                                <th scope="col">"Identifiant de l'agent"</th>
                                <th scope="col">"Nom d'hôte / Rôle"</th>
                                <th scope="col">"Dernière activité"</th>
                                <th scope="col">"Statut"</th>
                            </tr>
                        </thead>
                        <tbody id="workers-table-body">
                            <tr>
                                <td colspan="4" class="empty-state-text">
                                    "Chargement des serveurs et agents…"
                                </td>
                            </tr>
                        </tbody>
                    </table>
                </div>

                <div style="margin-top: 1.5rem; display: grid; grid-template-columns: repeat(auto-fit, minmax(280px, 1fr)); gap: 1rem;">
                    <div class="metric-card">
                        <div class="metric-label">"Architecture distribuée"</div>
                        <p style="font-size: 0.875rem; color: var(--text-dim); margin-top: 0.25rem;">
                            "Mode centralisé de 1 à N serveurs. Le coordinateur central gère les métadonnées et distribue les tâches avec des baux de 120 s renouvelés toutes les 20 s."
                        </p>
                    </div>
                    <div class="metric-card">
                        <div class="metric-label">"Haute disponibilité"</div>
                        <p style="font-size: 0.875rem; color: var(--text-dim); margin-top: 0.25rem;">
                            "La haute disponibilité reste optionnelle et nécessite un cadrage distinct ; aucun seuil de nœuds ne l'active automatiquement."
                        </p>
                    </div>
                </div>
            </section>
        </div>
    }
}
