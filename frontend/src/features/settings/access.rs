//! Domaine Réglages : compte connecté et mot de passe.

use crate::shared::ui::{Button, ButtonVariant, Card, DetailRow, FormField, SectionHeader};
use leptos::prelude::*;

/// Longueur minimale d'un mot de passe, annoncée par le navigateur (le serveur la vérifie aussi).
const PASSWORD_MIN_LENGTH: u32 = 8;

#[component]
pub fn SettingsAccess() -> impl IntoView {
    view! {
        <Card aria_labelledby="account-settings-title">
            <SectionHeader
                id="account-settings-title"
                title="Compte"
                subtitle="Votre session et votre mot de passe."
            />

            <div class="detail-grid">
                <DetailRow label="Connecté en tant que" id="account-name-text" />
                <DetailRow label="Rôle" id="account-role-text" />
                <DetailRow label="Centrale" id="account-instance-text" />
            </div>

            <form id="account-password-form" class="settings-form">
                <FormField
                    id="account-current-password"
                    label="Mot de passe actuel"
                    input_type="password"
                    required=true
                    autocomplete="current-password"
                />
                <FormField
                    id="account-new-password"
                    label="Nouveau mot de passe"
                    input_type="password"
                    required=true
                    minlength=PASSWORD_MIN_LENGTH
                    autocomplete="new-password"
                    help="Au moins 8 caractères. Vos autres sessions sont fermées."
                />
                <div class="inline-actions">
                    <Button id="account-password-submit-btn" variant=ButtonVariant::Primary submit=true>
                        "Changer le mot de passe"
                    </Button>
                    <Button id="account-logout-btn" variant=ButtonVariant::Secondary>
                        "Se déconnecter"
                    </Button>
                </div>
            </form>
        </Card>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_account_section_has_the_ids_used_by_the_client() {
        let html = view! { <SettingsAccess /> }.to_html();
        for id in [
            "account-name-text",
            "account-role-text",
            "account-instance-text",
            "account-password-form",
            "account-current-password",
            "account-new-password",
            "account-logout-btn",
        ] {
            assert!(html.contains(&format!("id=\"{id}\"")), "{id}");
        }
        assert!(!html.contains("admin-key"));
        assert!(!html.contains("style="));
    }
}
