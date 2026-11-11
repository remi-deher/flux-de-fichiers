//! Panneau de gestion des plugins, outils distribués et dépôts de sources.

use crate::core::{SubNav, SubNavTabItem};
use leptos::prelude::*;

#[component]
pub fn PluginsPanel() -> impl IntoView {
    let subnav_tabs = vec![
        SubNavTabItem {
            id: "btn-subtab-plugins-installed",
            label: "Outils installés",
            icon: Some("🔌"),
            target_id: "subtab-plugins-installed",
            is_active: true,
            badge: None,
        },
        SubNavTabItem {
            id: "btn-subtab-plugins-store",
            label: "Catalogue distant",
            icon: Some("🌐"),
            target_id: "subtab-plugins-store",
            is_active: false,
            badge: None,
        },
        SubNavTabItem {
            id: "btn-subtab-plugins-repos",
            label: "Dépôts de sources",
            icon: Some("📦"),
            target_id: "subtab-plugins-repos",
            is_active: false,
            badge: None,
        },
    ];

    let subnav_info = view! {
        <span class="subnav-info-badge">"📦 Dépôt officiel actif"</span>
    }
    .into_any();

    let subnav_actions = view! {
        <button
            type="button"
            id="btn-open-add-repo-dialog"
            class="btn btn-primary btn-sm"
            aria-label="Ajouter un nouveau dépôt de plugins"
        >
            "+ Ajouter un dépôt"
        </button>
        <button
            type="button"
            id="btn-sync-all-repos"
            class="btn btn-secondary btn-sm"
            aria-label="Synchroniser l'ensemble des catalogues"
        >
            "🔄 Synchroniser"
        </button>
    }
    .into_any();

    view! {
        <div
            id="panel-plugins"
            class="tab-panel"
            role="tabpanel"
            aria-labelledby="tab-btn-plugins"
            hidden
        >
            <SubNav
                aria_label="Sous-navigation de la section Plugins et Dépôts"
                tabs=subnav_tabs
                info=subnav_info
                actions=subnav_actions
            />

            // --- Sous-vue 1 : Outils installés et connecteurs locaux ---
            <div id="subtab-plugins-installed">
                <section class="section-panel" aria-labelledby="tools-catalogue-title">
                    <div class="section-panel-header">
                        <div>
                            <h2 id="tools-catalogue-title" class="section-title">
                                "Outils distribués installés"
                            </h2>
                            <p class="section-subtitle">
                                "Binaires d'exécution autonomes vérifiés par empreinte SHA-256 stricte et isolés du socle conteneur."
                            </p>
                        </div>
                        <div class="tools-filter-bar" role="toolbar" aria-label="Filtres du catalogue d'outils">
                            <button
                                type="button"
                                class="filter-chip active"
                                data-tool-filter="all"
                                aria-pressed="true"
                            >
                                "Tous les outils"
                            </button>
                            <button
                                type="button"
                                class="filter-chip"
                                data-tool-filter="ready"
                                aria-pressed="false"
                            >
                                "Prêts (.ready)"
                            </button>
                            <button
                                type="button"
                                class="filter-chip"
                                data-tool-filter="not_installed"
                                aria-pressed="false"
                            >
                                "Non installés"
                            </button>
                        </div>
                    </div>

                    <div class="tools-grid" id="tools-cards-container" aria-label="Cartes des outils déclarés">
                        // Carte 1 : MKVToolNix
                        <article class="tool-card" data-tool-id="tool.mkvtoolnix" data-cache-status="ready">
                            <div class="tool-card-header">
                                <div class="tool-identity">
                                    <span class="tool-badge-icon" aria-hidden="true">"🎬"</span>
                                    <div>
                                        <h3 class="tool-card-title">"MKVToolNix (mkvmerge)"</h3>
                                        <span class="tool-card-id">"tool.mkvtoolnix • v1.0.0"</span>
                                    </div>
                                </div>
                                <span class="status-pill success" title="Binaire intègre et prêt">
                                    <span class="status-dot"></span>
                                    ".ready (Prêt)"
                                </span>
                            </div>
                            <p class="tool-card-desc">
                                "Outil de remuxage sans réencodage validé. Distribution autonome hors conteneur applicatif. Matroska uniquement."
                            </p>
                            <div class="tool-meta-tags">
                                <span class="meta-tag">"Capacité : remux"</span>
                                <span class="meta-tag active-choice">"Choix actif : Oui"</span>
                                <span class="meta-tag">"Arch : x86_64, aarch64"</span>
                                <span class="meta-tag">"Mode : 0755"</span>
                            </div>
                        </article>

                        // Carte 2 : FFmpeg
                        <article class="tool-card" data-tool-id="tool.ffmpeg" data-cache-status="not_installed">
                            <div class="tool-card-header">
                                <div class="tool-identity">
                                    <span class="tool-badge-icon" aria-hidden="true">"🎞️"</span>
                                    <div>
                                        <h3 class="tool-card-title">"FFmpeg (distribué)"</h3>
                                        <span class="tool-card-id">"tool.ffmpeg • v7.1.0"</span>
                                    </div>
                                </div>
                                <span class="status-pill" title="Outil disponible au téléchargement mais non extrait">
                                    <span class="status-dot"></span>
                                    "Non installé"
                                </span>
                            </div>
                            <p class="tool-card-desc">
                                "Outil d'analyse multimédia, transcodage et génération de médias synthétiques de tests."
                            </p>
                            <div class="tool-meta-tags">
                                <span class="meta-tag">"Capacité : analyze / encode"</span>
                                <span class="meta-tag">"Choix actif : Non"</span>
                                <span class="meta-tag">"Arch : x86_64, aarch64"</span>
                                <span class="meta-tag">"Distribution : binaire autonome"</span>
                            </div>
                        </article>

                        // Carte 3 : MP4Box
                        <article class="tool-card" data-tool-id="tool.mp4box" data-cache-status="not_installed">
                            <div class="tool-card-header">
                                <div class="tool-identity">
                                    <span class="tool-badge-icon" aria-hidden="true">"📦"</span>
                                    <div>
                                        <h3 class="tool-card-title">"MP4Box (GPAC)"</h3>
                                        <span class="tool-card-id">"tool.mp4box • v2.4.0"</span>
                                    </div>
                                </div>
                                <span class="status-pill" title="Outil disponible au téléchargement mais non extrait">
                                    <span class="status-dot"></span>
                                    "Non installé"
                                </span>
                            </div>
                            <p class="tool-card-desc">
                                "Outil de multiplexage et optimisation MP4 (faststart / hint tracks)."
                            </p>
                            <div class="tool-meta-tags">
                                <span class="meta-tag">"Capacité : remux / mp4"</span>
                                <span class="meta-tag">"Choix actif : Non"</span>
                                <span class="meta-tag">"Arch : x86_64, aarch64"</span>
                            </div>
                        </article>

                        // Carte 4 : MediaInfo
                        <article class="tool-card" data-tool-id="tool.mediainfo" data-cache-status="ready">
                            <div class="tool-card-header">
                                <div class="tool-identity">
                                    <span class="tool-badge-icon" aria-hidden="true">"🔍"</span>
                                    <div>
                                        <h3 class="tool-card-title">"MediaInfo CLI"</h3>
                                        <span class="tool-card-id">"tool.mediainfo • v24.06"</span>
                                    </div>
                                </div>
                                <span class="status-pill success" title="Outil d'inspection intégré nativement dans l'environnement">
                                    <span class="status-dot"></span>
                                    ".ready (Intégrée)"
                                </span>
                            </div>
                            <p class="tool-card-desc">
                                "Analyse technique exhaustive et extraction des métadonnées vidéo/audio/sous-titres en tâche de fond."
                            </p>
                            <div class="tool-meta-tags">
                                <span class="meta-tag">"Capacité : probe"</span>
                                <span class="meta-tag active-choice">"Choix actif : Oui"</span>
                                <span class="meta-tag">"Arch : x86_64, aarch64"</span>
                            </div>
                        </article>

                        // Carte 5 : Sonde de télémétrie interne
                        <article class="tool-card" data-tool-id="tool.internal.telemetry" data-cache-status="ready">
                            <div class="tool-card-header">
                                <div class="tool-identity">
                                    <span class="tool-badge-icon" aria-hidden="true">"📊"</span>
                                    <div>
                                        <h3 class="tool-card-title">"Sonde de télémétrie système"</h3>
                                        <span class="tool-card-id">"tool.internal.telemetry • v0.1.0"</span>
                                    </div>
                                </div>
                                <span class="status-pill success" title="Sonde intégrée nativement dans le binaire applicatif">
                                    <span class="status-dot"></span>
                                    ".ready (Intégrée)"
                                </span>
                            </div>
                            <p class="tool-card-desc">
                                "Mesure périodique CPU, RAM, GPU (/dev/dri) et stockage local. Confinement strict sous UID 10001 (app) sans socket Docker."
                            </p>
                            <div class="tool-meta-tags">
                                <span class="meta-tag">"Capacité : sonde système"</span>
                                <span class="meta-tag active-choice">"Intégrée : Oui"</span>
                                <span class="meta-tag">"Mode : offline-preseeded"</span>
                                <span class="meta-tag">"Accès : non-root"</span>
                            </div>
                        </article>
                    </div>
                </section>

                // Section Connecteurs (*arr)
                <section class="section-panel" aria-labelledby="integrations-title">
                    <div class="section-panel-header">
                        <div>
                            <h2 id="integrations-title" class="section-title">"Intégrations & Connecteurs (*arr)"</h2>
                            <p class="section-subtitle">
                                "Connecteurs vers vos gestionnaires multimédias. Les valeurs par défaut configurées ici sont automatiquement héritées par les blocs de vos flux."
                            </p>
                        </div>
                    </div>

                    <div class="tools-grid">
                        // Plugin Sonarr
                        <article class="tool-card" data-plugin-id="plugin.sonarr">
                            <div class="tool-card-header">
                                <div class="tool-identity">
                                    <span class="tool-badge-icon" aria-hidden="true">"📡"</span>
                                    <div>
                                        <h3 class="tool-card-title">"Sonarr (Séries TV)"</h3>
                                        <span class="tool-card-id">"plugin.sonarr • v1.0.0"</span>
                                    </div>
                                </div>
                                <span class="status-pill info">
                                    <span class="status-dot"></span>
                                    "Actif"
                                </span>
                            </div>
                            <p class="tool-card-desc">
                                "Notification post-traitement, actualisation de série (rescan) et déclenchement du renommage automatique de fichiers."
                            </p>
                            <div class="tool-meta-tags">
                                <span class="meta-tag">"Type : plugin"</span>
                                <span class="meta-tag">"API : v3 REST"</span>
                            </div>
                            <div style="margin-top: 1rem; display: flex; justify-content: flex-end;">
                                <button
                                    type="button"
                                    class="btn btn-secondary btn-sm open-plugin-config-btn"
                                    data-plugin="plugin.sonarr"
                                    data-plugin-name="Sonarr"
                                >
                                    "⚙️ Paramètres par défaut"
                                </button>
                            </div>
                        </article>

                        // Plugin Radarr
                        <article class="tool-card" data-plugin-id="plugin.radarr">
                            <div class="tool-card-header">
                                <div class="tool-identity">
                                    <span class="tool-badge-icon" aria-hidden="true">"🎬"</span>
                                    <div>
                                        <h3 class="tool-card-title">"Radarr (Films)"</h3>
                                        <span class="tool-card-id">"plugin.radarr • v1.0.0"</span>
                                    </div>
                                </div>
                                <span class="status-pill info">
                                    <span class="status-dot"></span>
                                    "Actif"
                                </span>
                            </div>
                            <p class="tool-card-desc">
                                "Notification post-traitement, mise à jour des caractéristiques techniques de film et renommage automatique de collection."
                            </p>
                            <div class="tool-meta-tags">
                                <span class="meta-tag">"Type : plugin"</span>
                                <span class="meta-tag">"API : v3 REST"</span>
                            </div>
                            <div style="margin-top: 1rem; display: flex; justify-content: flex-end;">
                                <button
                                    type="button"
                                    class="btn btn-secondary btn-sm open-plugin-config-btn"
                                    data-plugin="plugin.radarr"
                                    data-plugin-name="Radarr"
                                >
                                    "⚙️ Paramètres par défaut"
                                </button>
                            </div>
                        </article>
                    </div>
                </section>
            </div>

            // --- Sous-vue 2 : Catalogue distant (Store unifié) ---
            <div id="subtab-plugins-store" hidden>
                <section class="section-panel" aria-labelledby="store-section-title">
                    <div class="section-panel-header">
                        <div>
                            <h2 id="store-section-title" class="section-title">
                                "Catalogue distant (Store unifié)"
                            </h2>
                            <p class="section-subtitle">
                                "Explorez et installez des extensions, outils et flux validés distribués par vos dépôts connectés."
                            </p>
                        </div>
                        <div class="tools-filter-bar" role="toolbar" aria-label="Filtres par type de composant store">
                            <button
                                type="button"
                                class="filter-chip store-filter-chip active"
                                data-store-filter="all"
                                aria-pressed="true"
                            >
                                "Tout afficher"
                            </button>
                            <button
                                type="button"
                                class="filter-chip store-filter-chip"
                                data-store-filter="flow"
                                aria-pressed="false"
                            >
                                "⚡ Flux"
                            </button>
                            <button
                                type="button"
                                class="filter-chip store-filter-chip"
                                data-store-filter="tool"
                                aria-pressed="false"
                            >
                                "🔧 Outils"
                            </button>
                        </div>
                    </div>

                    <div class="tools-grid" id="store-cards-container" aria-label="Composants distants disponibles">
                        <div class="empty-state-text" style="grid-column: 1 / -1; padding: 2rem;">
                            "Chargement du catalogue distant…"
                        </div>
                    </div>
                </section>
            </div>

            // --- Sous-vue 3 : Gestion Multi-Dépôts ---
            <div id="subtab-plugins-repos" hidden>
                <section class="section-panel" aria-labelledby="repos-section-title">
                    <div class="section-panel-header">
                        <div>
                            <h2 id="repos-section-title" class="section-title">
                                "Dépôts de distribution configurés"
                            </h2>
                            <p class="section-subtitle">
                                "Gérez les sources distantes de flux et d'outils. Les catalogues actifs sont interrogés pour alimenter le store."
                            </p>
                        </div>
                    </div>

                    <div class="table-wrapper">
                        <table class="data-table" aria-label="Liste des dépôts de plugins enregistrés">
                            <thead>
                                <tr>
                                    <th scope="col">"Dépôt"</th>
                                    <th scope="col">"URL d'index"</th>
                                    <th scope="col">"Type"</th>
                                    <th scope="col">"État"</th>
                                    <th scope="col">"Dernière synchro"</th>
                                    <th scope="col" style="text-align: right;">"Actions"</th>
                                </tr>
                            </thead>
                            <tbody id="repos-table-body">
                                <tr>
                                    <td colspan="6" class="empty-state-text">
                                        "Chargement des dépôts enregistrés…"
                                    </td>
                                </tr>
                            </tbody>
                        </table>
                    </div>
                </section>
            </div>

            // Dialogue modal d'ajout d'un dépôt de plugins
            <dialog id="add-repo-dialog" class="app-dialog" aria-labelledby="add-repo-title" aria-modal="true" style="max-width: 540px;">
                <div class="dialog-header">
                    <h3 id="add-repo-title" class="dialog-title">"Ajouter un dépôt de sources"</h3>
                    <button
                        type="button"
                        id="add-repo-close-x"
                        class="btn btn-secondary btn-sm"
                        aria-label="Fermer"
                    >
                        "✕"
                    </button>
                </div>
                <div class="dialog-body">
                    <p class="section-subtitle" style="margin-top: 0;">
                        "Renseignez l'URL HTTP(S) pointant vers le fichier d'index JSON (schema catalogue v1) du dépôt distant."
                    </p>
                    <div class="form-group">
                        <label for="repo-name-input" class="form-label">"Nom du dépôt"</label>
                        <input
                            type="text"
                            id="repo-name-input"
                            class="form-input"
                            placeholder="Ex : Dépôt Communautaire Omarchy"
                            required
                        />
                    </div>
                    <div class="form-group">
                        <label for="repo-url-input" class="form-label">"URL de l'index (index.json)"</label>
                        <input
                            type="url"
                            id="repo-url-input"
                            class="form-input"
                            placeholder="https://raw.githubusercontent.com/.../catalogue/index.json"
                            required
                        />
                    </div>
                </div>
                <div class="dialog-footer" style="display: flex; justify-content: flex-end; gap: 0.75rem; padding-top: 1rem; border-top: 1px solid var(--border-color); margin-top: 1rem;">
                    <button
                        type="button"
                        id="add-repo-cancel-btn"
                        class="btn btn-secondary"
                    >
                        "Annuler"
                    </button>
                    <button
                        type="button"
                        id="add-repo-save-btn"
                        class="btn btn-primary"
                    >
                        "Ajouter le dépôt"
                    </button>
                </div>
            </dialog>

            // Dialogue modal générique de configuration de plugin
            <dialog id="plugin-config-dialog" class="app-dialog" aria-labelledby="plugin-config-title" aria-modal="true" style="max-width: 600px;">
                <div class="dialog-header">
                    <h3 id="plugin-config-title" class="dialog-title">"Configuration du plugin"</h3>
                    <button
                        type="button"
                        id="plugin-config-close-x"
                        class="btn btn-secondary btn-sm"
                        aria-label="Fermer"
                    >
                        "✕"
                    </button>
                </div>
                <div class="dialog-body">
                    <p class="section-subtitle" style="margin-top: 0;">
                        "Ces paramètres sont sauvegardés dans votre base centrale SQLite (/config) et sont automatiquement transmis aux blocs correspondants dans les flux."
                    </p>
                    <input type="hidden" id="plugin-config-current-id" value="" />
                    <div id="plugin-config-fields-container">
                        <div class="form-group">
                            <label for="plugin-cfg-url" class="form-label">"URL du serveur"</label>
                            <input type="url" id="plugin-cfg-url" class="form-input" placeholder="http://192.168.1.50:8989" />
                        </div>
                        <div class="form-group">
                            <label for="plugin-cfg-apikey" class="form-label">"Clé d'API"</label>
                            <input type="password" id="plugin-cfg-apikey" class="form-input" placeholder="Clé secrète issue de l'application" />
                        </div>
                        <div style="display: grid; grid-template-columns: 1fr 1fr; gap: 0.75rem;">
                            <div class="form-group">
                                <label for="plugin-cfg-local-path" class="form-label">"Chemin local"</label>
                                <input type="text" id="plugin-cfg-local-path" class="form-input" value="/data" />
                            </div>
                            <div class="form-group">
                                <label for="plugin-cfg-remote-path" class="form-label">"Chemin distant (*arr)"</label>
                                <input type="text" id="plugin-cfg-remote-path" class="form-input" value="/media" />
                            </div>
                        </div>
                    </div>
                </div>
                <div class="dialog-footer" style="display: flex; justify-content: flex-end; gap: 0.75rem; padding-top: 1rem; border-top: 1px solid var(--border-color); margin-top: 1rem;">
                    <button
                        type="button"
                        id="plugin-config-cancel-btn"
                        class="btn btn-secondary"
                    >
                        "Annuler"
                    </button>
                    <button
                        type="button"
                        id="plugin-config-save-btn"
                        class="btn btn-primary"
                    >
                        "💾 Enregistrer les valeurs par défaut"
                    </button>
                </div>
            </dialog>
        </div>
    }
}
