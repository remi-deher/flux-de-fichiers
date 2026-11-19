//! Onglet Dépôts de la page Plugins : cartes de sources officielles/tierces, synchronisation et ajout.

use super::metadata::{StoreCard, StoreTone};
use super::repo_detail::{RepoDetail, RepoDrawerTab};
use super::{REPOSITORIES_PATH, SelectedItem, bump};
use crate::api;
use crate::route::encode_segment;
use crate::ui::search::{matches, query};
use crate::ui::topbar_action::{ActionSpec, provide_topbar_action};
use crate::ui::{
    Drawer, EmptyState, IconName, MetricTone, StatItem, StatStrip, Tag, TagTone, Toasts,
};
use forgearr_shared::plugins::{CreateRepositoryRequest, PluginRepository};
use leptos::prelude::*;
use serde_json::Value;

/// Opération sur un dépôt : bascule, synchronisation ou suppression.
#[derive(Clone, Copy)]
pub(super) enum RepoAction {
    Toggle,
    Sync,
    Delete,
}

async fn run_repo_action(id: String, action: RepoAction) -> Result<(), api::ApiError> {
    let base = format!("{REPOSITORIES_PATH}/{}", encode_segment(&id));
    match action {
        RepoAction::Toggle => api::put::<(), Value>(&format!("{base}/toggle"), &())
            .await
            .map(|_| ()),
        RepoAction::Sync => api::post::<(), Value>(&format!("{base}/sync"), None)
            .await
            .map(|_| ()),
        RepoAction::Delete => api::delete(&base).await,
    }
}

/// Filtre 1-clic des dépôts via leur `StatStrip`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RepoFilter {
    #[default]
    All,
    Active,
    Inactive,
}

impl RepoFilter {
    pub fn accepts(self, enabled: bool) -> bool {
        match self {
            Self::All => true,
            Self::Active => enabled,
            Self::Inactive => !enabled,
        }
    }
}

/// Compteurs affichés dans le bandeau `StatStrip` de l'onglet Dépôts.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RepoCounts {
    pub total: usize,
    pub active: usize,
    pub inactive: usize,
}

pub fn summarize_repos(repos: &[PluginRepository]) -> RepoCounts {
    let active = repos.iter().filter(|repo| repo.enabled).count();
    let total = repos.len();
    RepoCounts {
        total,
        active,
        inactive: total.saturating_sub(active),
    }
}

pub fn filter_repos(
    repos: Vec<PluginRepository>,
    filter: RepoFilter,
    text: &str,
) -> Vec<PluginRepository> {
    repos
        .into_iter()
        .filter(|repo| {
            filter.accepts(repo.enabled) && matches(&[&repo.name, &repo.index_url], text)
        })
        .collect()
}

pub fn format_sync_label(last_synced_at: Option<&str>) -> String {
    match last_synced_at {
        Some(raw) if !raw.trim().is_empty() => {
            format!("Synchro : {}", raw.split('T').next().unwrap_or(raw))
        }
        _ => "Jamais synchronisé".to_owned(),
    }
}

#[component]
pub fn ReposSection(
    repositories: LocalResource<Result<Vec<PluginRepository>, api::ApiError>>,
    reload_repos: RwSignal<u32>,
    reload_catalog: RwSignal<u32>,
    adding: RwSignal<bool>,
) -> impl IntoView {
    provide_topbar_action(ActionSpec {
        label: "Ajouter un dépôt",
        on_click: Callback::new(move |()| adding.set(true)),
    });
    let active_filter = RwSignal::new(RepoFilter::All);
    let selected_repo = RwSignal::new(None::<SelectedItem>);
    let detail_open = RwSignal::new(false);
    let detail_tab = RwSignal::new(0_usize);
    let reload_detail = RwSignal::new(0_u32);
    let detail_title = Signal::derive(move || {
        selected_repo
            .get()
            .map(|item| item.name)
            .unwrap_or_default()
    });
    let feedback = RwSignal::new(None::<String>);
    let act = Action::new_local(move |(id, action): &(String, RepoAction)| {
        run_repo_action(id.clone(), *action)
    });
    let sync_all = Action::new_local(move |ids: &Vec<String>| {
        let ids = ids.clone();
        async move {
            for id in ids {
                run_repo_action(id, RepoAction::Sync).await?;
            }
            Ok::<(), api::ApiError>(())
        }
    });
    Effect::new(move |_| match act.value().get() {
        Some(Ok(())) => {
            feedback.set(None);
            Toasts::notify_success("Opération sur le dépôt effectuée.");
            bump(reload_repos);
            bump(reload_catalog);
            bump(reload_detail);
        }
        Some(Err(error)) => Toasts::set_error(feedback, error.message()),
        None => {}
    });
    Effect::new(move |_| match sync_all.value().get() {
        Some(Ok(())) => {
            feedback.set(None);
            Toasts::notify_success("Tous les dépôts actifs ont été synchronisés.");
            bump(reload_repos);
            bump(reload_catalog);
            bump(reload_detail);
        }
        Some(Err(error)) => Toasts::set_error(feedback, error.message()),
        None => {}
    });
    let on_repo_action = Callback::new(move |(id, action): (String, RepoAction)| {
        if matches!(action, RepoAction::Delete) {
            detail_open.set(false);
        }
        act.dispatch((id, action));
    });
    let action_pending = Signal::derive(move || act.pending().get() || sync_all.pending().get());
    let (name, url) = (RwSignal::new(String::new()), RwSignal::new(String::new()));
    let add = Action::new_local(move |request: &CreateRepositoryRequest| {
        let request = request.clone();
        async move {
            api::post::<CreateRepositoryRequest, PluginRepository>(
                REPOSITORIES_PATH,
                Some(&request),
            )
            .await
        }
    });
    Effect::new(move |_| match add.value().get() {
        Some(Ok(_)) => {
            name.set(String::new());
            url.set(String::new());
            adding.set(false);
            Toasts::notify_success("Dépôt ajouté.");
            bump(reload_repos);
        }
        Some(Err(error)) => Toasts::set_error(feedback, error.message()),
        None => {}
    });

    let stats = Signal::derive(move || {
        let list = repositories.get().and_then(Result::ok).unwrap_or_default();
        let counts = summarize_repos(&list);
        let active = active_filter.get();
        let set_filter = move |next: RepoFilter| Callback::new(move |()| active_filter.set(next));
        vec![
            StatItem::new("Tous", counts.total.to_string(), MetricTone::Neutral)
                .with_filter(active == RepoFilter::All, set_filter(RepoFilter::All)),
            StatItem::new(
                "Actifs",
                counts.active.to_string(),
                if counts.active > 0 {
                    MetricTone::Success
                } else {
                    MetricTone::Warning
                },
            )
            .with_filter(active == RepoFilter::Active, set_filter(RepoFilter::Active)),
            StatItem::new(
                "Désactivés",
                counts.inactive.to_string(),
                MetricTone::Neutral,
            )
            .with_filter(
                active == RepoFilter::Inactive,
                set_filter(RepoFilter::Inactive),
            ),
        ]
    });

    view! {
        <div class="store-toolbar">
            <StatStrip aria_label="Synthèse des dépôts" items=stats />
            <div class="store-toolbar__actions">
                <button
                    type="button"
                    class="btn btn-secondary btn-sm"
                    disabled=action_pending
                    on:click=move |_| {
                        let active_ids: Vec<String> = repositories
                            .get_untracked()
                            .and_then(Result::ok)
                            .unwrap_or_default()
                            .into_iter()
                            .filter(|repo| repo.enabled)
                            .map(|repo| repo.id)
                            .collect();
                        if !active_ids.is_empty() {
                            sync_all.dispatch(active_ids);
                        }
                    }
                >
                    {move || if sync_all.pending().get() { "Synchronisation…" } else { "Tout synchroniser" }}
                </button>
            </div>
        </div>
        <Drawer
            open=detail_open
            eyebrow="Dépôt"
            title=detail_title
            tabs=RepoDrawerTab::TABS.to_vec()
            tab=detail_tab
        >
            {move || selected_repo.get().map(|item| {
                view! {
                    <RepoDetail
                        repo_id=item.id
                        tab=detail_tab
                        reload_key=reload_detail
                        on_action=on_repo_action
                        action_pending=action_pending
                    />
                }
            })}
        </Drawer>
        <Drawer open=adding eyebrow="Extensions" title="Ajouter un dépôt".to_owned()>
            <form class="inspection-panel" on:submit=move |event| {
                event.prevent_default();
                add.dispatch(CreateRepositoryRequest {
                    id: None,
                    name: name.get_untracked(),
                    index_url: url.get_untracked(),
                });
            }>
                <div class="form-group">
                    <label for="repo-name" class="form-label">"Nom du dépôt"</label>
                    <input id="repo-name" class="form-input" required=true prop:value=move || name.get()
                        on:input=move |event| name.set(event_target_value(&event)) />
                </div>
                <div class="form-group">
                    <label for="repo-url" class="form-label">"URL de l'index"</label>
                    <input id="repo-url" class="form-input" type="url" required=true placeholder="https://…/index.json"
                        prop:value=move || url.get() on:input=move |event| url.set(event_target_value(&event)) />
                </div>
                <div class="inspection-drawer__actions">
                    <button type="submit" class="btn btn-primary" disabled=move || add.pending().get()>"Ajouter le dépôt"</button>
                </div>
            </form>
        </Drawer>
        <section class="page-section">
            {move || {
                let text = query();
                let filter = active_filter.get();
                match repositories.get() {
                    None => view! {
                        <div class="empty-state-panel">
                            <p class="text-dim">"Chargement des dépôts…"</p>
                        </div>
                    }.into_any(),
                    Some(Err(error)) => view! {
                        <div class="empty-state-panel">
                            <p class="form-error">{error.message()}</p>
                        </div>
                    }.into_any(),
                    Some(Ok(list)) => {
                        let shown = filter_repos(list, filter, &text);
                        if shown.is_empty() {
                            return view! {
                                <div class="empty-state-panel">
                                    <EmptyState
                                        title="Aucun dépôt correspondant"
                                        message="Ajoutez un dépôt ou ajustez le filtre actif."
                                        icon_name=IconName::Layers
                                    />
                                </div>
                            }.into_any();
                        }
                        view! {
                            <div class="repo-list">
                                {shown.into_iter().map(|repo| {
                                    let (inspect_id, inspect_name, active_id, sync_id, toggle_id) = (
                                        repo.id.clone(),
                                        repo.name.clone(),
                                        repo.id.clone(),
                                        repo.id.clone(),
                                        repo.id.clone(),
                                    );
                                    let tone = if repo.enabled { StoreTone::Success } else { StoreTone::Neutral };
                                    let status_badge = view! {
                                        <Tag
                                            label=if repo.enabled { "Actif" } else { "Désactivé" }
                                            tone=if repo.enabled { TagTone::Success } else { TagTone::Neutral }
                                        />
                                    }.into_any();
                                    let context_tags = view! {
                                        <Tag
                                            label=if repo.is_official { "Officiel" } else { "Tiers" }
                                            tone=if repo.is_official { TagTone::Primary } else { TagTone::Neutral }
                                        />
                                    }.into_any();
                                    let sync_footer = format_sync_label(repo.last_synced_at.as_deref());
                                    let enabled = repo.enabled;
                                    let actions = view! {
                                        <button
                                            type="button"
                                            class="btn btn-primary btn-sm"
                                            disabled=action_pending
                                            on:click=move |_| {
                                                act.dispatch((sync_id.clone(), RepoAction::Sync));
                                            }
                                        >
                                            "Synchroniser"
                                        </button>
                                        <button
                                            type="button"
                                            class="btn btn-secondary btn-sm"
                                            disabled=action_pending
                                            on:click=move |_| {
                                                act.dispatch((toggle_id.clone(), RepoAction::Toggle));
                                            }
                                        >
                                            {if enabled { "Désactiver" } else { "Activer" }}
                                        </button>
                                    }.into_any();
                                    let open_detail = Callback::new(move |()| {
                                        selected_repo.set(Some(SelectedItem {
                                            id: inspect_id.clone(),
                                            name: inspect_name.clone(),
                                        }));
                                        detail_tab.set(0);
                                        detail_open.set(true);
                                    });
                                    view! {
                                        <StoreCard
                                            title=repo.name
                                            description=repo.index_url
                                            icon=IconName::Layers
                                            tone=tone
                                            status_badge=Some(status_badge)
                                            context_tags=Some(context_tags)
                                            footer_left=sync_footer
                                            footer_actions=Some(actions)
                                            active=Some(Signal::derive(move || {
                                                detail_open.get()
                                                    && selected_repo
                                                        .get()
                                                        .is_some_and(|item| item.id == active_id)
                                            }))
                                            on_open=open_detail
                                        />
                                    }
                                }).collect_view()}
                            </div>
                        }.into_any()
                    }
                }
            }}
            {move || feedback.get().map(|msg| view! {
                <p class="form-help" role="status">{msg}</p>
            })}
        </section>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo(id: &str, enabled: bool) -> PluginRepository {
        PluginRepository {
            id: id.to_owned(),
            name: id.to_owned(),
            index_url: format!("https://example.test/{id}.json"),
            enabled,
            is_official: false,
            last_synced_at: None,
            created_at: String::new(),
        }
    }

    #[test]
    fn summarize_and_filter_repos_partition_enabled_state() {
        let items = vec![repo("officiel", true), repo("communaute", false)];
        assert_eq!(
            summarize_repos(&items),
            RepoCounts {
                total: 2,
                active: 1,
                inactive: 1,
            }
        );
        assert_eq!(filter_repos(items.clone(), RepoFilter::All, "").len(), 2);
        assert_eq!(filter_repos(items.clone(), RepoFilter::Active, "").len(), 1);
        assert_eq!(filter_repos(items, RepoFilter::Inactive, "commu").len(), 1);
    }

    #[test]
    fn format_sync_label_handles_missing_and_iso_dates() {
        assert_eq!(format_sync_label(None), "Jamais synchronisé");
        assert_eq!(
            format_sync_label(Some("2026-11-19T12:00:00Z")),
            "Synchro : 2026-11-19"
        );
    }
}
