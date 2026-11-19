//! Tiroir d'inspection à onglets d'un dépôt de plugins : contenu, synchronisation et informations.

use super::repos::RepoAction;
use super::{REPOSITORIES_PATH, section_badge_label};
use crate::api;
use crate::route::encode_segment;
use crate::ui::{Tag, TagTone};
use forgearr_shared::plugins::RepositoryDetailView;
use leptos::prelude::*;

/// Onglets du tiroir d'un dépôt de plugins.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RepoDrawerTab {
    Content,
    Sync,
    Info,
}

impl RepoDrawerTab {
    pub const TABS: &'static [&'static str] = &["Contenu", "Synchronisation", "Informations"];

    pub const fn from_index(index: usize) -> Self {
        match index {
            0 => Self::Content,
            1 => Self::Sync,
            _ => Self::Info,
        }
    }
}

#[component]
fn ContentTab(detail: RepositoryDetailView) -> impl IntoView {
    let entries_count = detail.entries_count.to_string();
    let skipped_count = detail.skipped_count.to_string();
    view! {
        <div class="detail-grid">
            <div class="detail-term">"Copie locale"</div>
            <div class="detail-val">
                {if detail.synced_on_disk { "Synchronisée" } else { "Absente" }}
            </div>
            <div class="detail-term">"Entrées indexées"</div>
            <div class="detail-val">{entries_count}</div>
            <div class="detail-term">"Fiches ignorées"</div>
            <div class="detail-val">{skipped_count}</div>
            {detail.release_version.map(|rel| view! {
                <div class="detail-term">"Version release"</div>
                <div class="detail-val text-mono">{rel}</div>
            })}
            {detail.schema_version.map(|schema| view! {
                <div class="detail-term">"Schéma d'index"</div>
                <div class="detail-val text-mono">{schema}</div>
            })}
        </div>
        <div class="pack-components">
            <h3 class="form-label">"Pièces exposées par ce dépôt"</h3>
            {if detail.entries.is_empty() {
                view! { <p class="text-dim text-sm">"Aucune pièce synchronisée pour ce dépôt."</p> }.into_any()
            } else {
                view! {
                    <ul class="step-list">
                        {detail.entries.into_iter().map(|entry| view! {
                            <li class="step-item">
                                <div class="step-item__head">
                                    <Tag label=section_badge_label(entry.entry_type) tone=TagTone::Primary />
                                    <span class="text-strong">{entry.name}</span>
                                    <Tag label=entry.version tone=TagTone::Mono />
                                </div>
                                <div class="step-item__meta">
                                    <Tag label=entry.id tone=TagTone::Mono />
                                    {entry.capability.map(|cap| view! {
                                        <Tag label=cap.as_str().to_owned() tone=TagTone::Success />
                                    })}
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
fn SyncTab(
    detail: RepositoryDetailView,
    on_action: Callback<(String, RepoAction)>,
    action_pending: Signal<bool>,
) -> impl IntoView {
    let repo = detail.repository;
    let id_sync = repo.id.clone();
    let id_toggle = repo.id.clone();
    let id_delete = repo.id.clone();
    let last_sync = repo
        .last_synced_at
        .clone()
        .unwrap_or_else(|| "Jamais".into());

    view! {
        <div class="detail-grid">
            <div class="detail-term">"État"</div>
            <div class="detail-val">
                <span class=if repo.enabled { "status-pill success" } else { "status-pill info" }>
                    {if repo.enabled { "Actif" } else { "Désactivé" }}
                </span>
            </div>
            <div class="detail-term">"URL de l'index"</div>
            <div class="detail-val text-mono">{repo.index_url}</div>
            <div class="detail-term">"Dernière synchro"</div>
            <div class="detail-val">{last_sync}</div>
        </div>
        <div class="inspection-drawer__actions">
            <button
                type="button"
                class="btn btn-primary"
                disabled=action_pending
                on:click=move |_| on_action.run((id_sync.clone(), RepoAction::Sync))
            >
                "Synchroniser maintenant"
            </button>
            <button
                type="button"
                class="btn btn-secondary"
                disabled=action_pending
                on:click=move |_| on_action.run((id_toggle.clone(), RepoAction::Toggle))
            >
                {if repo.enabled { "Désactiver" } else { "Activer" }}
            </button>
            {(!repo.is_official).then(|| view! {
                <button
                    type="button"
                    class="btn btn-danger-quiet"
                    disabled=action_pending
                    on:click=move |_| on_action.run((id_delete.clone(), RepoAction::Delete))
                >
                    "Retirer"
                </button>
            })}
        </div>
    }
}

#[component]
fn InfoTab(detail: RepositoryDetailView) -> impl IntoView {
    let repo = detail.repository;
    view! {
        <div class="detail-grid">
            <div class="detail-term">"Identifiant"</div>
            <div class="detail-val text-mono">{repo.id}</div>
            <div class="detail-term">"Nom"</div>
            <div class="detail-val">{repo.name}</div>
            <div class="detail-term">"Origine"</div>
            <div class="detail-val">
                <Tag
                    label=if repo.is_official { "Officiel" } else { "Tiers" }
                    tone=if repo.is_official { TagTone::Primary } else { TagTone::Neutral }
                />
            </div>
            <div class="detail-term">"Ajouté le"</div>
            <div class="detail-val">{repo.created_at}</div>
        </div>
    }
}

#[component]
pub fn RepoDetail(
    repo_id: String,
    tab: RwSignal<usize>,
    reload_key: RwSignal<u32>,
    on_action: Callback<(String, RepoAction)>,
    action_pending: Signal<bool>,
) -> impl IntoView {
    let path = format!("{REPOSITORIES_PATH}/{}", encode_segment(&repo_id));
    let detail = LocalResource::new(move || {
        reload_key.track();
        let path = path.clone();
        async move { api::get::<RepositoryDetailView>(&path).await }
    });

    view! {
        <div class="inspection-panel">
            {move || {
                let active_tab = RepoDrawerTab::from_index(tab.get());
                match detail.get() {
                    None => view! { <p class="text-dim">"Chargement…"</p> }.into_any(),
                    Some(Err(error)) => view! { <p class="text-danger">{error.message()}</p> }.into_any(),
                    Some(Ok(data)) => match active_tab {
                        RepoDrawerTab::Content => view! { <ContentTab detail=data /> }.into_any(),
                        RepoDrawerTab::Sync => view! {
                            <SyncTab detail=data on_action=on_action action_pending=action_pending />
                        }
                        .into_any(),
                        RepoDrawerTab::Info => view! { <InfoTab detail=data /> }.into_any(),
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
    fn repo_drawer_tabs_map_all_indices() {
        assert_eq!(
            RepoDrawerTab::TABS,
            &["Contenu", "Synchronisation", "Informations"]
        );
        assert_eq!(RepoDrawerTab::from_index(0), RepoDrawerTab::Content);
        assert_eq!(RepoDrawerTab::from_index(1), RepoDrawerTab::Sync);
        assert_eq!(RepoDrawerTab::from_index(2), RepoDrawerTab::Info);
        assert_eq!(RepoDrawerTab::from_index(42), RepoDrawerTab::Info);
    }
}
