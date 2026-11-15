//! Paramètres : compte personnel, utilisateurs, clés d'API, système et configuration réseau (ADR 0011, 0014).

mod account;
mod accounts;
mod keys;
mod network;
mod network_diagnostics;
pub mod system;

pub use account::{logout, role_label};
pub use system::duration_label;

use crate::api;
use crate::shell::instance_role_badge;
use crate::ui::search::{matches, query};
use crate::ui::{MetricTone, StatItem, StatStrip};
use account::AccountSection;
use accounts::AccountsSection;
use flux_shared::session::Me;
use flux_shared::system::{KeySource, SystemStatus};
use flux_shared::{InstanceKind, Permission};
use keys::KeysSection;
use leptos::prelude::*;
use network::ConfigurationCard;
use network_diagnostics::DiagnosticsCard;
use system::{SystemSection, is_version_coherent};

const STATUS_PATH: &str = "/api/v1/system/status";

/// Sections visibles selon les droits et la nature de l'installation.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Section {
    Account,
    Users,
    Keys,
    System,
    Network,
}

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
    ]
    .into_iter()
    .filter_map(|(section, visible)| visible.then_some(section))
    .collect()
}

fn section_matches(section: Section, text: &str) -> bool {
    let keywords: &[&str] = match section {
        Section::Account => &[
            "mon compte",
            "compte",
            "identifiant",
            "mot de passe",
            "déconnecter",
            "session",
            "rôle",
        ],
        Section::Users => &["utilisateurs", "comptes", "permissions", "email", "rôle"],
        Section::Keys => &["clés d'api", "api", "jeton", "token", "worker"],
        Section::System => &[
            "système",
            "stockage",
            "version",
            "chiffrement",
            "clé",
            "base de données",
            "volume",
            "disque",
            "mise à jour",
        ],
        Section::Network => &[
            "réseau",
            "configuration",
            "tls",
            "http",
            "proxy",
            "session",
            "diagnostic",
            "courriel",
            "smtp",
        ],
    };
    matches(keywords, text)
}

#[component]
pub fn SettingsPage(me: Me) -> impl IntoView {
    let visible = sections(&me);
    let can_users = visible.contains(&Section::Users);
    let can_keys = visible.contains(&Section::Keys);
    let can_settings = visible.contains(&Section::System);

    let status_reload = RwSignal::new(0u64);
    let status_resource = can_settings.then(|| {
        LocalResource::new(move || {
            let _ = status_reload.get();
            api::get::<SystemStatus>(STATUS_PATH)
        })
    });
    let on_settings_saved =
        Callback::new(move |()| status_reload.update(|n| *n = n.wrapping_add(1)));
    let status_signal = status_resource.map(|res| {
        Signal::derive(move || res.get().map(|result| result.map_err(|err| err.message())))
    });

    let account_name = me.name.clone();
    let account_role = role_label(me.role);
    let node_kind = instance_role_badge(me.instance_kind);
    let stats = Signal::derive(move || {
        let mut items = vec![
            StatItem::new("Compte actif", account_name.clone(), MetricTone::Neutral)
                .with_detail(account_role),
        ];
        if let Some(status) = status_signal {
            match status.get().and_then(Result::ok) {
                Some(st) => {
                    let coherent = is_version_coherent(&st);
                    let key_detail = match st.key_source {
                        KeySource::Local => "Locale",
                        KeySource::External => "Externe",
                    };
                    let tls_detail = if st.network.tls {
                        "TLS direct"
                    } else {
                        "Proxy externe"
                    };
                    items.push(
                        StatItem::new(
                            "Version",
                            format!("v{}", st.version),
                            if coherent {
                                MetricTone::Success
                            } else {
                                MetricTone::Warning
                            },
                        )
                        .with_detail(if coherent {
                            "Cohérente"
                        } else {
                            "Décalée"
                        }),
                    );
                    items.push(
                        StatItem::new("Chiffrement", st.key_id, MetricTone::Neutral)
                            .with_detail(key_detail),
                    );
                    items.push(
                        StatItem::new(
                            "Réseau HTTP",
                            st.network.bind,
                            if st.network.tls {
                                MetricTone::Success
                            } else {
                                MetricTone::Neutral
                            },
                        )
                        .with_detail(tls_detail),
                    );
                }
                None => {
                    items.push(StatItem::new("Système", "Chargement…", MetricTone::Neutral));
                }
            }
        } else {
            items.push(StatItem::new("Nœud", node_kind, MetricTone::Neutral));
        }
        items
    });

    let account_hidden = Signal::derive(move || !section_matches(Section::Account, &query()));
    let system_hidden = Signal::derive(move || !section_matches(Section::System, &query()));
    let network_hidden = Signal::derive(move || !section_matches(Section::Network, &query()));

    view! {
        <div class="tab-panel" role="tabpanel">
            <StatStrip aria_label="Synthèse de la configuration" items=stats />
            <div class="settings-bento-grid">
                <AccountSection me=me hidden=account_hidden />
                {status_signal.map(|status| view! {
                    <SystemSection status=status hidden=system_hidden />
                    <ConfigurationCard status=status hidden=network_hidden on_saved=on_settings_saved />
                    <DiagnosticsCard hidden=network_hidden />
                })}
            </div>
            {can_users.then(|| view! { <AccountsSection /> })}
            {can_keys.then(|| view! { <KeysSection /> })}
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
        assert_eq!(sections(&viewer), vec![Section::Account]);

        let admin = Me::new(
            CallerKind::Account,
            "remi".into(),
            Role::Admin,
            InstanceKind::Central,
            None,
        );
        assert_eq!(sections(&admin).len(), 5);

        let worker = Me::new(
            CallerKind::Account,
            "remi".into(),
            Role::Admin,
            InstanceKind::Worker,
            None,
        );
        assert_eq!(sections(&worker), vec![Section::Account]);
    }

    #[test]
    fn section_search_matches_relevant_keywords() {
        assert!(section_matches(Section::Account, ""));
        assert!(section_matches(Section::Account, "mot de passe"));
        assert!(!section_matches(Section::Network, "mot de passe"));
        assert!(section_matches(Section::Network, "tls"));
        assert!(section_matches(Section::System, "chiffrement"));
    }
}
