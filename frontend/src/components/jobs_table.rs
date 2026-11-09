//! Composant de suivi de la file de tâches.

use leptos::prelude::*;

#[component]
pub fn JobsTable() -> impl IntoView {
    view! {
        <section class="section-panel" aria-labelledby="jobs-section-title">
            <div class="section-panel-header">
                <div>
                    <h2 id="jobs-section-title" class="section-title">"File des tâches de remuxage"</h2>
                    <p class="section-subtitle">
                        "Suivi de l'état (queued, running, succeeded, failed) et téléchargement authentifié des résultats."
                    </p>
                </div>
            </div>
            <div class="table-wrapper">
                <table class="data-table" aria-label="Liste des tâches de remuxage">
                    <thead>
                        <tr>
                            <th scope="col">"Identifiant"</th>
                            <th scope="col">"Source"</th>
                            <th scope="col">"Statut"</th>
                            <th scope="col">"Création"</th>
                            <th scope="col">"Téléchargement"</th>
                        </tr>
                    </thead>
                    <tbody id="jobs-table-body">
                        <tr>
                            <td colspan="5" class="empty-state-text">
                                "Chargement des tâches…"
                            </td>
                        </tr>
                    </tbody>
                </table>
            </div>
            <p class="form-help" style="margin-top: 0.75rem;">
                "Note sur les fichiers volumineux : le téléchargement s'effectue via l'API avec la clé en mémoire locale. Pour les médias de plusieurs gigaoctets, privilégier curl en direct avec en-tête Bearer ou l'accès au volume partagé."
            </p>
        </section>
    }
}
