//! Composant d'authentification et gestion locale des accès API.

use leptos::prelude::*;

#[component]
pub fn AuthCard() -> impl IntoView {
    view! {
        <section class="section-panel" aria-labelledby="auth-card-title">
            <div class="section-panel-header">
                <div>
                    <h2 id="auth-card-title" class="section-title">"Authentification API"</h2>
                    <p class="section-subtitle">
                        "Les routes d'administration requièrent une clé Bearer. La clé reste confinée à votre navigateur local."
                    </p>
                </div>
            </div>
            <div class="form-group">
                <label for="admin-key-input" class="form-label">
                    "Clé administrateur (ADMIN_KEY)"
                </label>
                <div style="display: flex; gap: 0.75rem; flex-wrap: wrap;">
                    <input
                        type="password"
                        id="admin-key-input"
                        class="form-input"
                        placeholder="Exemple : secret généré dans var/secrets/admin_key"
                        autocomplete="off"
                        spellcheck="false"
                        style="flex: 1; min-width: 260px;"
                    />
                    <button
                        type="button"
                        id="admin-key-save-btn"
                        class="btn btn-primary"
                    >
                        "Enregistrer en session"
                    </button>
                </div>
                <p id="auth-status-text" class="form-help">
                    "En développement local, retrouvez la clé dans var/secrets/admin_key."
                </p>
            </div>
        </section>
    }
}
