//! Zone centrale : Plan de travail graphique (Canvas SVG & Nœuds).

use leptos::prelude::*;

#[component]
pub fn FlowCanvas() -> impl IntoView {
    view! {
        <div class="canvas-board-wrapper" id="canvas-board-area" tabindex="0" role="region" aria-label="Plan de travail graphique du flux">
            <svg class="canvas-connections-svg" id="canvas-svg-lines" aria-hidden="true">
                // Tracés SVG de liaison générés dynamiquement par le script flows.js
            </svg>

            <div class="canvas-nodes-container" id="canvas-nodes-container">
                // Nœud 1 : Entrée Fichier
                <div class="canvas-node selected" id="node-1" data-node-id="node-1" data-block-type="file-input">
                    <div class="node-connector connector-out"></div>
                    <div class="node-header">
                        <span class="node-icon">"📄"</span>
                        <div class="node-title-box">
                            <span class="node-kind">"Déclencheur entrant"</span>
                            <h4 class="node-name">"Fichier source"</h4>
                        </div>
                        <span class="status-pill success node-pill">"Entrée"</span>
                    </div>
                    <div class="node-summary">
                        "Détection du média • Métadonnées inspectées automatiquement en sous-main."
                    </div>
                </div>

                // Nœud 2 : FFmpeg Video Encode
                <div class="canvas-node" id="node-2" data-node-id="node-2" data-block-type="video-encode">
                    <div class="node-connector connector-in"></div>
                    <div class="node-connector connector-out"></div>
                    <div class="node-header">
                        <span class="node-icon">"🎞️"</span>
                        <div class="node-title-box">
                            <span class="node-kind">"FFmpeg Builder"</span>
                            <h4 class="node-name">"Réencodage Vidéo"</h4>
                        </div>
                        <button type="button" class="node-remove-btn" title="Supprimer ce bloc">"✕"</button>
                    </div>
                    <div class="node-summary" id="node-2-summary">
                        "Codec : HEVC • Encodeur : Intel QSV • Qualité : 6 • Vitesse : 3"
                    </div>
                </div>

                // Nœud 3 : FFmpeg Audio Encode
                <div class="canvas-node" id="node-3" data-node-id="node-3" data-block-type="audio-encode">
                    <div class="node-connector connector-in"></div>
                    <div class="node-connector connector-out"></div>
                    <div class="node-header">
                        <span class="node-icon">"🔊"</span>
                        <div class="node-title-box">
                            <span class="node-kind">"FFmpeg Builder"</span>
                            <h4 class="node-name">"Réencodage Audio"</h4>
                        </div>
                        <button type="button" class="node-remove-btn" title="Supprimer ce bloc">"✕"</button>
                    </div>
                    <div class="node-summary" id="node-3-summary">
                        "Codec : AAC • Débit : 192 kbps • Canaux : Stéréo"
                    </div>
                </div>

                // Nœud 4 : MKVToolNix Remux
                <div class="canvas-node" id="node-4" data-node-id="node-4" data-block-type="remux-mkv">
                    <div class="node-connector connector-in"></div>
                    <div class="node-header">
                        <span class="node-icon">"🎬"</span>
                        <div class="node-title-box">
                            <span class="node-kind">"MKVToolNix"</span>
                            <h4 class="node-name">"Remuxage Matroska"</h4>
                        </div>
                        <button type="button" class="node-remove-btn" title="Supprimer ce bloc">"✕"</button>
                    </div>
                    <div class="node-summary" id="node-4-summary">
                        "Assemblage final MKV • Langues conservées : fre, eng"
                    </div>
                </div>
            </div>
        </div>
    }
}
