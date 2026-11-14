//! Paramètres : mon compte, thème, comptes, clés d'API et système, selon les droits (ADR 0011, 0014).

mod account;
mod accounts;
mod keys;
mod network;
mod storage;
mod system;

pub use account::logout;

use crate::ui::{SubNav, SubTab};
use account::AccountSection;
use accounts::AccountsSection;
use flux_shared::session::Me;
use flux_shared::{InstanceKind, Permission};
use keys::KeysSection;
use leptos::prelude::*;
use network::NetworkSection;
use storage::StorageSection;
use system::SystemSection;

/// Section de la page Paramètres et condition d'accès.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Section {
    Account,
    Users,
    Keys,
    System,
    Network,
    Storage,
}

impl Section {
    fn tab(self) -> SubTab {
        match self {
            Self::Account => SubTab::new("Mon compte", "👤"),
            Self::Users => SubTab::new("Utilisateurs", "👥"),
            Self::Keys => SubTab::new("Clés d'API", "🔑"),
            Self::System => SubTab::new("Système", "⚙️"),
            Self::Network => SubTab::new("Réseau", "🌐"),
            Self::Storage => SubTab::new("Stockage", "💾"),
        }
    }
}

/// Sections visibles pour ces droits, dans l'ordre de la sous-navigation.
fn sections(me: &Me) -> Vec<Section> {
    let central = me.instance_kind == InstanceKind::Central;
    [
        (Section::Account, true),
        (
            Section::Users,
            central && me.allows(Permission::ManageAccounts),
        ),
        (
            Section::Keys,
            central && me.allows(Permission::ManageApiKeys),
        ),
        (
            Section::System,
            central && me.allows(Permission::ManageSettings),
        ),
        (
            Section::Network,
            central && me.allows(Permission::ManageSettings),
        ),
        (
            Section::Storage,
            central && me.allows(Permission::ManageSettings),
        ),
    ]
    .into_iter()
    .filter_map(|(section, visible)| visible.then_some(section))
    .collect()
}

#[component]
pub fn SettingsPage(me: Me) -> impl IntoView {
    let visible = sections(&me);
    let tab = RwSignal::new(0_usize);
    let tabs = visible.iter().map(|section| section.tab()).collect();
    let me = StoredValue::new(me);
    view! {
        <div class="tab-panel" role="tabpanel">
            <SubNav aria_label="Sections des paramètres" tabs=tabs selected=tab />
            {move || match visible.get(tab.get()).copied().unwrap_or(Section::Account) {
                Section::Account => view! { <AccountSection me=me.get_value() /> }.into_any(),
                Section::Users => view! { <AccountsSection /> }.into_any(),
                Section::Keys => view! { <KeysSection /> }.into_any(),
                Section::System => view! { <SystemSection /> }.into_any(),
                Section::Network => view! { <NetworkSection /> }.into_any(),
                Section::Storage => view! { <StorageSection /> }.into_any(),
            }}
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use flux_shared::Role;
    use flux_shared::session::CallerKind;

    #[test]
    fn a_viewer_only_sees_its_account_and_an_admin_sees_everything() {
        let viewer = Me::new(
            CallerKind::Account,
            "lecteur".into(),
            Role::Viewer,
            InstanceKind::Central,
            None,
        );
        assert!(sections(&viewer) == vec![Section::Account]);
        let admin = Me::new(
            CallerKind::Account,
            "remi".into(),
            Role::Admin,
            InstanceKind::Central,
            None,
        );
        assert_eq!(sections(&admin).len(), 6);
        let worker = Me::new(
            CallerKind::Account,
            "remi".into(),
            Role::Admin,
            InstanceKind::Worker,
            None,
        );
        assert!(sections(&worker) == vec![Section::Account]);
    }
}
