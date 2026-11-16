//! Onglet Outils de la page Plugins : état d'installation et déploiement des binaires.

use super::tool_detail::{ToolDetail, ToolDrawerTab};
use super::{SelectedItem, TOOLS_PATH, bump, tool_status_label};
use crate::api;
use crate::route::encode_segment;
use crate::ui::search::{matches, query};
use crate::ui::{
    Drawer, EmptyState, IconName, MetricTone, ResourceCard, StatItem, StatStrip, Tag, TagTone,
    Toasts,
};
use flux_shared::plugins::{ToolStatus, ToolStatusSummary};
use leptos::prelude::*;
use serde_json::Value;

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
    tools: LocalResource<Result<Vec<ToolStatusSummary>, api::ApiError>>,
    reload_tools: RwSignal<u32>,
    can_install: bool,
) -> impl IntoView {
    let active_filter = RwSignal::new(ToolStateFilter::All);
    let selected_tool = RwSignal::new(None::<SelectedItem>);
    let installing_id = RwSignal::new(None::<String>);
    let drawer_open = RwSignal::new(false);
    let drawer_tab = RwSignal::new(0_usize);
    let drawer_title = Signal::derive(move || {
        selected_tool
            .get()
            .map(|item| item.name)
            .unwrap_or_default()
    });
    let feedback = RwSignal::new(None::<String>);
    let install = Action::new_local(move |id: &String| {
        let id = id.clone();
        installing_id.set(Some(id.clone()));
        let path = format!("{TOOLS_PATH}/{}/install", encode_segment(&id));
        async move { api::post::<(), Value>(&path, None).await }
    });
    Effect::new(move |_| match install.value().get() {
        Some(Ok(_)) => {
            installing_id.set(None);
            Toasts::set_success(feedback, "Outil installé.");
            bump(reload_tools);
        }
        Some(Err(error)) => {
            installing_id.set(None);
            Toasts::set_error(feedback, error.message());
        }
        None => {}
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
        <StatStrip aria_label="Synthèse des outils" items=stats />
        <Drawer
            open=drawer_open
            eyebrow="Outil"
            title=drawer_title
            tabs=ToolDrawerTab::TABS.to_vec()
            tab=drawer_tab
        >
            {move || selected_tool.get().map(|item| {
                view! {
                    <ToolDetail
                        tool_id=item.id
                        tab=drawer_tab
                        can_install=can_install
                        reload_tools=reload_tools
                    />
                }
            })}
        </Drawer>
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
                        <div class="resource-grid resource-grid--list">
                            {shown.into_iter().map(|tool| {
                                let (label, class) = tool_status_label(tool.status);
                                let id = tool.id.clone();
                                let open_id = id.clone();
                                let open_name = tool.name.clone();
                                let active_id = id.clone();
                                let installable = can_install
                                    && tool.installable
                                    && tool.status == ToolStatus::NotInstalled;
                                let pending_contract = !tool.installable
                                    && tool.status == ToolStatus::NotInstalled;
                                let tone = match tool.status {
                                    ToolStatus::Ready => MetricTone::Success,
                                    ToolStatus::Installing => MetricTone::Warning,
                                    ToolStatus::NotInstalled => MetricTone::Neutral,
                                };
                                let badge = view! {
                                    <div class="cluster">
                                        <Tag label=tool.version tone=TagTone::Mono />
                                        <span class=class>{label}</span>
                                    </div>
                                }
                                .into_any();
                                let open_detail = Callback::new(move |()| {
                                    selected_tool.set(Some(SelectedItem {
                                        id: open_id.clone(),
                                        name: open_name.clone(),
                                    }));
                                    drawer_tab.set(0);
                                    drawer_open.set(true);
                                });
                                let actions = view! {
                                    {installable.then(|| {
                                        let id = id.clone();
                                        let id_label = id.clone();
                                        view! {
                                            <button type="button" class="btn btn-secondary btn-sm" disabled=move || install.pending().get()
                                                on:click=move |_| { install.dispatch(id.clone()); }>
                                                {move || if install.pending().get() && installing_id.get().as_deref() == Some(id_label.as_str()) {
                                                    "Installation…"
                                                } else {
                                                    "Installer"
                                                }}
                                            </button>
                                        }
                                    })}
                                    <button type="button" class="btn btn-secondary btn-sm"
                                        on:click=move |_| open_detail.run(())>"Détails"</button>
                                }
                                .into_any();
                                view! {
                                    <ResourceCard
                                        title=tool.name
                                        subtitle=Some(tool.description)
                                        icon=IconName::Package
                                        tone=tone
                                        badge=Some(badge)
                                        active=Some(Signal::derive(move || {
                                            drawer_open.get()
                                                && selected_tool
                                                    .get()
                                                    .is_some_and(|item| item.id == active_id)
                                        }))
                                        on_click=Some(open_detail)
                                        footer_meta=id
                                        footer_actions=Some(actions)
                                    >
                                        <div class="cluster">
                                            <Tag label=tool.capability.as_str() tone=TagTone::Primary />
                                            <Tag label=tool.arch tone=TagTone::Mono />
                                            <Tag label=tool.distribution tone=TagTone::Neutral />
                                            {tool.current_choice.then(|| view! {
                                                <Tag label="Choix actuel" tone=TagTone::Success />
                                            })}
                                            {pending_contract.then(|| view! {
                                                <Tag label="Contrat en attente" tone=TagTone::Warning />
                                            })}
                                        </div>
                                    </ResourceCard>
                                }
                            }).collect_view()}
                        </div>
                        <p class="form-help" role="status">{move || feedback.get()}</p>
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
    use flux_shared::plugins::vocab::CatalogCapability;

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
