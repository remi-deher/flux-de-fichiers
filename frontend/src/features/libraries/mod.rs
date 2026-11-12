//! Gestion des bibliothèques de médias, des accès par nœud et des ressources associées.

pub mod file_browser;
pub mod library_card;
pub mod library_resources;
pub mod node_access;

use crate::core::{SubNav, SubNavTabItem};
use file_browser::FileBrowser;
use leptos::prelude::*;
use library_card::LibraryCard;
use node_access::NodePathConfig;

#[component]
pub fn LibrariesPanel() -> impl IntoView {
    let main_library_nodes = vec![NodePathConfig {
        node_id: "node-coordinator",
        node_name: "Coordinateur local",
        local_path: "/input",
        access_mode: "Point de montage conteneur (/input)",
        is_online: true,
        is_writable: false,
    }];

    let subnav_tabs = vec![
        SubNavTabItem {
            id: "btn-subtab-files-browser",
            label: "Explorateur de fichiers",
            icon: Some("🔍"),
            target_id: "subtab-files-browser",
            is_active: true,
            badge: None,
        },
        SubNavTabItem {
            id: "btn-subtab-files-libraries",
            label: "Dossiers & Nœuds",
            icon: Some("📁"),
            target_id: "subtab-files-libraries",
            is_active: false,
            badge: Some("1"),
        },
    ];

    let subnav_info = view! {
        <span class="subnav-info-badge">"📁 Volume /input"</span>
    }
    .into_any();

    view! {
        <div
            id="panel-files"
            class="tab-panel"
            role="tabpanel"
            aria-labelledby="tab-btn-files"
            hidden
        >
            <SubNav
                aria_label="Sous-navigation de la section Bibliothèques"
                tabs=subnav_tabs
                info=subnav_info
            />

            // Sous-vue 1 : Explorateur de médias (actif par défaut)
            <div id="subtab-files-browser">
                <FileBrowser />
            </div>

            // Sous-vue 2 : Dossiers & Accès Nœuds
            <div id="subtab-files-libraries" hidden>
                <section class="section-panel" aria-labelledby="libraries-section-title">
                    <div class="section-panel-header">
                        <div>
                            <h2 id="libraries-section-title" class="section-title">
                                "Bibliothèque d'ingestion des médias (/input)"
                            </h2>
                            <p class="section-subtitle">
                                "Surveillance du dossier source, intégrité lecture seule et déclenchement des pipelines de remuxage sans altération de l'original."
                            </p>
                        </div>
                    </div>

                    <div style="margin-bottom: 1.5rem;">
                        <LibraryCard
                            name="Bibliothèque principale d'ingestion"
                            media_type="🎬 Médias Vidéo & Matroska"
                            status="Surveillance active"
                            assigned_flow="Remuxage Matroska propre"
                            nodes=main_library_nodes
                        />
                    </div>
                </section>
            </div>
        </div>
    }
}
