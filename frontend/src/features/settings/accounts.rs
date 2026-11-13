//! Domaine Réglages : comptes des utilisateurs (rôle, désactivation, création).

use super::role_option;
use crate::shared::ui::{
    Button, ButtonVariant, FormField, PASSWORD_MIN_LENGTH, RegistreTable, SectionHeader,
};
use flux_shared::Role;
use leptos::prelude::*;

#[component]
pub fn SettingsAccounts() -> impl IntoView {
    view! {
        <section class="section-panel" aria-labelledby="accounts-title">
            <SectionHeader
                id="accounts-title"
                title="Comptes"
                subtitle="Qui peut se connecter et ce qu'il peut faire. Un compte se désactive, il ne se supprime pas."
            />
            <div class="registre" id="accounts-registre">
                <RegistreTable
                    aria_label="Comptes"
                    headers=vec!["Identifiant", "Rôle", "État", "Action"]
                    body_id="accounts-table-body"
                    empty="Chargement des comptes…"
                />
            </div>
            <form id="account-create-form" class="settings-form">
                <FormField id="account-create-username" label="Identifiant" required=true autocomplete="off" help="3 à 40 caractères : lettres, chiffres, point, tiret et tiret bas." />
                <FormField id="account-create-password" label="Mot de passe" input_type="password" required=true minlength=PASSWORD_MIN_LENGTH autocomplete="new-password" help="Au moins 8 caractères." />
                <div class="form-group">
                    <label for="account-create-role" class="form-label">"Rôle"</label>
                    <select id="account-create-role" class="form-select">
                        {role_option(Role::Viewer, "Lecteur (consultation seule)")}
                        {role_option(Role::Admin, "Administrateur")}
                    </select>
                </div>
                <Button id="account-create-btn" variant=ButtonVariant::Primary submit=true>
                    "Créer le compte"
                </Button>
            </form>
        </section>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn render() -> String {
        view! { <SettingsAccounts /> }.to_html()
    }

    #[test]
    fn test_section_has_every_id_used_by_the_client() {
        let html = render();
        for id in [
            "accounts-table-body",
            "account-create-form",
            "account-create-username",
            "account-create-password",
            "account-create-role",
            "account-create-btn",
        ] {
            assert!(html.contains(&format!("id=\"{id}\"")), "{id}");
        }
        assert!(!html.contains("style="));
    }

    #[test]
    fn test_creation_offers_only_human_roles() {
        let html = render();
        for role in Role::ACCOUNT_ROLES {
            assert!(html.contains(&format!("value=\"{}\"", role.as_str())));
        }
        assert!(!html.contains(&format!("value=\"{}\"", Role::Worker.as_str())));
    }
}
