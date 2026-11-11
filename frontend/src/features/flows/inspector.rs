//! Volet latéral droit : Inspecteur et configuration du nœud sélectionné (Rôle Construire).

use leptos::prelude::*;

#[component]
pub fn FlowInspector() -> impl IntoView {
    view! {
        <aside class="canvas-inspector-sidebar" id="canvas-inspector" aria-label="Paramètres du bloc sélectionné">
            <div class="inspector-header">
                <h3 id="inspector-block-title" class="inspector-title">"Paramètres du bloc"</h3>
                <span id="inspector-block-tool" class="status-pill info">"FFmpeg"</span>
            </div>

            // Formulaire dynamique injecté selon le bloc sélectionné
            <div id="inspector-form-container">
                <div class="form-group">
                    <label for="node-cfg-codec" class="form-label">"Codec vidéo"</label>
                    <select id="node-cfg-codec" class="form-select">
                        <option value="hevc" selected="selected">"HEVC / H.265 (Recommandé)"</option>
                        <option value="h264">"H.264 / AVC"</option>
                        <option value="av1">"AV1"</option>
                        <option value="copy">"Copie directe"</option>
                    </select>
                </div>
                <div class="form-group">
                    <label for="node-cfg-encoder" class="form-label">"Encodeur matériel"</label>
                    <select id="node-cfg-encoder" class="form-select">
                        <option value="auto" selected="selected">"Intel QSV (Automatique)"</option>
                        <option value="nvenc">"Nvidia NVENC"</option>
                        <option value="vaapi">"VA-API (Linux)"</option>
                        <option value="cpu">"Processeur CPU (libx265)"</option>
                    </select>
                </div>
                <div class="form-group">
                    <label for="node-cfg-quality" class="form-label">
                        "Qualité (1-9) : " <span id="node-quality-val" style="font-weight: 700; color: var(--primary);">"6"</span>
                    </label>
                    <input type="range" id="node-cfg-quality" min="1" max="9" value="6" style="width: 100%; accent-color: var(--primary);" />
                </div>
                <div class="form-group">
                    <label for="node-cfg-speed" class="form-label">
                        "Vitesse (1-5) : " <span id="node-speed-val" style="font-weight: 700; color: var(--primary);">"3"</span>
                    </label>
                    <input type="range" id="node-cfg-speed" min="1" max="5" value="3" style="width: 100%; accent-color: var(--primary);" />
                </div>
                <div class="form-group">
                    <label for="node-cfg-custom" class="form-label">"Paramètres personnalisés libres"</label>
                    <input type="text" id="node-cfg-custom" class="form-input" placeholder="ex: -pix_fmt yuv420p10le" />
                </div>
            </div>
        </aside>
    }
}
