//! Panneau d'accueil et tableau de bord du poste de pilotage.

use crate::shared::ui::{Button, ButtonVariant, Card, MetricCard, SectionHeader};
use leptos::prelude::*;

#[component]
pub fn DashboardPanel() -> impl IntoView {
    view! {
        <div id="panel-dashboard" class="tab-panel" role="tabpanel" aria-labelledby="tab-btn-dashboard">
            <section class="metrics-grid" aria-label="Indicateurs clés du système">
                <MetricCard
                    label="Coordinateur"
                    value="…"
                    id="metric-health"
                    subtitle="Sonde /health"
                />
                <MetricCard
                    label="Mode opérationnel"
                    value="Centralisé"
                    id="metric-mode"
                    subtitle="1 à N serveurs • HA optionnelle"
                    value_style="font-size: 1.25rem;"
                />
                <MetricCard
                    label="Fichiers prêts"
                    value="0"
                    id="metric-files-count"
                    subtitle="Dans var/input/ (lecture seule)"
                />
                <MetricCard
                    label="Tâches enregistrées"
                    value="0"
                    id="metric-jobs-count"
                    subtitle="En base de données SQLite (/config)"
                />
                <MetricCard
                    label="Agents actifs"
                    value="0"
                    id="metric-workers-count"
                    subtitle="Enregistrés via l'API"
                />
            </section>

            <Card aria_labelledby="quick-actions-title">
                <SectionHeader
                    id="quick-actions-title"
                    title="Actions rapides"
                    subtitle="Démarrez un traitement, configurez vos flux ou supervisez l'exécution des tâches."
                />
                <div style="display: flex; gap: 0.75rem; flex-wrap: wrap;">
                    <Button
                        id="quick-goto-files-btn"
                        variant=ButtonVariant::Primary
                    >
                        "Parcourir les fichiers et lancer un traitement"
                    </Button>
                    <Button
                        id="quick-goto-flows-btn"
                        variant=ButtonVariant::Secondary
                    >
                        "Flux de traitement (Flows)"
                    </Button>
                    <Button
                        id="quick-goto-jobs-btn"
                        variant=ButtonVariant::Secondary
                    >
                        "Gestionnaire de tâches"
                    </Button>
                    <Button
                        id="quick-goto-settings-btn"
                        variant=ButtonVariant::Secondary
                    >
                        "Gérer la session / Authentification"
                    </Button>
                </div>
            </Card>

            <Card aria_labelledby="recent-jobs-title">
                <SectionHeader
                    id="recent-jobs-title"
                    title="Dernières tâches traitées"
                    subtitle="Aperçu des 3 derniers traitements soumis."
                />
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
            </Card>

            <Card aria_labelledby="future-plan-title">
                <SectionHeader
                    id="future-plan-title"
                    title="Fonctionnalités planifiées (hors périmètre actuel)"
                    subtitle="Présentation honnête des modules d'extension en cours de conception."
                />
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
            </Card>
        </div>
    }
}
