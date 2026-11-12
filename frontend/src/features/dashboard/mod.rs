//! Panneau d'accueil et Hub de statistiques : Vue d'ensemble du système.

use crate::shared::ui::{
    Button, ButtonVariant, Card, IconName, MetricCard, MetricTone, SectionHeader,
};
use leptos::prelude::*;

/// Valeur affichée tant que la première synchronisation n'a pas répondu.
const PENDING_VALUE: &str = "—";

/// Ligne d'état d'un montage : libellé à gauche, valeur (mise à jour par le JS) à droite.
fn storage_row(label: &'static str, id: &'static str, value: &'static str) -> impl IntoView {
    view! {
        <div class="storage-row">
            <span class="storage-label">{label}</span>
            <span class="storage-val" id=id>{value}</span>
        </div>
    }
}

#[component]
fn OverviewHero() -> impl IntoView {
    view! {
        <div class="overview-hero-card">
            <div class="hero-content">
                <div class="hero-badge" id="hero-status-badge" data-tone="neutral">
                    <span class="status-dot"></span>
                    <span id="hero-status-text">"Vérification du coordinateur…"</span>
                </div>
                <h1 class="hero-title">"Vue d'ensemble"</h1>
                <p class="hero-subtitle">
                    "Pipelines d'encodage MKV, stockage média et agents de traitement, en direct."
                </p>
            </div>
            <div class="hero-actions">
                <Button id="quick-goto-files-btn" variant=ButtonVariant::Primary>
                    "Parcourir les bibliothèques"
                </Button>
                <Button id="quick-goto-flows-btn" variant=ButtonVariant::Secondary>
                    "Concepteur de flux"
                </Button>
            </div>
        </div>
    }
}

#[component]
fn OverviewMetrics() -> impl IntoView {
    view! {
        <section class="metrics-grid" aria-label="Statistiques clés du système">
            <MetricCard
                label="Fichiers répertoriés"
                value=PENDING_VALUE
                id="metric-files-count"
                subtitle="Sources dans var/input/ (lecture seule)"
                icon=IconName::Libraries
            />
            <MetricCard
                label="Volume total géré"
                value=PENDING_VALUE
                id="metric-storage-volume"
                subtitle="Médias analysés par le coordinateur"
                icon=IconName::Storage
            />
            <MetricCard
                label="Taux de réussite"
                value=PENDING_VALUE
                id="metric-success-rate"
                subtitle="Baux de traitement menés à terme"
                icon=IconName::Check
                tone=MetricTone::Neutral
            />
            <MetricCard
                label="Workers en ligne"
                value=PENDING_VALUE
                id="metric-workers-count"
                subtitle="Coordinateur autonome inclus"
                icon=IconName::Workers
            />
        </section>
    }
}

#[component]
fn RecentJobsCard() -> impl IntoView {
    let all_jobs_action = view! {
        <Button
            id="quick-goto-jobs-btn"
            variant=ButtonVariant::Secondary
            class="btn-sm"
            aria_label="Voir toutes les tâches"
        >
            "Toutes les tâches →"
        </Button>
    }
    .into_any();

    view! {
        <Card aria_labelledby="recent-jobs-title">
            <SectionHeader
                id="recent-jobs-title"
                title="Activité récente"
                subtitle="Dernières exécutions, synchronisées en direct."
                actions=all_jobs_action
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
    }
}

#[component]
fn MediaInsightsCard() -> impl IntoView {
    view! {
        <Card aria_labelledby="media-insights-title">
            <SectionHeader
                id="media-insights-title"
                title="Médias et stockage"
                subtitle="Répartition des formats et rôle de chaque montage."
            />
            <div class="media-insights-wrapper">
                <div class="insights-section-title">
                    <span>"Formats de conteneurs détectés"</span>
                    <span id="media-formats-total-label" class="insights-total">"0 fichier"</span>
                </div>
                <div class="segmented-distribution-bar" id="media-formats-bar">
                    <div class="segment-fill default-segment"></div>
                </div>
                <div class="distribution-legend" id="media-formats-legend">
                    <span class="legend-item"><span class="legend-dot mkv"></span>"MKV (Matroska)"</span>
                    <span class="legend-item"><span class="legend-dot mp4"></span>"MP4 / M4V"</span>
                    <span class="legend-item"><span class="legend-dot other"></span>"Autres"</span>
                </div>
                <div class="storage-breakdown">
                    {storage_row("Entrée (/input)", "storage-input-val", "Lecture seule")}
                    {storage_row("Sortie (/output)", "storage-output-val", "Écriture atomique")}
                    {storage_row("Persistance (/config)", "storage-config-val", "SQLite WAL • Baux et clés")}
                </div>
            </div>
        </Card>
    }
}

#[component]
pub fn DashboardPanel() -> impl IntoView {
    view! {
        <div id=crate::core::panel::DASHBOARD.dom_id class="tab-panel" role="tabpanel" aria-labelledby=crate::core::panel::DASHBOARD.button_id>
            <OverviewHero />
            <OverviewMetrics />
            <div class="overview-grid-2col">
                <RecentJobsCard />
                <MediaInsightsCard />
            </div>
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn render() -> String {
        view! { <DashboardPanel /> }.to_html()
    }

    #[test]
    fn test_dashboard_keeps_ids_used_by_the_client() {
        let html = render();
        for id in [
            "metric-files-count",
            "metric-storage-volume",
            "metric-success-rate",
            "metric-workers-count",
            "recent-jobs-table-body",
            "media-formats-bar",
            "media-formats-legend",
            "media-formats-total-label",
            "hero-status-text",
            "hero-status-badge",
            "quick-goto-files-btn",
            "quick-goto-flows-btn",
            "quick-goto-jobs-btn",
        ] {
            assert!(html.contains(&format!("id=\"{id}\"")), "id manquant : {id}");
        }
    }

    #[test]
    fn test_dashboard_has_four_metric_tiles_with_icons() {
        let html = render();
        assert_eq!(html.matches("class=\"metric-card\"").count(), 4);
        assert_eq!(html.matches("class=\"metric-icon\"").count(), 4);
    }

    #[test]
    fn test_dashboard_has_no_inline_style_and_no_duplicated_shortcuts_card() {
        let html = render();
        assert!(!html.contains("style="));
        assert!(!html.contains("quick-actions-title"));
    }

    #[test]
    fn test_storage_row_renders_label_and_value() {
        let html = view! { {storage_row("Libellé", "row-id", "Valeur")} }.to_html();
        assert!(html.contains("Libellé"));
        assert!(html.contains("id=\"row-id\""));
        assert!(html.contains("Valeur"));
    }
}
