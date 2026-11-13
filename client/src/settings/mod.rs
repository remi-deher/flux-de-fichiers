//! Paramètres : mon compte, thème, comptes, clés d'API et système, selon les droits (ADR 0011, 0014).

mod account;
mod accounts;
mod keys;
mod system;

pub use account::logout;

use crate::theme::ThemePicker;
use account::AccountSection;
use accounts::AccountsSection;
use flux_shared::session::Me;
use flux_shared::{InstanceKind, Permission};
use keys::KeysSection;
use leptos::prelude::*;
use system::SystemSection;

#[component]
pub fn SettingsPage(me: Me) -> impl IntoView {
    let central = me.instance_kind == InstanceKind::Central;
    let accounts = central && me.allows(Permission::ManageAccounts);
    let keys = central && me.allows(Permission::ManageApiKeys);
    let system = central && me.allows(Permission::ManageSettings);
    view! {
        <div class="registre">
            <h1 class="section-title">"Paramètres"</h1>
            <AccountSection me=me />
            <section class="section-panel" aria-labelledby="theme-title">
                <h2 id="theme-title" class="section-title">"Apparence"</h2>
                <ThemePicker />
            </section>
            {accounts.then(|| view! { <AccountsSection /> })}
            {keys.then(|| view! { <KeysSection /> })}
            {system.then(|| view! { <SystemSection /> })}
        </div>
    }
}
