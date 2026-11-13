//! Module d'administration et de réglages : cinq sections pilotées par une sous-barre.
//!
//! Le compte et le thème vivent dans la barre latérale ; cette page est réservée aux administrateurs.

pub mod accounts;
pub mod api_keys;
pub mod integrations;
pub mod network;
pub mod overview;
pub mod security;
pub mod storage;

use crate::core::{SubNav, SubNavTabItem};
use accounts::SettingsAccounts;
use api_keys::SettingsApiKeys;
use flux_shared::Role;
use integrations::SettingsIntegrations;
use leptos::prelude::*;
use network::SettingsNetwork;
use overview::SettingsOverview;
use security::SettingsEncryption;
use storage::SettingsStorage;

pub const SECTION_OVERVIEW: &str = "settings-section-overview";
pub const SECTION_USERS: &str = "settings-section-users";
pub const SECTION_KEYS: &str = "settings-section-keys";
pub const SECTION_NETWORK: &str = "settings-section-network";
pub const SECTION_STORAGE: &str = "settings-section-storage";

pub const TAB_OVERVIEW: &str = "btn-subtab-settings-overview";
pub const TAB_USERS: &str = "btn-subtab-settings-users";
pub const TAB_KEYS: &str = "btn-subtab-settings-keys";
pub const TAB_NETWORK: &str = "btn-subtab-settings-network";
pub const TAB_STORAGE: &str = "btn-subtab-settings-storage";

/// Une option du choix de rôle : la valeur est le nom partagé avec le serveur.
pub(super) fn role_option(role: Role, label: &'static str) -> impl IntoView {
    view! { <option value=role.as_str()>{label}</option> }
}

/// Une section de la page : seule celle de l'onglet actif est visible.
#[component]
fn SettingsSection(
    id: &'static str,
    #[prop(optional)] active: bool,
    children: Children,
) -> impl IntoView {
    view! {
        <div id=id class="settings-section" role="tabpanel" hidden=!active>
            {children()}
        </div>
    }
}

fn settings_tabs() -> Vec<SubNavTabItem> {
    vec![
        SubNavTabItem::new(TAB_OVERVIEW, "Vue d'ensemble", SECTION_OVERVIEW, true),
        SubNavTabItem::new(TAB_USERS, "Utilisateurs", SECTION_USERS, false),
        SubNavTabItem::new(TAB_KEYS, "Clés et sécurité", SECTION_KEYS, false),
        SubNavTabItem::new(TAB_NETWORK, "Réseau", SECTION_NETWORK, false),
        SubNavTabItem::new(TAB_STORAGE, "Stockage", SECTION_STORAGE, false),
    ]
}

#[component]
pub fn SettingsPanel() -> impl IntoView {
    view! {
        <div
            id=crate::core::panel::SETTINGS.dom_id
            class="tab-panel"
            role="tabpanel"
            aria-labelledby=crate::core::panel::SETTINGS.button_id
            hidden
        >
            <SubNav aria_label="Sections des paramètres" tabs=settings_tabs() />
            <SettingsSection id=SECTION_OVERVIEW active=true>
                <SettingsOverview />
            </SettingsSection>
            <SettingsSection id=SECTION_USERS>
                <SettingsAccounts />
            </SettingsSection>
            <SettingsSection id=SECTION_KEYS>
                <SettingsApiKeys />
                <SettingsEncryption />
            </SettingsSection>
            <SettingsSection id=SECTION_NETWORK>
                <SettingsNetwork />
                <SettingsIntegrations />
            </SettingsSection>
            <SettingsSection id=SECTION_STORAGE>
                <SettingsStorage />
            </SettingsSection>
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn render() -> String {
        view! { <SettingsPanel /> }.to_html()
    }

    #[test]
    fn test_five_tabs_each_control_one_section() {
        let html = render();
        for (tab, section) in [
            (TAB_OVERVIEW, SECTION_OVERVIEW),
            (TAB_USERS, SECTION_USERS),
            (TAB_KEYS, SECTION_KEYS),
            (TAB_NETWORK, SECTION_NETWORK),
            (TAB_STORAGE, SECTION_STORAGE),
        ] {
            assert!(html.contains(&format!("id=\"{tab}\"")), "{tab}");
            assert!(
                html.contains(&format!("aria-controls=\"{section}\"")),
                "{section}"
            );
            assert_eq!(
                html.matches(&format!("id=\"{section}\"")).count(),
                1,
                "{section}"
            );
        }
        assert_eq!(html.matches("class=\"settings-section\"").count(), 5);
    }

    #[test]
    fn test_only_the_overview_is_visible_at_first() {
        let html = render();
        // Quatre sections cachées ; la vue d'ensemble n'a pas l'attribut.
        let hidden_sections = html
            .matches("class=\"settings-section\" role=\"tabpanel\" hidden")
            .count();
        assert_eq!(hidden_sections, 4);
    }

    #[test]
    fn test_keys_and_encryption_share_one_section() {
        let html = render();
        let keys = html.find(SECTION_KEYS).unwrap_or(usize::MAX);
        let network = html.find(SECTION_NETWORK).unwrap_or(0);
        for id in ["api-keys-title", "encryption-title"] {
            let at = html.find(id).unwrap_or(usize::MAX);
            assert!(keys < at && at < network, "{id}");
        }
    }

    #[test]
    fn test_account_and_theme_are_not_on_this_page() {
        let html = render();
        for gone in [
            "account-password-form",
            "account-logout-btn",
            "theme-select",
        ] {
            assert!(!html.contains(gone), "{gone}");
        }
    }
}
