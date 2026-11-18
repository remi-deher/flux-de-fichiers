//! Paramètres : compte personnel, utilisateurs, clés d'API, système et configuration réseau (ADR 0011, 0014).

mod account;
mod accounts;
mod keys;
mod mcp_audit;
mod network;
mod network_diagnostics;
pub mod system;

pub use account::{logout, role_label};
pub use system::duration_label;

use crate::api;
use crate::shell::instance_role_badge;
use crate::ui::{MetricTone, StatItem, StatStrip};
use account::AccountSection;
use accounts::AccountsSection;
use flux_shared::session::Me;
use flux_shared::system::{KeySource, SystemStatus};
use flux_shared::{InstanceKind, Permission};
use keys::KeysSection;
use leptos::prelude::*;
use mcp_audit::McpAuditSection;
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
    Network,
    System,
}

impl Section {
    fn label(self) -> &'static str {
        match self {
            Self::Account => "Mon compte",
            Self::Users => "Utilisateurs",
            Self::Keys => "Clés d'API",
            Self::Network => "Réseau & Courriel",
            Self::System => "Système",
        }
    }
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
            Section::Network,
            central && me.allows(Permission::ManageSettings),
        ),
        (
            Section::System,
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
    let show_tabs = visible.len() > 1;
    let can_settings = visible.contains(&Section::System);
    let active_tab = RwSignal::new(visible.first().copied().unwrap_or(Section::Account));

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

    view! {
        <div class="tab-panel" role="tabpanel">
            <StatStrip aria_label="Synthèse de la configuration" items=stats />
            {show_tabs.then(|| view! {
                <nav class="page-subnav" role="tablist" aria-label="Sections des paramètres">
                    {visible.into_iter().map(|tab| {
                        let is_active = move || active_tab.get() == tab;
                        view! {
                            <button
                                type="button"
                                role="tab"
                                class="page-subnav__tab"
                                class:is-active=is_active
                                aria-selected=move || is_active().to_string()
                                on:click=move |_| active_tab.set(tab)
                            >
                                {tab.label()}
                            </button>
                        }
                    }).collect_view()}
                </nav>
            })}
            {move || match active_tab.get() {
                Section::Account => view! { <AccountSection me=me.clone() /> }.into_any(),
                Section::Users => view! { <AccountsSection /> }.into_any(),
                Section::Keys => view! {
                    <div class="stack-md">
                        <KeysSection />
                        <McpAuditSection />
                    </div>
                }.into_any(),
                Section::Network => status_signal.map(|status| view! {
                    <div class="settings-bento-grid">
                        <ConfigurationCard status=status on_saved=on_settings_saved />
                        <DiagnosticsCard />
                    </div>
                }).into_any(),
                Section::System => status_signal.map(|status| view! {
                    <SystemSection status=status />
                }).into_any(),
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
    fn every_section_has_a_readable_label() {
        let labels: Vec<_> = [
            Section::Account,
            Section::Users,
            Section::Keys,
            Section::Network,
            Section::System,
        ]
        .into_iter()
        .map(Section::label)
        .collect();
        assert_eq!(
            labels,
            [
                "Mon compte",
                "Utilisateurs",
                "Clés d'API",
                "Réseau & Courriel",
                "Système",
            ]
        );
    }
}
