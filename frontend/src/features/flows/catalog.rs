//! Registre des flux que l'instance exécute.

use crate::shared::ui::{RegistreFact, RegistreTable};
use leptos::prelude::*;

#[component]
pub fn FlowCatalog() -> impl IntoView {
    view! {
        <div id="flows-registre" class="registre">
            <div class="registre-toolbar">
                <h2 id="flows-section-title" class="section-title">
                    "Flux que j'exécute"
                </h2>
                <button
                    type="button"
                    id="btn-open-flow-wizard"
                    class="btn btn-primary btn-sm"
                    aria-label="Ouvrir l'assistant de création de flux"
                >
                    "Nouveau flux"
                </button>
            </div>
            <RegistreTable
                aria_label="Flux que j'exécute"
                headers=vec!["Flux", "Masque", "Composition", "Action"]
                body_id="flows-table-body"
                empty="Chargement des flux…"
            />
            <RegistreFact id="flows-fact" action_id="flows-fact-action" action="Éditer" />
        </div>
    }
}
