//! Panneau de gestion des plugins et outils distribués.

use leptos::prelude::*;

#[component]
pub fn PluginsPanel() -> impl IntoView {
    view! {
        <div
            id="panel-plugins"
            class="tab-panel"
            role="tabpanel"
            aria-labelledby="tab-btn-plugins"
            hidden
        >
            // 1. Catalogue des outils distribués
            <section class="section-panel" aria-labelledby="tools-catalogue-title">
                <div class="section-panel-header">
                    <div>
                        <h2 id="tools-catalogue-title" class="section-title">
                            "Catalogue des outils distribués"
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
                    <article class="tool-card" data-tool-id="tool.mediainfo" data-cache-status="not_installed">
                        <div class="tool-card-header">
                            <div class="tool-identity">
                                <span class="tool-badge-icon" aria-hidden="true">"🔍"</span>
                                <div>
                                    <h3 class="tool-card-title">"MediaInfo CLI"</h3>
                                    <span class="tool-card-id">"tool.mediainfo • v24.06"</span>
                                </div>
                            </div>
                            <span class="status-pill" title="Outil disponible au téléchargement mais non extrait">
                                <span class="status-dot"></span>
                                "Non installé"
                            </span>
                        </div>
                        <p class="tool-card-desc">
                            "Analyse technique exhaustive et extraction des métadonnées vidéo/audio/sous-titres en début de flux."
                        </p>
                        <div class="tool-meta-tags">
                            <span class="meta-tag">"Capacité : inspect"</span>
                            <span class="meta-tag">"Choix actif : Oui"</span>
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
        </div>
    }
}
