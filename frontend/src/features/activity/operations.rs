//! Sous-vue Exploiter : Tâches actives, file d'attente et progression temps réel (Rôle Exploiter).

use leptos::prelude::*;

#[component]
pub fn ActivityOperations() -> impl IntoView {
    view! {
        <div id="activity-view-operations" class="activity-subview">
            <div class="toolbar-bar">
                <div class="search-box">
                    <span class="search-icon" aria-hidden="true">"🔍"</span>
                    <input
                        type="search"
                        id="jobs-search-input"
                        class="search-input"
                        placeholder="Filtrer les traitements actifs…"
                        aria-label="Filtrer les tâches actives par ID ou nom"
                    />
                </div>
                <div class="filter-group" role="group" aria-label="Filtrer les tâches actives">
                    <button
                        type="button"
                        class="filter-chip"
                        data-filter="all"
                        aria-pressed="true"
                    >
                        "Tous" <span id="count-filter-all" style="opacity: 0.8;">"(0)"</span>
                    </button>
                    <button
                        type="button"
                        class="filter-chip"
                        data-filter="queued"
                        aria-pressed="false"
                    >
                        "En attente" <span id="count-filter-queued" style="opacity: 0.8;">"(0)"</span>
                    </button>
                    <button
                        type="button"
                        class="filter-chip"
                        data-filter="running"
                        aria-pressed="false"
                    >
                        "En cours" <span id="count-filter-running" style="opacity: 0.8;">"(0)"</span>
                    </button>
                    <button
                        type="button"
                        class="filter-chip"
                        data-filter="succeeded"
                        aria-pressed="false"
                    >
                        "Succès" <span id="count-filter-succeeded" style="opacity: 0.8;">"(0)"</span>
                    </button>
                    <button
                        type="button"
                        class="filter-chip"
                        data-filter="failed"
                        aria-pressed="false"
                    >
                        "Échecs" <span id="count-filter-failed" style="opacity: 0.8;">"(0)"</span>
                    </button>
                </div>
            </div>

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
