//! Domaine Réglages : Session et authentification sécurisée.

use leptos::prelude::*;

#[component]
pub fn SettingsAccess() -> impl IntoView {
    view! {
        <section class="section-panel" aria-labelledby="session-settings-title">
            <div class="section-panel-header">
                <div>
                    <h2 id="session-settings-title" class="section-title">
                        "Session & Authentification"
                    </h2>
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
    }
}
