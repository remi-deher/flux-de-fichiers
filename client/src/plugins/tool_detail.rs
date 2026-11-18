//! Tiroir d'inspection à onglets d'un outil distribué : distribution, utilisation et informations.

use super::{TOOLS_PATH, bump, tool_status_label};
use crate::api;
use crate::route::encode_segment;
use crate::ui::{Icon, IconName, Tag, TagTone, Toasts};
use flux_shared::plugins::{ToolDetailView, ToolStatus};
use leptos::prelude::*;
use serde_json::Value;

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
fn DistributionTab(
    detail: ToolDetailView,
    can_install: bool,
    reload_tools: RwSignal<u32>,
    reload_detail: RwSignal<u32>,
) -> impl IntoView {
    let tool_id = detail.summary.id.clone();
    let (status_label, status_class) = tool_status_label(detail.summary.status);
    let installable = can_install
        && detail.summary.installable
        && detail.summary.status == ToolStatus::NotInstalled
        && detail.release_status.as_deref() != Some("contract-only");
    let feedback = RwSignal::new(None::<String>);
    let install = Action::new_local(move |(): &()| {
        let path = format!("{TOOLS_PATH}/{}/install", encode_segment(&tool_id));
        async move { api::post::<(), Value>(&path, None).await }
    });
    Effect::new(move |_| match install.value().get() {
        Some(Ok(_)) => {
            Toasts::set_success(feedback, "Outil installé.");
            bump(reload_tools);
            bump(reload_detail);
        }
        Some(Err(error)) => Toasts::set_error(feedback, error.message()),
        None => {}
    });

    view! {
        <div class="detail-grid">
            <div class="detail-term">"Statut"</div>
            <div class="detail-val"><span class=status_class>{status_label}</span></div>
            <div class="detail-term">"Distribution"</div>
            <div class="detail-val">{detail.summary.distribution}</div>
            {detail.tool_version.map(|ver| view! {
                <div class="detail-term">"Version binaire"</div>
                <div class="detail-val text-mono">{ver}</div>
            })}
            {detail.release_status.map(|status| view! {
                <div class="detail-term">"Disponibilité"</div>
                <div class="detail-val text-mono">{status}</div>
            })}
            {detail.distribution_mode.map(|mode| view! {
                <div class="detail-term">"Installation"</div>
                <div class="detail-val text-mono">{mode}</div>
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
        {installable.then(|| view! {
            <div class="inspection-drawer__actions">
                <button
                    type="button"
                    class="btn btn-primary"
                    disabled=move || install.pending().get()
                    on:click=move |_| { install.dispatch(()); }
                >
                    <Icon name=IconName::Download size=INSTALL_ICON_SIZE />
                    <span>{move || if install.pending().get() { "Installation…" } else { "Installer l'outil" }}</span>
                </button>
            </div>
        })}
        {(!installable && detail.summary.status == ToolStatus::NotInstalled).then(|| view! {
            <p class="text-dim text-sm">"Bientôt disponible : aucun paquet téléchargeable pour cet appareil."</p>
        })}
        <p class="form-help" role="status">{move || feedback.get()}</p>
    }
}

#[component]
fn UsageTab(detail: ToolDetailView) -> impl IntoView {
    view! {
        <div class="detail-grid">
            <div class="detail-term">"Capacité"</div>
            <div class="detail-val"><Tag label=detail.summary.capability.as_str().to_owned() tone=TagTone::Primary /></div>
            <div class="detail-term">"Choix actif"</div>
            <div class="detail-val">{if detail.summary.current_choice { "Oui" } else { "Non" }}</div>
            {detail.interface.map(|iface| view! {
                <div class="detail-term">"Interface"</div>
                <div class="detail-val text-mono">{iface}</div>
            })}
            {detail.invocation.map(|inv| view! {
                <div class="detail-term">"Invocation"</div>
                <div class="detail-val text-mono">{inv}</div>
            })}
        </div>
        <div class="pack-components">
            <h3 class="form-label">"Plugins utilisant cet outil"</h3>
            {if detail.used_by_plugins.is_empty() {
                view! { <p class="text-dim text-sm">"Aucun plugin du catalogue ne déclare cet outil en dépendance."</p> }.into_any()
            } else {
                view! {
                    <ul class="step-list">
                        {detail.used_by_plugins.into_iter().map(|plugin_id| view! {
                            <li class="step-item">
                                <div class="step-item__head">
                                    <Tag label="Plugin" tone=TagTone::Success />
                                    <span class="text-mono text-strong">{plugin_id}</span>
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
fn InfoTab(detail: ToolDetailView) -> impl IntoView {
    view! {
        {(!detail.summary.description.is_empty()).then(|| view! {
            <p class="text-sm">{detail.summary.description}</p>
        })}
        <div class="detail-grid">
            <div class="detail-term">"Identifiant"</div>
            <div class="detail-val text-mono">{detail.summary.id}</div>
            <div class="detail-term">"Version catalogue"</div>
            <div class="detail-val text-mono">{detail.summary.version}</div>
            <div class="detail-term">"Architecture hôte"</div>
            <div class="detail-val text-mono">{detail.summary.arch}</div>
            {detail.author.map(|author| view! {
                <div class="detail-term">"Auteur"</div>
                <div class="detail-val">{author}</div>
            })}
            {detail.license.map(|license| view! {
                <div class="detail-term">"Licence"</div>
                <div class="detail-val text-mono">{license}</div>
            })}
            {detail.compatibility.app_min.map(|min| view! {
                <div class="detail-term">"App minimale"</div>
                <div class="detail-val text-mono">{min}</div>
            })}
            {(!detail.compatibility.platforms.is_empty()).then(|| view! {
                <div class="detail-term">"Plateformes"</div>
                <div class="detail-val">
                    <div class="cluster">
                        {detail.compatibility.platforms.into_iter().map(|plat| view! {
                            <Tag label=plat tone=TagTone::Mono />
                        }).collect_view()}
                    </div>
                </div>
            })}
            {(!detail.tags.is_empty()).then(|| view! {
                <div class="detail-term">"Étiquettes"</div>
                <div class="detail-val">
                    <div class="cluster">
                        {detail.tags.into_iter().map(|tag| view! {
                            <Tag label=tag tone=TagTone::Neutral />
                        }).collect_view()}
                    </div>
                </div>
            })}
        </div>
    }
}

#[component]
pub fn ToolDetail(
    tool_id: String,
    tab: RwSignal<usize>,
    can_install: bool,
    reload_tools: RwSignal<u32>,
) -> impl IntoView {
    let reload_detail = RwSignal::new(0_u32);
    let path = format!("{TOOLS_PATH}/{}", encode_segment(&tool_id));
    let detail = LocalResource::new(move || {
        reload_detail.track();
        let path = path.clone();
        async move { api::get::<ToolDetailView>(&path).await }
    });

    view! {
        <div class="inspection-panel">
            {move || {
                let active_tab = ToolDrawerTab::from_index(tab.get());
                match detail.get() {
                    None => view! { <p class="text-dim">"Chargement…"</p> }.into_any(),
                    Some(Err(error)) => view! { <p class="text-danger">{error.message()}</p> }.into_any(),
                    Some(Ok(data)) => match active_tab {
                        ToolDrawerTab::Distribution => view! {
                            <DistributionTab
                                detail=data
                                can_install=can_install
                                reload_tools=reload_tools
                                reload_detail=reload_detail
                            />
                        }
                        .into_any(),
                        ToolDrawerTab::Usage => view! { <UsageTab detail=data /> }.into_any(),
                        ToolDrawerTab::Info => view! { <InfoTab detail=data /> }.into_any(),
                    },
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
