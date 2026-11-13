//! Domaine Réglages : utilisateurs. Un registre, et un tiroir par compte pour le modifier.

use super::role_option;
use crate::shared::ui::{
    Button, ButtonVariant, DetailRow, FormField, InspectionPane, PASSWORD_MIN_LENGTH,
    RegistreTable, SectionHeader, inspection_drawer,
};
use flux_shared::Role;
use leptos::prelude::*;

fn account_panes() -> Vec<InspectionPane> {
    vec![InspectionPane {
        suffix: "overview",
        label: "Compte",
        body: view! {
            <div id="account-create-form" hidden>
                <FormField id="account-create-username" label="Identifiant" required=true autocomplete="off" help="3 à 40 caractères : lettres, chiffres, point, tiret et tiret bas." />
                <FormField id="account-create-password" label="Mot de passe" input_type="password" required=true minlength=PASSWORD_MIN_LENGTH autocomplete="new-password" help="Au moins 8 caractères." />
                <div class="form-group">
                    <label for="account-create-role" class="form-label">"Rôle"</label>
                    <select id="account-create-role" class="form-select">
                        {role_option(Role::Viewer, "Lecteur (consultation seule)")}
                        {role_option(Role::Admin, "Administrateur")}
                    </select>
                </div>
            </div>

            <div id="account-detail" hidden>
                <div class="detail-grid">
                    <DetailRow label="Identifiant" id="account-detail-name" />
                    <DetailRow label="Rôle" id="account-detail-role" />
                    <DetailRow label="État" id="account-detail-state" />
                </div>

                <div id="account-admin-controls" class="form-group" hidden>
                    <label for="account-edit-role" class="form-label">"Changer le rôle"</label>
                    <select id="account-edit-role" class="form-select">
                        {role_option(Role::Viewer, "Lecteur (consultation seule)")}
                        {role_option(Role::Admin, "Administrateur")}
                    </select>
                    <p class="form-help">"Les sessions du compte sont fermées à chaque changement."</p>
                </div>

                <form id="account-password-form" class="settings-form" hidden>
                    <p class="form-label">"Changer mon mot de passe"</p>
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
                    <Button id="account-password-submit-btn" variant=ButtonVariant::Primary submit=true>
                        "Changer le mot de passe"
                    </Button>
                </form>
            </div>
        }
        .into_any(),
    }]
}

fn account_actions() -> AnyView {
    view! {
        <div class="inspection-drawer__actions">
            <button type="button" id="account-create-btn" class="btn btn-primary btn-sm inspection-panel__action" hidden disabled>
                "Créer le compte"
            </button>
            <button type="button" id="account-toggle-btn" class="btn btn-danger btn-sm inspection-panel__action" hidden disabled>
                "Désactiver"
            </button>
        </div>
    }
    .into_any()
}

#[component]
pub fn SettingsAccounts() -> impl IntoView {
    let new_account = view! {
        <Button id="account-new-btn" variant=ButtonVariant::Primary class="btn-sm">
            "Nouvel utilisateur"
        </Button>
    }
    .into_any();
    view! {
        <section class="section-panel" aria-labelledby="accounts-title">
            <SectionHeader
                id="accounts-title"
                title="Utilisateurs"
                subtitle="Qui peut se connecter et ce qu'il peut faire. Un compte se désactive, il ne se supprime pas."
                actions=new_account
            />
            <div class="registre" id="accounts-registre">
                <RegistreTable
                    aria_label="Utilisateurs"
                    headers=vec!["Identifiant", "Rôle", "État", "Action"]
                    body_id="accounts-table-body"
                    empty="Chargement des utilisateurs…"
                />
                {inspection_drawer("accounts-fact", account_panes(), Some(account_actions()))}
            </div>
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
    fn test_registre_uses_the_common_table_and_drawer() {
        let html = render();
        for id in ["accounts-table-body", "accounts-fact", "accounts-registre"] {
            assert!(html.contains(&format!("id=\"{id}\"")), "{id}");
        }
        assert!(html.contains("class=\"registre\""));
    }

    #[test]
    fn test_drawer_has_every_id_used_by_the_client() {
        let html = render();
        for id in [
            "account-new-btn",
            "account-create-form",
            "account-create-username",
            "account-create-password",
            "account-create-role",
            "account-create-btn",
            "account-detail",
            "account-detail-name",
            "account-detail-role",
            "account-detail-state",
            "account-admin-controls",
            "account-edit-role",
            "account-toggle-btn",
            "account-password-form",
            "account-current-password",
            "account-new-password",
        ] {
            assert!(html.contains(&format!("id=\"{id}\"")), "{id}");
        }
        assert!(!html.contains("style="));
    }

    #[test]
    fn test_only_human_roles_can_be_chosen() {
        let html = render();
        for role in Role::ACCOUNT_ROLES {
            assert!(html.contains(&format!("value=\"{}\"", role.as_str())));
        }
        assert!(!html.contains(&format!("value=\"{}\"", Role::Worker.as_str())));
    }

    #[test]
    fn test_the_drawer_starts_without_any_account_in_edit_mode() {
        let html = render();
        // Rien n'est modifiable tant qu'aucune ligne n'est ouverte : contrôles et formulaire cachés.
        for hidden_id in [
            "account-create-form",
            "account-detail",
            "account-admin-controls",
            "account-password-form",
        ] {
            let at = html.find(&format!("id=\"{hidden_id}\"")).unwrap_or(0);
            let tag_end = html[at..].find('>').map_or(html.len(), |end| at + end);
            assert!(html[at..tag_end].contains("hidden"), "{hidden_id}");
        }
    }
}
