//! Registre des flux que l'instance exécute.

use crate::shared::ui::{InspectionPanel, RegistreTable};
use leptos::prelude::*;

#[component]
pub fn FlowCatalog() -> impl IntoView {
    view! {
        <div id="flows-registre" class="registre">
            <h2 id="flows-section-title" class="section-title">
                "Flux que j'exécute"
            </h2>
            <RegistreTable
                aria_label="Flux que j'exécute"
                headers=vec!["Flux", "Masque", "Composition", "Action"]
                body_id="flows-table-body"
                empty="Chargement des flux…"
            />
            <InspectionPanel id="flows-fact" action="Éditer" />
        </div>
    }
}
