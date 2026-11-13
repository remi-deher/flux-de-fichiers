//! Écran d'accès : installation de la centrale au premier démarrage, puis connexion.
//!
//! Le script client choisit le formulaire à afficher d'après `GET /api/v1/setup/status` et `/api/v1/me`.

use crate::shared::ui::{Button, ButtonVariant, FormField};
use leptos::prelude::*;

/// Longueur minimale d'un mot de passe, annoncée par le navigateur (le serveur la vérifie aussi).
const PASSWORD_MIN_LENGTH: u32 = 8;

#[component]
pub fn AuthGate() -> impl IntoView {
    view! {
        <div id="auth-gate" class="auth-gate" role="dialog" aria-modal="true" aria-labelledby="auth-gate-title">
            <div class="auth-gate__card">
                <div class="auth-gate__brand">
                    <span class="auth-gate__logo">"MKV"</span>
                    <span class="auth-gate__name">"Flux de Fichiers"</span>
                </div>
                <h1 id="auth-gate-title" class="auth-gate__title">"Chargement…"</h1>
                <p id="auth-gate-subtitle" class="auth-gate__subtitle"></p>

                <form id="setup-form" class="auth-gate__form" hidden>
                    <FormField id="setup-name" label="Nom de la centrale" required=true autocomplete="off" help="Affiché dans l'interface et dans la liste des nœuds." />
                    <FormField id="setup-username" label="Identifiant administrateur" required=true autocomplete="username" />
                    <FormField id="setup-password" label="Mot de passe" input_type="password" required=true minlength=PASSWORD_MIN_LENGTH autocomplete="new-password" help="Au moins 8 caractères." />
                    <FormField id="setup-password-confirm" label="Confirmer le mot de passe" input_type="password" required=true minlength=PASSWORD_MIN_LENGTH autocomplete="new-password" />
                    <p class="auth-gate__notice">
                        "Tant que l'installation n'est pas terminée, toute personne qui atteint cette page peut la terminer : faites-le tout de suite."
                    </p>
                    <Button id="setup-submit-btn" variant=ButtonVariant::Primary submit=true class="auth-gate__submit">
                        "Installer la centrale"
                    </Button>
                </form>

                <form id="login-form" class="auth-gate__form" hidden>
                    <FormField id="login-username" label="Identifiant" required=true autocomplete="username" />
                    <FormField id="login-password" label="Mot de passe" input_type="password" required=true autocomplete="current-password" />
                    <Button id="login-submit-btn" variant=ButtonVariant::Primary submit=true class="auth-gate__submit">
                        "Se connecter"
                    </Button>
                </form>

                <p id="auth-gate-error" class="form-error" role="alert"></p>
            </div>
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn render() -> String {
        view! { <AuthGate /> }.to_html()
    }

    #[test]
    fn test_gate_has_both_forms_hidden_until_the_script_chooses() {
        let html = render();
        for id in [
            "setup-form",
            "login-form",
            "auth-gate-error",
            "auth-gate-title",
        ] {
            assert!(html.contains(&format!("id=\"{id}\"")), "{id}");
        }
        assert_eq!(html.matches(" hidden").count(), 2);
    }

    #[test]
    fn test_setup_collects_name_account_and_a_confirmed_password() {
        let html = render();
        for id in [
            "setup-name",
            "setup-username",
            "setup-password",
            "setup-password-confirm",
            "setup-submit-btn",
        ] {
            assert!(html.contains(&format!("id=\"{id}\"")), "{id}");
        }
        assert!(html.contains("autocomplete=\"new-password\""));
        assert!(html.contains("autocomplete=\"current-password\""));
    }

    #[test]
    fn test_submit_buttons_submit_their_form() {
        let html = render();
        assert_eq!(html.matches("type=\"submit\"").count(), 2);
    }

    #[test]
    fn test_gate_has_no_inline_style() {
        assert!(!render().contains("style="));
    }
}
