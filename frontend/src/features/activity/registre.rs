//! Registre des tâches : tableau commun, puis tiroir d'inspection de la tâche ouverte.

use crate::shared::ui::{DetailRow, InspectionPane, RegistreTable, inspection_drawer};
use leptos::prelude::*;

/// Identifiant du registre, cible commune des onglets d'Activité.
pub const JOBS_REGISTRE_ID: &str = "jobs-registre";

fn job_panes() -> Vec<InspectionPane> {
    vec![
        InspectionPane {
            suffix: "overview",
            label: "Aperçu",
            body: view! {
                <div class="detail-grid">
                    <DetailRow label="ID de la tâche" id="detail-job-id" />
                    <DetailRow label="Fichier source" id="detail-job-source" />
                    <DetailRow label="Statut" id="detail-job-status" />
                    <DetailRow label="Agent assigné" id="detail-job-worker" />
                    <DetailRow label="Tentatives" id="detail-job-attempts" />
                    <DetailRow label="Création" id="detail-job-created" />
                    <DetailRow label="Fichier résultat" id="detail-job-output" />
                </div>
            }
            .into_any(),
        },
        InspectionPane {
            suffix: "diagnostic",
            label: "Diagnostic",
            body: view! {
                <div id="detail-error-box" class="toast-notice error" hidden>
                    <strong>"Erreur signalée :"</strong>
                    <div id="detail-error-content" class="code-snippet">"—"</div>
                </div>
                <div class="form-group">
                    <label class="form-label">"Commande de téléchargement directe (curl) :"</label>
                    <div id="detail-curl-command" class="code-snippet">"—"</div>
                </div>
            }
            .into_any(),
        },
    ]
}

fn job_header_actions() -> AnyView {
    view! {
        <div class="inspection-drawer__actions">
            <button
                type="button"
                id="jobs-fact-download-btn"
                class="btn btn-primary btn-sm inspection-panel__action"
                hidden
                disabled
            >
                "Télécharger"
            </button>
        </div>
    }
    .into_any()
}

#[component]
pub fn ActivityRegistre() -> impl IntoView {
    view! {
        <div class="registre" id=JOBS_REGISTRE_ID role="tabpanel">
            <input type="search" id="jobs-search-input" hidden />
            <RegistreTable
                aria_label="Tâches de traitement"
                headers=vec!["Identifiant", "Source", "Statut", "Création", "Action"]
                body_id="jobs-table-body"
                empty="Chargement des tâches…"
            />
            {inspection_drawer("jobs-fact", job_panes(), Some(job_header_actions()))}
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn render() -> String {
        view! { <ActivityRegistre /> }.to_html()
    }

    #[test]
    fn test_registre_uses_the_common_table_and_drawer() {
        let html = render();
        assert!(html.contains("class=\"registre\""));
        assert!(html.contains("id=\"jobs-table-body\""));
        assert!(html.contains("class=\"inspection-drawer\""));
        assert!(html.contains("id=\"jobs-fact\""));
        assert!(html.contains("id=\"jobs-fact-pane-diagnostic\""));
    }

    #[test]
    fn test_drawer_keeps_every_detail_id_used_by_the_client() {
        let html = render();
        for id in [
            "detail-job-id",
            "detail-job-source",
            "detail-job-status",
            "detail-job-worker",
            "detail-job-attempts",
            "detail-job-created",
            "detail-job-output",
            "detail-error-box",
            "detail-error-content",
            "detail-curl-command",
            "jobs-fact-download-btn",
            "jobs-search-input",
        ] {
            assert!(html.contains(&format!("id=\"{id}\"")), "id manquant : {id}");
        }
    }

    #[test]
    fn test_registre_has_no_modal_and_no_inline_style() {
        let html = render();
        assert!(!html.contains("<dialog"));
        assert!(!html.contains("style="));
    }
}
