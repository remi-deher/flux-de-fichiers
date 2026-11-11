//! Volet latéral gauche : Palette des blocs disponibles (Rôle Construire).

use leptos::prelude::*;

#[component]
pub fn FlowPalette() -> impl IntoView {
    view! {
        <aside class="canvas-blocks-sidebar" aria-label="Bibliothèque de blocs disponibles">
            <div class="sidebar-block-group">
                <span class="sidebar-group-title">"📁 Entrées & Sorties"</span>
                <div class="draggable-block-item" data-block-type="file-input" data-tool="system">
                    <span class="block-drag-icon">"📄"</span>
                    <div class="block-drag-info">
                        <strong>"Fichier entrant"</strong>
                        <span>"Surveillance / Déclencheur"</span>
                    </div>
                </div>
                <div class="draggable-block-item addable" data-block-type="file-output" data-tool="system">
                    <span class="block-drag-icon">"💾"</span>
                    <div class="block-drag-info">
                        <strong>"Destination finale"</strong>
                        <span>"Remplacement / Déplacement"</span>
                    </div>
                    <span class="block-add-plus">"+"</span>
                </div>
            </div>

            <div class="sidebar-block-group">
                <span class="sidebar-group-title">"🔀 Logique & Conditions"</span>
                <div class="draggable-block-item addable" data-block-type="condition" data-tool="logic">
                    <span class="block-drag-icon">"🔀"</span>
                    <div class="block-drag-info">
                        <strong>"Aiguillage conditionnel"</strong>
                        <span>"Vrai / Faux / Repli matériel"</span>
                    </div>
                    <span class="block-add-plus">"+"</span>
                </div>
            </div>

            <div class="sidebar-block-group">
                <span class="sidebar-group-title">"🎞️ Traitement Vidéo"</span>
                <div class="draggable-block-item addable" data-block-type="video-encode" data-tool="ffmpeg">
                    <span class="block-drag-icon">"🎞️"</span>
                    <div class="block-drag-info">
                        <strong>"Réencoder Vidéo"</strong>
                        <span>"FFmpeg (HEVC/QSV/NVENC)"</span>
                    </div>
                    <span class="block-add-plus">"+"</span>
                </div>
                <div class="draggable-block-item addable" data-block-type="remux-mkv" data-tool="mkvtoolnix">
                    <span class="block-drag-icon">"🎬"</span>
                    <div class="block-drag-info">
                        <strong>"Remuxer MKV"</strong>
                        <span>"MKVToolNix (mkvmerge)"</span>
                    </div>
                    <span class="block-add-plus">"+"</span>
                </div>
                <div class="draggable-block-item addable" data-block-type="mkvpropedit" data-tool="mkvtoolnix">
                    <span class="block-drag-icon">"⚡"</span>
                    <div class="block-drag-info">
                        <strong>"Édition En-têtes (In-Place)"</strong>
                        <span>"mkvpropedit (Sans copie disque)"</span>
                    </div>
                    <span class="block-add-plus">"+"</span>
                </div>
                <div class="draggable-block-item addable" data-block-type="remux-mp4" data-tool="mp4box">
                    <span class="block-drag-icon">"📦"</span>
                    <div class="block-drag-info">
                        <strong>"Remuxer MP4"</strong>
                        <span>"MP4Box GPAC (FastStart)"</span>
                    </div>
                    <span class="block-add-plus">"+"</span>
                </div>
            </div>

            <div class="sidebar-block-group">
                <span class="sidebar-group-title">"🔊 Traitement Audio"</span>
                <div class="draggable-block-item addable" data-block-type="audio-encode" data-tool="ffmpeg">
                    <span class="block-drag-icon">"🔊"</span>
                    <div class="block-drag-info">
                        <strong>"Réencoder Audio"</strong>
                        <span>"FFmpeg (AAC/Opus/AC3)"</span>
                    </div>
                    <span class="block-add-plus">"+"</span>
                </div>
            </div>

            <div class="sidebar-block-group">
                <span class="sidebar-group-title">"🔗 Intégrations (*arr & Webhooks)"</span>
                <div class="draggable-block-item addable" data-block-type="webhook" data-tool="webhook">
                    <span class="block-drag-icon">"🌐"</span>
                    <div class="block-drag-info">
                        <strong>"Notification Webhook"</strong>
                        <span>"Envoi HTTP contextuel"</span>
                    </div>
                    <span class="block-add-plus">"+"</span>
                </div>
                <div class="draggable-block-item addable" data-block-type="sonarr" data-tool="sonarr">
                    <span class="block-drag-icon">"📡"</span>
                    <div class="block-drag-info">
                        <strong>"Sonarr : Mettre à jour"</strong>
                        <span>"Rescan & Renommage"</span>
                    </div>
                    <span class="block-add-plus">"+"</span>
                </div>
                <div class="draggable-block-item addable" data-block-type="radarr" data-tool="radarr">
                    <span class="block-drag-icon">"🎬"</span>
                    <div class="block-drag-info">
                        <strong>"Radarr : Mettre à jour"</strong>
                        <span>"Rescan & Renommage"</span>
                    </div>
                    <span class="block-add-plus">"+"</span>
                </div>
            </div>

            <div class="sidebar-help-tip">
                "💡 Cliquez sur un bloc avec « + » pour l'ajouter à la suite de votre chaîne."
            </div>
        </aside>
    }
}
