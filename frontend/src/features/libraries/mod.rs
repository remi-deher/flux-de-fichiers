//! Gestion des bibliothèques de médias sources et sélection des flux.

pub mod file_browser;
pub mod library_card;

use file_browser::FileBrowser;
use leptos::prelude::*;
use library_card::LibraryCard;

#[component]
pub fn LibrariesPanel() -> impl IntoView {
    view! {
        <div
            id="panel-files"
            class="tab-panel"
            role="tabpanel"
            aria-labelledby="tab-btn-files"
            hidden
        >
            <section class="section-panel" aria-labelledby="libraries-section-title">
                <div class="section-panel-header">
                    <div>
                        <h2 id="libraries-section-title" class="section-title">
                            "Bibliothèques & Dossiers surveillés"
                        </h2>
                        <p class="section-subtitle">
                            "Surveillance continue des volumes de stockage d'entrée et association automatique des flux."
                        </p>
                    </div>
                </div>

                <div style="display: grid; grid-template-columns: repeat(auto-fit, minmax(280px, 1fr)); gap: 1rem; margin-bottom: 1rem;">
                    <LibraryCard
                        name="Bibliothèque Principale (Entrée)"
                        path="/var/input"
                        pattern="*.mkv"
                        status="Surveillance active"
                        flow="Remuxage Matroska propre (défaut)"
                    />
                </div>
            </section>

            <FileBrowser />
        </div>
    }
}
