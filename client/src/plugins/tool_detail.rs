//! Tiroir d'inspection à onglets d'un outil distribué : distribution, utilisation (liens croisés) et informations.

use super::metadata::{CommonInfoRows, MetaRow, StatusBanner, meta_date_row, tool_status_tag};
use super::{SelectedItem, TOOL_TOPICS, TOOLS_PATH, install_with_dependencies_action};
use crate::api::ApiError;
use crate::live::load;
use crate::route::encode_segment;
use crate::ui::{Icon, IconName, Tag, TagTone};
use forgearr_shared::plugins::{DependencyInstallResult, ToolDetailView, ToolStatus};
use leptos::prelude::*;

const INSTALL_ICON_SIZE: u32 = 15;
const SHA_PREVIEW_LEN: usize = 12;

/// Onglets du tiroir d'un outil distribué.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolDrawerTab {
    Distribution,
    Usage,
    Info,
}

impl ToolDrawerTab {
    pub const TABS: &'static [&'static str] = &["Distribution", "Utilisation", "Informations"];

    pub const fn from_index(index: usize) -> Self {
        match index {
            0 => Self::Distribution,
            1 => Self::Usage,
            _ => Self::Info,
        }
    }
}

pub fn short_sha(sha: &str) -> String {
    if sha.chars().count() <= SHA_PREVIEW_LEN {
        sha.to_owned()
    } else {
        let prefix: String = sha.chars().take(SHA_PREVIEW_LEN).collect();
        format!("{prefix}…")
    }
}

#[component]
fn ToolActionBanner(
    detail: ToolDetailView,
    can_install: bool,
    install: Action<String, Result<DependencyInstallResult, ApiError>>,
    feedback: RwSignal<Option<String>>,
) -> impl IntoView {
    let tool_id = detail.summary.id.clone();
    let (status_label, _, banner_tone) = tool_status_tag(detail.summary.status);
    let installable = can_install
        && detail.summary.installable
        && detail.summary.status == ToolStatus::NotInstalled
        && detail.release_status.as_deref() != Some("contract-only");

    let subtitle = match detail.summary.status {
        ToolStatus::Ready => format!(
            "Disponible sur {} ({}).",
            detail.summary.arch, detail.summary.distribution
        ),
        ToolStatus::Installing => "Téléchargement et extraction en cours…".to_owned(),
        ToolStatus::NotInstalled if installable => {
            "Prêt à être téléchargé dans le cache local.".to_owned()
        }
        ToolStatus::NotInstalled => {
            "Aucun paquet téléchargeable pour cette architecture pour le moment.".to_owned()
        }
    };

    let action = installable.then(|| {
        view! {
            <button
                type="button"
                class="btn btn-primary btn-sm"
                disabled=move || install.pending().get()
                on:click=move |_| { install.dispatch(tool_id.clone()); }
            >
                <Icon name=IconName::Download size=INSTALL_ICON_SIZE />
                <span>{move || if install.pending().get() { "Installation…" } else { "Installer" }}</span>
            </button>
        }
        .into_any()
    });

    view! {
        <StatusBanner
            title=status_label
            text=Some(subtitle)
            tone=banner_tone
            action=action
        />
        {move || feedback.get().map(|msg| view! {
            <p class="form-help" role="status">{msg}</p>
        })}
    }
}

#[component]
fn DistributionTab(detail: ToolDetailView) -> impl IntoView {
    view! {
        <div class="detail-grid">
            <MetaRow term="Distribution">{detail.summary.distribution}</MetaRow>
            {detail.tool_version.map(|ver| view! {
                <MetaRow term="Version binaire" mono=true>{ver}</MetaRow>
            })}
            {detail.release_status.map(|status| view! {
                <MetaRow term="Disponibilité" mono=true>{status}</MetaRow>
            })}
            {meta_date_row("Publication", detail.released_at)}
            {detail.distribution_mode.map(|mode| view! {
                <MetaRow term="Installation" mono=true>{mode}</MetaRow>
            })}
        </div>
        <div class="pack-components">
            <h3 class="form-label">"Paquets disponibles"</h3>
            {if detail.artifacts.is_empty() {
                view! { <p class="text-dim text-sm">"Aucun paquet disponible pour le moment."</p> }.into_any()
            } else {
                view! {
                    <ul class="step-list">
                        {detail.artifacts.into_iter().map(|art| view! {
                            <li class="step-item">
                                <div class="step-item__head">
                                    <span class="text-mono text-strong">{art.arch}</span>
                                    <Tag label=art.archive_format tone=TagTone::Neutral />
                                    {art.host_supported.then(|| view! {
                                        <Tag label="Hôte actuel" tone=TagTone::Success />
                                    })}
                                </div>
                                <div class="step-item__meta">
                                    <Tag label=art.binary_relative_path tone=TagTone::Mono />
                                    <Tag label=format!("sha256:{}", short_sha(&art.archive_sha256)) tone=TagTone::Mono />
                                </div>
                            </li>
                        }).collect_view()}
                    </ul>
                }.into_any()
            }}
        </div>
    }
}

#[component]
fn UsageTab(detail: ToolDetailView, on_open_plugin: Callback<SelectedItem>) -> impl IntoView {
    view! {
        <div class="detail-grid">
            <MetaRow term="Capacité">
                <Tag label=detail.summary.capability.as_str().to_owned() tone=TagTone::Primary />
            </MetaRow>
            <MetaRow term="Choix actif">
                {if detail.summary.current_choice { "Oui" } else { "Non" }}
            </MetaRow>
            {detail.interface.map(|iface| view! {
                <MetaRow term="Interface" mono=true>{iface}</MetaRow>
            })}
            {detail.invocation.map(|inv| view! {
                <MetaRow term="Invocation" mono=true>{inv}</MetaRow>
            })}
        </div>
        <div class="pack-components">
            <h3 class="form-label">"Extensions utilisant cet outil"</h3>
            {if detail.used_by_plugins.is_empty() {
                view! { <p class="text-dim text-sm">"Aucune extension du catalogue ne déclare cet outil en dépendance."</p> }.into_any()
            } else {
                view! {
                    <ul class="step-list">
                        {detail.used_by_plugins.into_iter().map(|plugin_id| {
                            let target_id = plugin_id.clone();
                            let target_name = plugin_id.clone();
                            view! {
                                <li>
                                    <button
                                        type="button"
                                        class="dep-link"
                                        on:click=move |_| on_open_plugin.run(SelectedItem {
                                            id: target_id.clone(),
                                            name: target_name.clone(),
                                        })
                                    >
                                        <span class="dep-link__main">
                                            <Icon name=IconName::Plugins size=15 />
                                            <span class="text-mono text-strong">{plugin_id}</span>
                                        </span>
                                        <span class="dep-link__meta">
                                            <Tag label="Ouvrir l'extension" tone=TagTone::Primary />
                                        </span>
                                    </button>
                                </li>
                            }
                        }).collect_view()}
                    </ul>
                }.into_any()
            }}
        </div>
    }
}

#[component]
fn InfoTab(detail: ToolDetailView) -> impl IntoView {
    view! {
        {(!detail.summary.description.is_empty()).then(|| view! {
            <p class="text-sm">{detail.summary.description}</p>
        })}
        <div class="detail-grid">
            <CommonInfoRows
                id=detail.summary.id
                version=detail.summary.version
                updated_at=detail.summary.updated_at
                author=detail.author
                license=detail.license
                app_min=detail.compatibility.app_min
                platforms=detail.compatibility.platforms
                tags=detail.tags
            />
            <MetaRow term="Architecture hôte" mono=true>{detail.summary.arch}</MetaRow>
        </div>
    }
}

#[component]
pub fn ToolDetail(
    tool_id: String,
    tab: RwSignal<usize>,
    can_install: bool,
    on_open_plugin: Callback<SelectedItem>,
) -> impl IntoView {
    // Hors de la fiche relue : l'installation en cours garde son état et son bilan.
    let feedback = RwSignal::new(None::<String>);
    let install = install_with_dependencies_action(TOOLS_PATH, feedback);
    let path = format!("{TOOLS_PATH}/{}", encode_segment(&tool_id));
    let detail = load::<ToolDetailView>(TOOL_TOPICS, path);

    view! {
        <div class="inspection-panel">
            {move || {
                let active_tab = ToolDrawerTab::from_index(tab.get());
                match detail.get() {
                    None => view! { <p class="text-dim">"Chargement…"</p> }.into_any(),
                    Some(Err(error)) => view! { <p class="text-danger">{error.message()}</p> }.into_any(),
                    Some(Ok(data)) => {
                        let banner_data = data.clone();
                        view! {
                            <ToolActionBanner
                                detail=banner_data
                                can_install=can_install
                                install=install
                                feedback=feedback
                            />
                            {match active_tab {
                                ToolDrawerTab::Distribution => view! {
                                    <DistributionTab detail=data />
                                }
                                .into_any(),
                                ToolDrawerTab::Usage => view! {
                                    <UsageTab detail=data on_open_plugin=on_open_plugin />
                                }.into_any(),
                                ToolDrawerTab::Info => view! { <InfoTab detail=data /> }.into_any(),
                            }}
                        }
                        .into_any()
                    }
                }
            }}
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_drawer_tabs_map_all_indices() {
        assert_eq!(
            ToolDrawerTab::TABS,
            &["Distribution", "Utilisation", "Informations"]
        );
        assert_eq!(ToolDrawerTab::from_index(0), ToolDrawerTab::Distribution);
        assert_eq!(ToolDrawerTab::from_index(1), ToolDrawerTab::Usage);
        assert_eq!(ToolDrawerTab::from_index(2), ToolDrawerTab::Info);
        assert_eq!(ToolDrawerTab::from_index(99), ToolDrawerTab::Info);
    }

    #[test]
    fn short_sha_truncates_long_digests() {
        assert_eq!(short_sha("abcdef"), "abcdef");
        assert_eq!(short_sha("0123456789abcdef0123"), "0123456789ab…");
    }
}
