//! Module d'activité : Rôles Exploiter (traitements en direct) et Diagnostiquer (historique et échecs).

pub mod diagnostic;
pub mod dialog;
pub mod operations;

pub use diagnostic::ActivityDiagnostic;
pub use dialog::JobDetailDialog;
pub use operations::ActivityOperations;

use crate::{
    core::{SubNav, SubNavTabItem},
    shared::ui::{Card, SectionHeader, StatusBadge},
};
use leptos::prelude::*;

#[component]
pub fn JobsPanel() -> impl IntoView {
    let subnav_tabs = vec![
        SubNavTabItem {
            id: "btn-subtab-jobs-active",
            label: "File en direct",
            icon: Some("⚡"),
            target_id: "subtab-jobs-active",
            is_active: true,
            badge: None,
        },
        SubNavTabItem {
            id: "btn-subtab-jobs-history",
            label: "Historique",
            icon: Some("📜"),
            target_id: "subtab-jobs-history",
            is_active: false,
            badge: None,
        },
        SubNavTabItem {
            id: "btn-subtab-jobs-diagnostic",
            label: "Diagnostic",
            icon: Some("🔬"),
            target_id: "subtab-jobs-diagnostic",
            is_active: false,
            badge: None,
        },
    ];

    let subnav_info = view! {
        <StatusBadge
            id="sse-status-badge"
            label="Sondage périodique"
            variant="info"
            title="Mode de rafraîchissement des tâches".to_string()
        />
    }
    .into_any();

    view! {
        <div id="panel-jobs" class="tab-panel" role="tabpanel" aria-labelledby="tab-btn-jobs" hidden>
            <SubNav
                aria_label="Sous-navigation de la section Activité et Traitements"
                tabs=subnav_tabs
                info=subnav_info
            />

            // Conteneur principal de l'activité
            <Card aria_labelledby="jobs-section-title">
                <SectionHeader
                    id="jobs-section-title"
                    title="Poste d'activité et supervision"
                    subtitle="Suivi temps réel des traitements actifs (Exploiter) et analyse approfondie des exécutions antérieures (Diagnostiquer)."
                />

                <div id="subtab-jobs-active">
                    <ActivityOperations />
                </div>
                <div id="subtab-jobs-history" hidden>
                    // La table des tâches est partagée dynamiquement
                </div>
                <div id="subtab-jobs-diagnostic" hidden>
                    <ActivityDiagnostic />
                </div>
            </Card>

            <JobDetailDialog />
        </div>
    }
}
