//! Onglet Dépôts de la page Plugins : sources officielles/tierces, synchronisation et ajout.

use super::repo_detail::{RepoDetail, RepoDrawerTab};
use super::{REPOSITORIES_PATH, bump};
use crate::api;
use crate::route::encode_segment;
use crate::ui::search::{matches, query};
use crate::ui::{Drawer, MetricTone, StatItem, StatStrip, Toasts};
use flux_shared::plugins::{CreateRepositoryRequest, PluginRepository};
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

#[component]
pub fn ReposSection(
    repositories: LocalResource<Result<Vec<PluginRepository>, api::ApiError>>,
    reload_repos: RwSignal<u32>,
    reload_catalog: RwSignal<u32>,
    adding: RwSignal<bool>,
) -> impl IntoView {
    let active_filter = RwSignal::new(RepoFilter::All);
    let selected_repo = RwSignal::new(None::<String>);
    let detail_open = RwSignal::new(false);
    let detail_tab = RwSignal::new(0_usize);
    let reload_detail = RwSignal::new(0_u32);
    let detail_title = Signal::derive(move || selected_repo.get().unwrap_or_default());
    let feedback = RwSignal::new(None::<String>);
    let act = Action::new_local(move |(id, action): &(String, RepoAction)| {
        run_repo_action(id.clone(), *action)
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
    let on_repo_action = Callback::new(move |(id, action): (String, RepoAction)| {
        if matches!(action, RepoAction::Delete) {
            detail_open.set(false);
        }
        act.dispatch((id, action));
    });
    let action_pending = Signal::derive(move || act.pending().get());
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
        <StatStrip aria_label="Synthèse des dépôts" items=stats />
        <section class="section-panel">
            <div class="registre">
                <div class="table-wrapper">
                    <table class="data-table" aria-label="Dépôts de plugins">
                        <thead><tr>
                            <th scope="col">"Dépôt"</th><th scope="col">"Index"</th><th scope="col">"État"</th>
                            <th scope="col">"Dernière synchronisation"</th><th scope="col" class="cell-end">"Actions"</th>
                        </tr></thead>
                        <tbody>
                            {move || {
                                let text = query();
                                let filter = active_filter.get();
                                match repositories.get() {
                                    None => view! {
                                        <tr><td colspan="5" class="empty-state-text">"Chargement des dépôts…"</td></tr>
                                    }.into_any(),
                                    Some(Err(error)) => view! {
                                        <tr><td colspan="5" class="text-danger">{error.message()}</td></tr>
                                    }.into_any(),
                                    Some(Ok(list)) => {
                                        let shown = filter_repos(list, filter, &text);
                                        if shown.is_empty() {
                                            return view! {
                                                <tr><td colspan="5" class="empty-state-text">"Aucun dépôt correspondant."</td></tr>
                                            }.into_any();
                                        }
                                        shown.into_iter().map(|repo| {
                                            let (inspect_id, toggle_id, sync_id, delete_id) = (
                                                repo.id.clone(),
                                                repo.id.clone(),
                                                repo.id.clone(),
                                                repo.id.clone(),
                                            );
                                            view! {
                                                <tr>
                                                    <td class="text-strong">
                                                        {repo.name.clone()}
                                                        {repo.is_official.then(|| view! {
                                                            " "<span class="repo-badge official">"Officiel"</span>
                                                        })}
                                                    </td>
                                                    <td class="text-mono truncate">{repo.index_url.clone()}</td>
                                                    <td><span class=if repo.enabled { "status-pill success" } else { "status-pill info" }>
                                                        {if repo.enabled { "Actif" } else { "Désactivé" }}</span></td>
                                                    <td class="text-dim">{repo.last_synced_at.clone().unwrap_or_else(|| "Jamais".into())}</td>
                                                    <td class="cell-end inline-actions">
                                                        <button type="button" class="btn btn-secondary btn-sm"
                                                            on:click=move |_| {
                                                                selected_repo.set(Some(inspect_id.clone()));
                                                                detail_tab.set(0);
                                                                detail_open.set(true);
                                                            }>"Inspecter"</button>
                                                        <button type="button" class="btn btn-secondary btn-sm" disabled=move || act.pending().get()
                                                            on:click=move |_| { act.dispatch((sync_id.clone(), RepoAction::Sync)); }>"Synchroniser"</button>
                                                        <button type="button" class="btn btn-secondary btn-sm" disabled=move || act.pending().get()
                                                            on:click=move |_| { act.dispatch((toggle_id.clone(), RepoAction::Toggle)); }>
                                                            {if repo.enabled { "Désactiver" } else { "Activer" }}</button>
                                                        {(!repo.is_official).then(|| view! {
                                                            <button type="button" class="btn btn-sm btn-danger-quiet" disabled=move || act.pending().get()
                                                                on:click=move |_| { act.dispatch((delete_id.clone(), RepoAction::Delete)); }>"Retirer"</button>
                                                        })}
                                                    </td>
                                                </tr>
                                            }
                                        }).collect_view().into_any()
                                    }
                                }
                            }}
                        </tbody>
                    </table>
                </div>
                <Drawer
                    open=detail_open
                    eyebrow="Dépôt"
                    title=detail_title
                    tabs=RepoDrawerTab::TABS.to_vec()
                    tab=detail_tab
                >
                    {move || selected_repo.get().map(|id| {
                        view! {
                            <RepoDetail
                                repo_id=id
                                tab=detail_tab
                                reload_key=reload_detail
                                on_action=on_repo_action
                                action_pending=action_pending
                            />
                        }
                    })}
                </Drawer>
                <Drawer open=adding eyebrow="Plugins" title="Ajouter un dépôt".to_owned()>
                    <form class="inspection-panel" on:submit=move |event| {
                        event.prevent_default();
                        add.dispatch(CreateRepositoryRequest { id: None, name: name.get_untracked(), index_url: url.get_untracked() });
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
                <p class="form-help" role="status">{move || feedback.get()}</p>
            </div>
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
}
