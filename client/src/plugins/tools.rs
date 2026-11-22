//! Onglet Outils de la page Plugins : état d'installation et déploiement des binaires (grille App Store).

use super::metadata::{StoreCard, tool_status_tag};
use super::{SelectedItem, TOOLS_PATH, install_with_dependencies_action};
use crate::live::Loaded;
use crate::ui::search::{matches, query};
use crate::ui::{EmptyState, IconName, MetricTone, StatItem, StatStrip, Tag, TagTone};
use forgearr_shared::plugins::{ToolStatus, ToolStatusSummary};
use leptos::prelude::*;

/// Filtre 1-clic des outils via leur `StatStrip`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ToolStateFilter {
    #[default]
    All,
    Ready,
    Installing,
    NotInstalled,
}

impl ToolStateFilter {
    pub fn accepts(self, status: ToolStatus) -> bool {
        match self {
            Self::All => true,
            Self::Ready => status == ToolStatus::Ready,
            Self::Installing => status == ToolStatus::Installing,
            Self::NotInstalled => status == ToolStatus::NotInstalled,
        }
    }
}

/// Compteurs affichés dans le bandeau `StatStrip` de l'onglet Outils.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ToolCounts {
    pub total: usize,
    pub ready: usize,
    pub installing: usize,
    pub not_installed: usize,
}

pub fn summarize_tools(tools: &[ToolStatusSummary]) -> ToolCounts {
    let mut counts = ToolCounts {
        total: tools.len(),
        ..ToolCounts::default()
    };
    for tool in tools {
        match tool.status {
            ToolStatus::Ready => counts.ready += 1,
            ToolStatus::Installing => counts.installing += 1,
            ToolStatus::NotInstalled => counts.not_installed += 1,
        }
    }
    counts
}

pub fn filter_tools(
    tools: Vec<ToolStatusSummary>,
    filter: ToolStateFilter,
    text: &str,
) -> Vec<ToolStatusSummary> {
    tools
        .into_iter()
        .filter(|tool| {
            filter.accepts(tool.status)
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

#[component]
pub fn ToolsSection(
    tools: Loaded<Vec<ToolStatusSummary>>,
    can_install: bool,
    active_tool_id: Signal<Option<String>>,
    on_open_tool: Callback<SelectedItem>,
) -> impl IntoView {
    let active_filter = RwSignal::new(ToolStateFilter::All);
    let installing_id = RwSignal::new(None::<String>);
    let feedback = RwSignal::new(None::<String>);
    let install = install_with_dependencies_action(TOOLS_PATH, feedback);
    Effect::new(move |_| {
        if !install.pending().get() {
            installing_id.set(None);
        }
    });

    let stats = Signal::derive(move || {
        let list = tools.get().and_then(Result::ok).unwrap_or_default();
        let counts = summarize_tools(&list);
        let active = active_filter.get();
        let set_filter =
            move |next: ToolStateFilter| Callback::new(move |()| active_filter.set(next));
        vec![
            StatItem::new("Tous", counts.total.to_string(), MetricTone::Neutral).with_filter(
                active == ToolStateFilter::All,
                set_filter(ToolStateFilter::All),
            ),
            StatItem::new(
                "Prêts",
                counts.ready.to_string(),
                if counts.ready > 0 {
                    MetricTone::Success
                } else {
                    MetricTone::Neutral
                },
            )
            .with_filter(
                active == ToolStateFilter::Ready,
                set_filter(ToolStateFilter::Ready),
            ),
            StatItem::new(
                "En installation",
                counts.installing.to_string(),
                if counts.installing > 0 {
                    MetricTone::Warning
                } else {
                    MetricTone::Neutral
                },
            )
            .with_filter(
                active == ToolStateFilter::Installing,
                set_filter(ToolStateFilter::Installing),
            ),
            StatItem::new(
                "Non installés",
                counts.not_installed.to_string(),
                MetricTone::Neutral,
            )
            .with_filter(
                active == ToolStateFilter::NotInstalled,
                set_filter(ToolStateFilter::NotInstalled),
            ),
        ]
    });

    view! {
        <div class="store-toolbar">
            <StatStrip aria_label="Synthèse des outils" items=stats />
        </div>
        {move || {
            let text = query();
            let filter = active_filter.get();
            match tools.get() {
                None => view! {
                    <div class="empty-state-panel">
                        <p class="text-dim">"Chargement des outils…"</p>
                    </div>
                }
                .into_any(),
                Some(Err(error)) => view! {
                    <div class="empty-state-panel">
                        <p class="form-error">{error.message()}</p>
                    </div>
                }
                .into_any(),
                Some(Ok(list)) => {
                    let shown = filter_tools(list, filter, &text);
                    if shown.is_empty() {
                        let unfiltered = text.trim().is_empty() && filter == ToolStateFilter::All;
                        return view! {
                            <div class="empty-state-panel">
                                <EmptyState
                                    title=if unfiltered {
                                        "Aucun outil distribué"
                                    } else {
                                        "Aucun outil correspondant"
                                    }
                                    message=if unfiltered {
                                        "Installez un pack depuis le catalogue pour provisionner des outils."
                                    } else {
                                        "Ajustez votre recherche ou le filtre d'état."
                                    }
                                    icon_name=IconName::Package
                                />
                            </div>
                        }
                        .into_any();
                    }
                    view! {
                        <div class="store-grid">
                            {shown.into_iter().map(|tool| {
                                let (status_label, status_tone, card_tone) = tool_status_tag(tool.status);
                                let id = tool.id.clone();
                                let open_id = id.clone();
                                let open_name = tool.name.clone();
                                let active_id = id.clone();
                                let installable = can_install
                                    && tool.installable
                                    && tool.status == ToolStatus::NotInstalled;
                                let pending_contract = !tool.installable
                                    && tool.status == ToolStatus::NotInstalled;
                                let status_badge = view! {
                                    <Tag label=status_label tone=status_tone />
                                }.into_any();
                                let context_tags = view! {
                                    <Tag label=tool.capability.as_str() tone=TagTone::Primary />
                                    {tool.current_choice.then(|| view! {
                                        <Tag label="Par défaut" tone=TagTone::Success />
                                    })}
                                    {pending_contract.then(|| view! {
                                        <Tag label="Bientôt disponible" tone=TagTone::Warning />
                                    })}
                                }.into_any();
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
                                            on:click=move |_| {
                                                installing_id.set(Some(id_dispatch.clone()));
                                                install.dispatch(id_dispatch.clone());
                                            }
                                        >
                                            {move || if install.pending().get() && installing_id.get().as_deref() == Some(id_label.as_str()) {
                                                "Installation…"
                                            } else {
                                                "Installer"
                                            }}
                                        </button>
                                    }
                                    .into_any()
                                });
                                let description = if tool.description.trim().is_empty() {
                                    "Outil distribué exécuté sur les nœuds.".to_owned()
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
                    .into_any()
                }
            }
        }}
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
    fn summarize_and_filter_tools_partition_statuses() {
        let items = vec![
            tool("ffmpeg", ToolStatus::Ready),
            tool("mkvmerge", ToolStatus::Installing),
            tool("handbrake", ToolStatus::NotInstalled),
        ];
        assert_eq!(
            summarize_tools(&items),
            ToolCounts {
                total: 3,
                ready: 1,
                installing: 1,
                not_installed: 1,
            }
        );
        assert_eq!(
            filter_tools(items.clone(), ToolStateFilter::All, "").len(),
            3
        );
        assert_eq!(
            filter_tools(items.clone(), ToolStateFilter::Ready, "").len(),
            1
        );
        assert_eq!(
            filter_tools(items.clone(), ToolStateFilter::Installing, "").len(),
            1
        );
        assert_eq!(
            filter_tools(items, ToolStateFilter::NotInstalled, "hand").len(),
            1
        );
    }
}
