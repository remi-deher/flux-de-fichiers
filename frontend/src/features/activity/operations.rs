//! Sous-vue Exploiter : Tâches actives, file d'attente et progression temps réel (Rôle Exploiter).

use leptos::prelude::*;

#[component]
pub fn ActivityOperations() -> impl IntoView {
    view! {
        <div id="activity-view-operations" class="activity-subview">
            <input type="search" id="jobs-search-input" hidden />

            <div class="table-wrapper">
                <table class="data-table" aria-label="File des traitements en cours d'exécution">
                    <thead>
                        <tr>
                            <th scope="col">"Identifiant"</th>
                            <th scope="col">"Source"</th>
                            <th scope="col">"Statut / Progression"</th>
                            <th scope="col">"Création"</th>
                            <th scope="col" style="text-align: right;">"Actions"</th>
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
        </div>
    }
}
