//! Module d'administration et de réglages organisé par domaines métier.

pub mod access;
pub mod connections;
pub mod network;
pub mod storage;

use access::SettingsAccess;
use connections::SettingsConnections;
use leptos::prelude::*;
use network::SettingsNetwork;
use storage::SettingsStorage;

#[component]
pub fn SettingsPanel() -> impl IntoView {
    view! {
        <div
            id="panel-settings"
            class="tab-panel"
            role="tabpanel"
            aria-labelledby="tab-btn-settings"
            hidden
        >
            <SettingsAccess />
            <SettingsConnections />
            <SettingsNetwork />
            <SettingsStorage />
        </div>
    }
}
