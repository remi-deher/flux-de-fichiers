//! Outils distribués de l'onglet Installés : état du binaire et téléchargement (ADR 0029 §16).

use super::catalog::{InstallFlags, ReadinessFilter};
use super::metadata::{StoreCard, tool_status_tag};
use super::{SelectedItem, TOOLS_PATH, install_with_dependencies_action};
use crate::ui::search::matches;
use crate::ui::{IconName, Tag, TagTone};
use forgearr_shared::plugins::{CatalogReadiness, ToolStatus, ToolStatusSummary};
use leptos::prelude::*;
use std::collections::HashMap;

/// État d'un outil vu par les filtres de l'onglet : un binaire absent demande une action.
pub const fn tool_readiness(status: ToolStatus) -> CatalogReadiness {
    match status {
        ToolStatus::Ready => CatalogReadiness::Ready,
        ToolStatus::Installing => CatalogReadiness::Available,
        ToolStatus::NotInstalled => CatalogReadiness::NeedsTools,
    }
}

pub fn filter_tools(
    tools: Vec<ToolStatusSummary>,
    readiness: ReadinessFilter,
    flags: &HashMap<String, InstallFlags>,
    text: &str,
) -> Vec<ToolStatusSummary> {
    tools
        .into_iter()
        .filter(|tool| {
            let tool_flags = flags.get(&tool.id).copied().unwrap_or_default();
            readiness.accepts(tool_readiness(tool.status), tool_flags)
                && matches(
                    &[
                        &tool.name,
                        &tool.id,
                        &tool.description,
                        tool.capability.as_str(),
                    ],
                    text,
                )
        })
        .collect()
}

/// Grille des outils, avec le téléchargement d'un binaire absent.
#[component]
pub fn ToolGrid(
    tools: Vec<ToolStatusSummary>,
    can_install: bool,
    active_tool_id: Signal<Option<String>>,
    on_open_tool: Callback<SelectedItem>,
) -> impl IntoView {
    let installing_id = RwSignal::new(None::<String>);
    let feedback = RwSignal::new(None::<String>);
    let install = install_with_dependencies_action(TOOLS_PATH, feedback);
    Effect::new(move |_| {
        if !install.pending().get() {
            installing_id.set(None);
        }
    });
    view! {
        <div class="store-grid">
            {tools.into_iter().map(|tool| {
                let (status_label, status_tone, card_tone) = tool_status_tag(tool.status);
                let id = tool.id.clone();
                let open_id = id.clone();
                let open_name = tool.name.clone();
                let active_id = id.clone();
                let installable = can_install
                    && tool.installable
                    && tool.status == ToolStatus::NotInstalled;
                let pending_contract = !tool.installable && tool.status == ToolStatus::NotInstalled;
                let status_badge = view! { <Tag label=status_label tone=status_tone /> }.into_any();
                let context_tags = view! {
                    <Tag label=tool.capability.as_str() tone=TagTone::Primary />
                    {tool.current_choice.then(|| view! {
                        <Tag label="Par défaut" tone=TagTone::Success />
                    })}
                    {pending_contract.then(|| view! {
                        <Tag label="Bientôt disponible" tone=TagTone::Warning />
                    })}
                }
                .into_any();
                let open_detail = Callback::new(move |()| {
                    on_open_tool.run(SelectedItem {
                        id: open_id.clone(),
                        name: open_name.clone(),
                    });
                });
                let actions = installable.then(|| {
                    let id_dispatch = id.clone();
                    let id_label = id.clone();
                    view! {
                        <button
                            type="button"
                            class="btn btn-primary btn-sm"
                            disabled=move || install.pending().get()
                            on:click=move |ev| {
                                ev.stop_propagation();
                                installing_id.set(Some(id_dispatch.clone()));
                                install.dispatch(id_dispatch.clone());
                            }
                        >
                            {move || if install.pending().get()
                                && installing_id.get().as_deref() == Some(id_label.as_str())
                            {
                                "Téléchargement…"
                            } else {
                                "Télécharger"
                            }}
                        </button>
                    }
                    .into_any()
                });
                let description = if tool.description.trim().is_empty() {
                    "Outil distribué exécuté sur les workers.".to_owned()
                } else {
                    tool.description
                };
                view! {
                    <StoreCard
                        focus_key=tool.id
                        title=tool.name
                        description=description
                        icon=IconName::Package
                        tone=card_tone
                        status_badge=Some(status_badge)
                        context_tags=Some(context_tags)
                        footer_left=format!("v{}", tool.version)
                        footer_actions=actions
                        active=Some(Signal::derive(move || {
                            active_tool_id.get().as_deref() == Some(active_id.as_str())
                        }))
                        on_open=open_detail
                    />
                }
            }).collect_view()}
        </div>
        {move || feedback.get().map(|msg| view! {
            <p class="form-help" role="status">{msg}</p>
        })}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use forgearr_shared::plugins::vocab::CatalogCapability;

    fn tool(id: &str, status: ToolStatus) -> ToolStatusSummary {
        ToolStatusSummary {
            id: id.to_owned(),
            name: id.to_owned(),
            version: "1.0.0".to_owned(),
            description: "Outil de test".to_owned(),
            capability: CatalogCapability::new("transcode"),
            arch: "x86_64".to_owned(),
            distribution: "apt".to_owned(),
            current_choice: false,
            status,
            installable: true,
            updated_at: None,
        }
    }

    #[test]
    fn tools_follow_the_state_filters_of_the_installed_tab() {
        let items = vec![
            tool("ffmpeg", ToolStatus::Ready),
            tool("mkvmerge", ToolStatus::Installing),
            tool("handbrake", ToolStatus::NotInstalled),
        ];
        let mut flags = HashMap::new();
        flags.insert(
            "ffmpeg".to_owned(),
            InstallFlags {
                update: true,
                damaged: false,
            },
        );
        let ids = |filter, text| {
            filter_tools(items.clone(), filter, &flags, text)
                .into_iter()
                .map(|tool| tool.id)
                .collect::<Vec<_>>()
        };
        assert_eq!(ids(ReadinessFilter::All, "").len(), 3);
        assert_eq!(ids(ReadinessFilter::Ready, ""), ["ffmpeg", "mkvmerge"]);
        assert_eq!(ids(ReadinessFilter::ActionRequired, "hand"), ["handbrake"]);
        assert_eq!(ids(ReadinessFilter::Updates, ""), ["ffmpeg"]);
        assert!(ids(ReadinessFilter::Damaged, "").is_empty());
    }
}
