//! Registre des bibliothèques déclarées. Le tiroir porte le nom, les workers et le flux.

use crate::shared::ui::{InspectionPane, RegistreTable, inspection_drawer};
use leptos::prelude::*;

fn library_panes() -> Vec<InspectionPane> {
    vec![InspectionPane {
        suffix: "overview",
        label: "Bibliothèque",
        body: view! {
            <div class="form-group">
                <label for="library-name-input" class="form-label">"Nom"</label>
                <input
                    type="text"
                    id="library-name-input"
                    class="form-input"
                    maxlength="80"
                    autocomplete="off"
                />
            </div>
            <p id="libraries-fact-facts" class="inspection-panel__facts">
                "Le détail de la bibliothèque apparaît ici."
            </p>
            <h3 class="section-title">"Workers"</h3>
            <div id="library-workers-list" class="library-entry-list"></div>
            <div class="form-group">
                <label for="library-entry-worker" class="form-label">"Worker"</label>
                <select id="library-entry-worker" class="form-select"></select>
            </div>
            <div class="form-group">
                <label for="library-entry-path" class="form-label">"Chemin sur ce worker"</label>
                <input
                    type="text"
                    id="library-entry-path"
                    class="form-input"
                    placeholder="/mnt/medias"
                    autocomplete="off"
                />
                <p class="form-help">"Chemin absolu tel que ce worker le voit."</p>
            </div>
            <div class="form-group">
                <label for="library-entry-access" class="form-label">"Accès"</label>
                <select id="library-entry-access" class="form-select">
                    <option value="read_write">"Lecture / Écriture"</option>
                    <option value="read">"Lecture seule"</option>
                </select>
            </div>
            <button type="button" id="library-entry-add-btn" class="btn btn-secondary btn-sm">
                "Ajouter le worker"
            </button>
            <h3 class="section-title">"Flux"</h3>
            <div class="form-group">
                <label for="library-flow-select" class="form-label">"Flux accroché"</label>
                <select id="library-flow-select" class="form-select"></select>
                <p class="form-help">"Un seul flux est accroché à la bibliothèque."</p>
            </div>
        }
        .into_any(),
    }]
}

fn library_header_actions() -> AnyView {
    view! {
        <div class="inspection-drawer__actions">
            <button
                type="button"
                id="libraries-save-btn"
                class="btn btn-primary btn-sm inspection-panel__action"
                disabled
            >
                "Enregistrer"
            </button>
            <button
                type="button"
                id="libraries-delete-btn"
                class="btn btn-danger btn-sm inspection-panel__action"
                hidden
                disabled
            >
                "Supprimer"
            </button>
        </div>
    }
    .into_any()
}

#[component]
pub fn LibrariesPanel() -> impl IntoView {
    view! {
        <div
            id=crate::core::panel::FILES.dom_id
            class="tab-panel"
            role="tabpanel"
            aria-labelledby=crate::core::panel::FILES.button_id
            hidden
        >
            <div class="registre" id="library-registre">
                <input type="search" id="libraries-search-input" hidden />
                <RegistreTable
                    aria_label="Bibliothèques déclarées"
                    headers=vec!["Bibliothèque", "Flux", "Workers", "État", "Action"]
                    body_id="libraries-table-body"
                    empty="Chargement des bibliothèques…"
                />
                {inspection_drawer("libraries-fact", library_panes(), Some(library_header_actions()))}
            </div>
        </div>
    }
}
