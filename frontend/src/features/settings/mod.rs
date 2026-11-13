//! Module d'administration et de réglages organisé par domaines métier.

pub mod access;
pub mod accounts;
pub mod api_keys;
pub mod connections;
pub mod network;
pub mod storage;

use access::SettingsAccess;
use accounts::SettingsAccounts;
use api_keys::SettingsApiKeys;
use connections::SettingsConnections;
use flux_shared::Role;
use leptos::prelude::*;
use network::SettingsNetwork;
use storage::SettingsStorage;

/// Une option du choix de rôle : la valeur est le nom partagé avec le serveur.
pub(super) fn role_option(role: Role, label: &'static str) -> impl IntoView {
    view! { <option value=role.as_str()>{label}</option> }
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
            <SettingsAccess />
            <SettingsAccounts />
            <SettingsApiKeys />
            <SettingsConnections />
            <SettingsNetwork />
            <SettingsStorage />
        </div>
    }
}
