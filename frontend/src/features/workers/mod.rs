//! Registre des nœuds. Une seule nature, donc pas de sous-navigation.
//! Le tiroir porte l'aperçu du nœud ouvert, puis le raccordement d'un nouveau nœud.

use crate::features::widgets::{WidgetArea, WidgetPlacement};
use crate::shared::ui::{
    Button, ButtonVariant, DetailRow, InspectionPane, RegistreTable, inspection_drawer,
};
use leptos::prelude::*;

/// Identifiant du volet de raccordement, sélectionné par l'action de capsule.
pub const WORKER_CONNECT_TAB_ID: &str = "workers-fact-tab-connect";

fn overview_pane() -> InspectionPane {
    InspectionPane {
        suffix: "overview",
        label: "Aperçu",
        body: view! {
            <p id="workers-fact-facts" class="inspection-panel__facts">
                "Le détail du nœud apparaît ici."
            </p>
        }
        .into_any(),
    }
}

fn connect_pane() -> InspectionPane {
    InspectionPane {
        suffix: "connect",
        label: "Raccordement",
        body: view! {
            <p class="form-help">
                "Clé d'agent et commande pour adjoindre un worker distant en HTTP."
            </p>
            <div class="detail-grid">
                <DetailRow label="Rôle de l'instance">
                    <code>"Coordinateur central"</code>" (seule autorité)"
                </DetailRow>
                <DetailRow label="Répertoire de configuration">
                    <code>"/config"</code>" (SQLite flux.db, clés 256 bits auto-amorcées)"
                </DetailRow>
                <DetailRow label="Worker local intégré" id="config-local-worker" />
                <DetailRow label="Version du coordinateur" id="config-version" />
                <DetailRow label="Taille max par fichier" id="config-max-file-bytes" />
            </div>

            <div class="form-group">
                <p class="form-help">
                    "Créez une clé d'API de rôle « Worker » dans les Paramètres, puis passez-la au worker avec WORKER_KEY. Elle est montrée une seule fois, à sa création."
                </p>
                <Button id="btn-open-api-keys" variant=ButtonVariant::Secondary data_goto="settings">
                    "Ouvrir les clés d'API"
                </Button>
            </div>

            <div class="form-group">
                <label for="docker-worker-cmd" class="form-label">
                    "Commande Docker de raccordement immédiat (HTTP)"
                </label>
                <pre class="code-block-wrapper"><code id="docker-worker-cmd">"docker run -d --name flux-worker -e COORDINATOR_URL=http://localhost:3000 -e WORKER_KEY=<CLÉ_API_WORKER> -v /chemin/data:/data flux-de-fichiers worker"</code></pre>
                <Button
                    id="btn-copy-worker-cmd"
                    variant=ButtonVariant::Secondary
                    aria_label="Copier la commande Docker pour worker dans le presse-papier"
                >
                    "Copier la commande"
                </Button>
            </div>
        }
        .into_any(),
    }
}

#[component]
pub fn WorkersPanel() -> impl IntoView {
    view! {
        <div id=crate::core::panel::WORKERS.dom_id class="tab-panel" role="tabpanel" aria-labelledby=crate::core::panel::WORKERS.button_id hidden>
            <WidgetArea placement=WidgetPlacement::Workers />
            <div class="registre">
                <RegistreTable
                    aria_label="Nœuds qui exécutent"
                    headers=vec!["Nœud", "Rôle", "Présence", "État", "Action"]
                    body_id="workers-table-body"
                    empty="Chargement des nœuds…"
                />
                {inspection_drawer("workers-fact", vec![overview_pane(), connect_pane()], None)}
            </div>
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn render() -> String {
        view! { <WorkersPanel /> }.to_html()
    }

    #[test]
    fn test_workers_use_the_common_table_and_drawer() {
        let html = render();
        assert!(html.contains("id=\"workers-table-body\""));
        assert!(html.contains("class=\"inspection-drawer\""));
        assert!(html.contains(&format!("id=\"{WORKER_CONNECT_TAB_ID}\"")));
        assert!(!html.contains("registre-fact"));
        assert!(!html.contains("worker-connect-panel"));
    }

    #[test]
    fn test_connect_pane_keeps_every_id_used_by_the_client() {
        let html = render();
        for id in [
            "config-local-worker",
            "config-version",
            "config-max-file-bytes",
            "btn-open-api-keys",
            "docker-worker-cmd",
            "btn-copy-worker-cmd",
            "workers-fact-facts",
        ] {
            assert!(html.contains(&format!("id=\"{id}\"")), "id manquant : {id}");
        }
    }

    #[test]
    fn test_workers_panel_has_no_inline_style() {
        assert!(!render().contains("style="));
    }
}
