//! Panneau d'accueil et tableau de bord du poste de pilotage.

use leptos::prelude::*;

#[component]
pub fn DashboardPanel() -> impl IntoView {
    view! {
        <div id="panel-dashboard" class="tab-panel" role="tabpanel" aria-labelledby="tab-btn-dashboard">
            <section class="metrics-grid" aria-label="Indicateurs clés du système">
                <div class="metric-card">
                    <div class="metric-label">"Coordinateur"</div>
                    <div id="metric-health" class="metric-value">"…"</div>
                    <div class="metric-subtitle">"Sonde /health"</div>
                </div>
                <div class="metric-card">
                    <div class="metric-label">"Mode opérationnel"</div>
                    <div id="metric-mode" class="metric-value" style="font-size: 1.25rem;">"Centralisé"</div>
                    <div class="metric-subtitle">"1 à N serveurs • HA optionnelle"</div>
                </div>
                <div class="metric-card">
                    <div class="metric-label">"Fichiers prêts"</div>
                    <div id="metric-files-count" class="metric-value">"0"</div>
                    <div class="metric-subtitle">"Dans var/input/ (lecture seule)"</div>
                </div>
                <div class="metric-card">
                    <div class="metric-label">"Tâches enregistrées"</div>
                    <div id="metric-jobs-count" class="metric-value">"0"</div>
                    <div class="metric-subtitle">"En base de données SQLite (/config)"</div>
                </div>
                <div class="metric-card">
                    <div class="metric-label">"Agents actifs"</div>
                    <div id="metric-workers-count" class="metric-value">"0"</div>
                    <div class="metric-subtitle">"Enregistrés via l'API"</div>
                </div>
            </section>

            <section class="section-panel" aria-labelledby="quick-actions-title">
                <div class="section-panel-header">
                    <div>
                        <h2 id="quick-actions-title" class="section-title">"Actions rapides"</h2>
                        <p class="section-subtitle">"Démarrez un traitement ou supervisez la chaîne de remuxage."</p>
                    </div>
                </div>
                <div style="display: flex; gap: 0.75rem; flex-wrap: wrap;">
                    <button
                        type="button"
                        id="quick-goto-files-btn"
                        class="btn btn-primary"
                    >
                        "Parcourir les fichiers et lancer un remuxage"
                    </button>
                    <button
                        type="button"
                        id="quick-goto-jobs-btn"
                        class="btn btn-secondary"
                    >
                        "Consulter la file des tâches"
                    </button>
                    <button
                        type="button"
                        id="quick-goto-settings-btn"
                        class="btn btn-secondary"
                    >
                        "Gérer la session / Authentification"
                    </button>
                </div>
            </section>

            <section class="section-panel" aria-labelledby="recent-jobs-title">
                <div class="section-panel-header">
                    <div>
                        <h2 id="recent-jobs-title" class="section-title">"Dernières tâches traitées"</h2>
                        <p class="section-subtitle">"Aperçu des 3 derniers traitements soumis."</p>
                    </div>
                </div>
                <div class="table-wrapper">
                    <table class="data-table" aria-label="Dernières tâches traitées">
                        <thead>
                            <tr>
                                <th scope="col">"ID"</th>
                                <th scope="col">"Source"</th>
                                <th scope="col">"Statut"</th>
                                <th scope="col">"Création"</th>
                            </tr>
                        </thead>
                        <tbody id="recent-jobs-table-body">
                            <tr>
                                <td colspan="4" class="empty-state-text">
                                    "Aucune tâche enregistrée pour le moment."
                                </td>
                            </tr>
                        </tbody>
                    </table>
                </div>
            </section>

            <section class="section-panel" aria-labelledby="future-plan-title">
                <div class="section-panel-header">
                    <div>
                        <h2 id="future-plan-title" class="section-title">"Fonctionnalités planifiées (hors périmètre actuel)"</h2>
                        <p class="section-subtitle">
                            "Présentation honnête des modules d'extension en cours de conception."
                        </p>
                    </div>
                </div>
                <div class="future-grid">
                    <div class="future-card">
                        <span class="future-badge">"Planifié"</span>
                        <h3 style="font-size: 1rem; font-weight: 700; margin-bottom: 0.35rem;">"Éditeur visuel de flux"</h3>
                        <p style="font-size: 0.8125rem; color: var(--text-dim);">
                            "Composition graphique des étapes de traitement sans écriture de script. En attente de spécification de contrat."
                        </p>
                    </div>
                    <div class="future-card">
                        <span class="future-badge">"Planifié"</span>
                        <h3 style="font-size: 1rem; font-weight: 700; margin-bottom: 0.35rem;">"Connecteurs distants S3 & SMB"</h3>
                        <p style="font-size: 0.8125rem; color: var(--text-dim);">
                            "Acquisition et dépôt direct sur partages réseau distants. Le périmètre actuel opère sur volumes Docker montés."
                        </p>
                    </div>
                    <div class="future-card">
                        <span class="future-badge">"Planifié"</span>
                        <h3 style="font-size: 1rem; font-weight: 700; margin-bottom: 0.35rem;">"Catalogue d'outils et plugins"</h3>
                        <p style="font-size: 0.8125rem; color: var(--text-dim);">
                            "Dépôt séparé flux-de-fichiers-plugins en cours d'élaboration. Aucun chargeur dynamique n'est encore déployé."
                        </p>
                    </div>
                </div>
            </section>
        </div>
    }
}
