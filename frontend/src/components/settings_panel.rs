//! Panneau des paramètres, session web sécurisée et diagnostics.

use leptos::prelude::*;

#[component]
pub fn SettingsPanel() -> impl IntoView {
    view! {
        <div id="panel-settings" class="tab-panel" role="tabpanel" aria-labelledby="tab-btn-settings" hidden>
            <section class="section-panel" aria-labelledby="session-settings-title">
                <div class="section-panel-header">
                    <div>
                        <h2 id="session-settings-title" class="section-title">"Session & Authentification"</h2>
                        <p class="section-subtitle">
                            "Gestion sécurisée de l'accès administrateur. Aucune clé secrète n'est conservée en clair dans sessionStorage ou localStorage."
                        </p>
                    </div>
                </div>

                <div class="form-group">
                    <label for="admin-key-input" class="form-label">
                        "Clé d'administration (ADMIN_KEY) ou jeton de session"
                    </label>
                    <div style="display: flex; gap: 0.75rem; flex-wrap: wrap;">
                        <input
                            type="password"
                            id="admin-key-input"
                            class="form-input"
                            placeholder="Clé secrète (ex: issue de var/secrets/admin_key)"
                            autocomplete="off"
                            spellcheck="false"
                            style="flex: 1; min-width: 260px;"
                        />
                        <button
                            type="button"
                            id="admin-key-save-btn"
                            class="btn btn-primary"
                        >
                            "Ouvrir la session"
                        </button>
                        <button
                            type="button"
                            id="admin-key-logout-btn"
                            class="btn btn-secondary"
                            style="display: none;"
                        >
                            "Fermer la session"
                        </button>
                    </div>
                    <div id="auth-status-box" style="margin-top: 0.5rem;">
                        <p id="auth-status-text" class="form-help">
                            "🔒 Mode mémoire volatile actif : la clé réside uniquement dans la mémoire JavaScript de l'onglet actif. Recharger ou fermer l'onglet efface instantanément la clé."
                        </p>
                    </div>
                </div>
            </section>

            <section class="section-panel" aria-labelledby="theme-settings-title">
                <div class="section-panel-header">
                    <div>
                        <h2 id="theme-settings-title" class="section-title">"Préférences d'affichage"</h2>
                        <p class="section-subtitle">"Personnalisez l'apparence selon vos conditions de visionnage."</p>
                    </div>
                </div>
                <div class="form-group" style="max-width: 320px;">
                    <label for="theme-select" class="form-label">"Thème de couleur"</label>
                    <select id="theme-select" class="form-select">
                        <option value="system">"Système (automatique)"</option>
                        <option value="light">"Clair (contrastes renforcés)"</option>
                        <option value="dark">"Sombre (haute lisibilité)"</option>
                    </select>
                    <p class="form-help">
                        "Respecte automatiquement le paramètre prefers-color-scheme de votre système d'exploitation."
                    </p>
                </div>
            </section>

            <section class="section-panel" aria-labelledby="diagnostic-settings-title">
                <div class="section-panel-header">
                    <div>
                        <h2 id="diagnostic-settings-title" class="section-title">"Diagnostics et points d'accès"</h2>
                        <p class="section-subtitle">"Informations de raccordement réseau et état du coordinateur."</p>
                    </div>
                </div>
                <div class="detail-grid">
                    <div class="detail-term">"URL de l'interface"</div>
                    <div class="detail-val">"http://localhost:3000"</div>

                    <div class="detail-term">"API REST unifiée"</div>
                    <div class="detail-val">"/api/v1/ (same-origin, port unique HTTP 3000)"</div>

                    <div class="detail-term">"Architecture & Stockage"</div>
                    <div class="detail-val">"Modèle *arr conteneur unique • SQLite /config/flux.db (WAL)"</div>

                    <div class="detail-term">"Worker local intégré"</div>
                    <div class="detail-val" id="settings-local-worker">"—"</div>

                    <div class="detail-term">"Version"</div>
                    <div class="detail-val" id="settings-version">"—"</div>

                    <div class="detail-term">"Contrat d'API"</div>
                    <div class="detail-val">"OpenAPI 3.1 • docs/openapi.yaml"</div>

                    <div class="detail-term">"Moteur de remux"</div>
                    <div class="detail-val">"MKVToolNix (mkvmerge) natif"</div>
                </div>
            </section>
        </div>
    }
}
