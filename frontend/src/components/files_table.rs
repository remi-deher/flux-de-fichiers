//! Composant d'affichage des fichiers sources disponibles.

use leptos::prelude::*;

#[component]
pub fn FilesTable() -> impl IntoView {
    view! {
        <section class="section-panel" aria-labelledby="files-section-title">
            <div class="section-panel-header">
                <div>
                    <h2 id="files-section-title" class="section-title">"Fichiers sources admissibles"</h2>
                    <p class="section-subtitle">
                        "Fichiers MKV détectés dans INPUT_DIR, prêts pour un remuxage sans réencodage."
                    </p>
                </div>
            </div>
            <div class="table-wrapper">
                <table class="data-table" aria-label="Liste des fichiers sources disponibles">
                    <thead>
                        <tr>
                            <th scope="col">"Nom du fichier"</th>
                            <th scope="col">"Taille"</th>
                            <th scope="col">"Action"</th>
                        </tr>
                    </thead>
                    <tbody id="files-table-body">
                        <tr>
                            <td colspan="3" class="empty-state-text">
                                "Chargement des fichiers sources…"
                            </td>
                        </tr>
                    </tbody>
                </table>
            </div>
        </section>
    }
}
