//! Panneau d'accueil et Hub de statistiques : Vue d'ensemble du système.

use crate::shared::ui::{Button, ButtonVariant, Card, MetricCard, SectionHeader};
use leptos::prelude::*;

#[component]
pub fn DashboardPanel() -> impl IntoView {
    view! {
        <div id=crate::core::panel::DASHBOARD.dom_id class="tab-panel" role="tabpanel" aria-labelledby=crate::core::panel::DASHBOARD.button_id>
            // Bannière de synthèse d'accueil (Hero Overview Card)
            <div class="overview-hero-card">
                <div class="hero-content">
                    <div class="hero-badge">
                        <span class="status-dot"></span>
                        <span id="hero-status-text">"Système opérationnel • Coordinateur autonome"</span>
                    </div>
                    <h1 class="hero-title">"Vue d'ensemble du Système"</h1>
                    <p class="hero-subtitle">
                        "Surveillance en direct des pipelines d'encodage MKV, du stockage média et des agents de traitement distribués."
                    </p>
                </div>
                <div class="hero-actions">
                    <Button id="quick-goto-files-btn" variant=ButtonVariant::Primary>
                        "Parcourir les bibliothèques"
                    </Button>
                </div>
            </div>

            // Grille des indicateurs statistiques clés (KPIs)
            <section class="metrics-grid" aria-label="Statistiques clés du système">
                <MetricCard
                    label="Fichiers répertoriés"
                    value="0"
                    id="metric-files-count"
                    subtitle="Sources dans var/input/ (lecture seule)"
                />
                <MetricCard
                    label="Volume total géré"
                    value="0 o"
                    id="metric-storage-volume"
                    subtitle="Médias analysés par le coordinateur"
                />
                <MetricCard
                    label="Taux de réussite"
                    value="100%"
                    id="metric-success-rate"
                    subtitle="Baux de traitement complétés sans erreur"
                />
                <MetricCard
                    label="Tâches enregistrées"
                    value="0"
                    id="metric-jobs-count"
                    subtitle="Historique des encodages en base SQLite"
                />
                <MetricCard
                    label="Workers en ligne"
                    value="1"
                    id="metric-workers-count"
                    subtitle="Central autonome inclus"
                />
            </section>

            // Grille bi-colonne : Activité récente & Insights médias
            <div class="overview-grid-2col">
                // Colonne gauche : Traitements récents
                <Card aria_labelledby="recent-jobs-title">
                    <div style="display: flex; justify-content: space-between; align-items: baseline; margin-bottom: 0.5rem;">
                        <SectionHeader
                            id="recent-jobs-title"
                            title="Dernières tâches & Traitements en direct"
                            subtitle="Suivi temps réel des exécutions (synchronisé via SSE)."
                        />
                        <button
                            type="button"
                            id="quick-goto-jobs-btn"
                            class="btn btn-secondary btn-sm"
                            aria-label="Voir toutes les tâches"
                        >
                            "Toutes les tâches →"
                        </button>
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
                </Card>

                // Colonne droite : Statistiques de la Bibliothèque & Stockage
                <Card aria_labelledby="media-insights-title">
                    <SectionHeader
                        id="media-insights-title"
                        title="Distribution des Médias & Stockage"
                        subtitle="Répartition des formats et occupation des montages."
                    />

                    <div class="media-insights-wrapper">
                        <div class="insights-section-title">
                            <span>"Formats de conteneurs détectés"</span>
                            <span id="media-formats-total-label" style="font-weight: 600; color: var(--text-main);">"0 fichier"</span>
                        </div>
                        <div class="segmented-distribution-bar" id="media-formats-bar">
                            <div class="segment-fill default-segment" style="width: 100%;"></div>
                        </div>
                        <div class="distribution-legend" id="media-formats-legend">
                            <span class="legend-item"><span class="legend-dot mkv"></span>"MKV (Matroska)"</span>
                            <span class="legend-item"><span class="legend-dot mp4"></span>"MP4 / M4V"</span>
                            <span class="legend-item"><span class="legend-dot other"></span>"Autres"</span>
                        </div>

                        <div class="storage-breakdown">
                            <div class="storage-row">
                                <span class="storage-label">"Volume d'entrée (/input)"</span>
                                <span class="storage-val" id="storage-input-val">"Lecture seule • Sécurisé"</span>
                            </div>
                            <div class="storage-row">
                                <span class="storage-label">"Volume de sortie (/output)"</span>
                                <span class="storage-val" id="storage-output-val">"Écriture atomique des remux"</span>
                            </div>
                            <div class="storage-row">
                                <span class="storage-label">"Persistance système (/config)"</span>
                                <span class="storage-val" id="storage-config-val">"SQLite WAL • Baux & Clés"</span>
                            </div>
                        </div>
                    </div>
                </Card>
            </div>

            // Section Actions rapides
            <Card aria_labelledby="quick-actions-title">
                <SectionHeader
                    id="quick-actions-title"
                    title="Raccourcis & Navigation rapide"
                    subtitle="Accédez directement aux modules de conception et de supervision."
                />
                <div style="display: flex; gap: 0.75rem; flex-wrap: wrap;">
                    <Button id="quick-goto-flows-btn" variant=ButtonVariant::Secondary>
                        "Concepteur de flux (Flows)"
                    </Button>
                    <Button id="quick-goto-settings-btn" variant=ButtonVariant::Secondary>
                        "Paramètres d'administration & Sécurité"
                    </Button>
                </div>
            </Card>
        </div>
    }
}
