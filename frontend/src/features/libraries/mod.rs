//! Gestion des bibliothèques de médias, des accès par nœud et des ressources associées.

pub mod file_browser;
pub mod library_card;
pub mod library_resources;
pub mod node_access;

use file_browser::FileBrowser;
use leptos::prelude::*;
use library_card::LibraryCard;
use library_resources::LibraryResources;
use node_access::NodePathConfig;

#[component]
pub fn LibrariesPanel() -> impl IntoView {
    let main_library_nodes = vec![
        NodePathConfig {
            node_id: "node-coordinator",
            node_name: "Coordinateur (TRUENAS)",
            local_path: "/input",
            access_mode: "Accès direct partagé",
            is_online: true,
            is_writable: false,
        },
        NodePathConfig {
            node_id: "node-artemis",
            node_name: "Worker ARTEMIS (GPU)",
            local_path: "/mnt/artemis/media/input",
            access_mode: "Accès direct distant",
            is_online: true,
            is_writable: false,
        },
    ];

    let audio_library_nodes = vec![
        NodePathConfig {
            node_id: "node-coordinator",
            node_name: "Coordinateur (TRUENAS)",
            local_path: "/music",
            access_mode: "Accès direct partagé",
            is_online: true,
            is_writable: true,
        },
        NodePathConfig {
            node_id: "node-artemis",
            node_name: "Worker ARTEMIS (GPU)",
            local_path: "/mnt/artemis/music",
            access_mode: "Accès direct distant",
            is_online: true,
            is_writable: true,
        },
    ];

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
                            "Bibliothèques & Dossiers configurés"
                        </h2>
                        <p class="section-subtitle">
                            "Gestion multi-nœuds des volumes de stockage, assignation automatique des flux et surveillance sans contrainte d'extension."
                        </p>
                    </div>
                </div>

                <div style="display: grid; grid-template-columns: repeat(auto-fit, minmax(320px, 1fr)); gap: 1.25rem; margin-bottom: 1.5rem;">
                    <LibraryCard
                        name="Bibliothèque Vidéo Principale"
                        media_type="🎬 Vidéo"
                        status="Surveillance active"
                        assigned_flow="Remuxage Matroska propre"
                        nodes=main_library_nodes
                    />

                    <LibraryCard
                        name="Médiathèque Audio & Musique"
                        media_type="🎵 Audio & Musique"
                        status="Surveillance active"
                        assigned_flow="Extraction & Normalisation Audio EBU R128"
                        nodes=audio_library_nodes
                    />
                </div>
            </section>

            // Synthèse des flows et plugins associés
            <LibraryResources />

            // Explorateur de médias
            <div style="margin-top: 1.5rem;">
                <FileBrowser />
            </div>
        </div>
    }
}
