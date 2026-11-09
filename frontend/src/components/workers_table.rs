//! Composant d'affichage des agents (workers) enregistrés.

use leptos::prelude::*;

#[component]
pub fn WorkersTable() -> impl IntoView {
    view! {
        <section class="section-panel" aria-labelledby="workers-section-title">
            <div class="section-panel-header">
                <div>
                    <h2 id="workers-section-title" class="section-title">"Agents de traitement (Workers)"</h2>
                    <p class="section-subtitle">
                        "Conteneurs MKVToolNix exécutant les baux de remuxage sans FFmpeg en production."
                    </p>
                </div>
            </div>
            <div class="table-wrapper">
                <table class="data-table" aria-label="Liste des agents de remuxage enregistrés">
                    <thead>
                        <tr>
                            <th scope="col">"UUID de l'agent"</th>
                            <th scope="col">"Nom d'enregistrement"</th>
                            <th scope="col">"Statut"</th>
                            <th scope="col">"Dernier signalement"</th>
                        </tr>
                    </thead>
                    <tbody id="workers-table-body">
                        <tr>
                            <td colspan="4" class="empty-state-text">
                                "Chargement des agents…"
                            </td>
                        </tr>
                    </tbody>
                </table>
            </div>
        </section>
    }
}
