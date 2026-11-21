//! Tiroir d'inspection à onglets d'un dépôt de plugins : contenu, synchronisation et informations.

use super::metadata::{MetaRow, StatusBanner, StoreTone};
use super::repos::RepoAction;
use super::script_preview::{ScriptPreview, script_preview_path};
use super::{REPOSITORIES_PATH, section_badge_label};
use crate::api;
use crate::route::encode_segment;
use crate::ui::{Tag, TagTone};
use forgearr_shared::plugins::RepositoryDetailView;
use forgearr_shared::plugins::vocab::CatalogItemType;
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

/// Onglet Contenu : liste des pièces, ou aperçu du code du script sélectionné.
#[component]
fn ContentTab(detail: RepositoryDetailView, previewed: RwSignal<Option<String>>) -> impl IntoView {
    let repo_id = detail.repository.id.clone();
    move || match previewed.get() {
        Some(entry_id) => view! {
            <ScriptPreview
                path=script_preview_path(&repo_id, &entry_id)
                on_back=Some(Callback::new(move |()| previewed.set(None)))
            />
        }
        .into_any(),
        None => view! { <ContentList detail=detail.clone() previewed=previewed /> }.into_any(),
    }
}

#[component]
fn ContentList(detail: RepositoryDetailView, previewed: RwSignal<Option<String>>) -> impl IntoView {
    let entries_count = detail.entries_count.to_string();
    let skipped_count = detail.skipped_count.to_string();
    view! {
        <div class="detail-grid">
            <MetaRow term="Copie locale">
                {if detail.synced_on_disk { "Synchronisée" } else { "Absente" }}
            </MetaRow>
            <MetaRow term="Entrées indexées">{entries_count}</MetaRow>
            <MetaRow term="Fiches ignorées">{skipped_count}</MetaRow>
            {detail.release_version.map(|rel| view! {
                <MetaRow term="Version release" mono=true>{rel}</MetaRow>
            })}
            {detail.schema_version.map(|schema| view! {
                <MetaRow term="Schéma d'index" mono=true>{schema}</MetaRow>
            })}
        </div>
        {(!detail.warnings.is_empty()).then(|| view! {
            <div class="pack-components">
                <h3 class="form-label">"Sous-catégories non retenues"</h3>
                <ul class="step-list">
                    {detail.warnings.into_iter().map(|warning| view! {
                        <li class="step-item">
                            <div class="step-item__meta">{warning}</div>
                        </li>
                    }).collect_view()}
                </ul>
            </div>
        })}
        <div class="pack-components">
            <h3 class="form-label">"Pièces exposées par ce dépôt"</h3>
            {if detail.entries.is_empty() {
                view! { <p class="text-dim text-sm">"Aucune pièce synchronisée pour ce dépôt."</p> }.into_any()
            } else {
                view! {
                    <ul class="step-list">
                        {detail.entries.into_iter().map(|entry| {
                            let preview_id = (entry.entry_type == CatalogItemType::Script)
                                .then(|| entry.id.clone());
                            view! {
                            <li class="step-item">
                                <div class="step-item__head">
                                    <Tag label=section_badge_label(entry.entry_type) tone=TagTone::Primary />
                                    <span class="text-strong">{entry.name}</span>
                                    <Tag label=entry.version tone=TagTone::Mono />
                                    {preview_id.map(|id| view! {
                                        <button
                                            type="button"
                                            class="btn btn-secondary btn-sm"
                                            on:click=move |_| previewed.set(Some(id.clone()))
                                        >
                                            "Voir le code"
                                        </button>
                                    })}
                                </div>
                                <div class="step-item__meta">
                                    <Tag label=entry.id tone=TagTone::Mono />
                                    {entry.capability.map(|cap| view! {
                                        <Tag label=cap.as_str().to_owned() tone=TagTone::Success />
                                    })}
                                </div>
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
            <MetaRow term="État">
                <Tag
                    label=if repo.enabled { "Actif" } else { "Désactivé" }
                    tone=if repo.enabled { TagTone::Success } else { TagTone::Neutral }
                />
            </MetaRow>
            <MetaRow term="URL de l'index" mono=true>{repo.index_url}</MetaRow>
            <MetaRow term="Dernière synchro">{last_sync}</MetaRow>
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
            <MetaRow term="Identifiant" mono=true>{repo.id}</MetaRow>
            <MetaRow term="Nom">{repo.name}</MetaRow>
            <MetaRow term="Origine">
                <Tag
                    label=if repo.is_official { "Officiel" } else { "Tiers" }
                    tone=if repo.is_official { TagTone::Primary } else { TagTone::Neutral }
                />
            </MetaRow>
            <MetaRow term="Ajouté le">{repo.created_at}</MetaRow>
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
    let previewed = RwSignal::new(None::<String>);
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
                    Some(Ok(data)) => {
                        let sync_id = data.repository.id.clone();
                        let banner_tone = if !data.repository.enabled {
                            StoreTone::Neutral
                        } else if data.skipped_count > 0 || !data.warnings.is_empty() {
                            StoreTone::Warning
                        } else if data.synced_on_disk {
                            StoreTone::Success
                        } else {
                            StoreTone::Primary
                        };
                        let banner_title = if !data.repository.enabled {
                            "Dépôt désactivé".to_owned()
                        } else if !data.synced_on_disk {
                            "Synchronisation requise".to_owned()
                        } else {
                            format!("{} entrée(s) indexée(s)", data.entries_count)
                        };
                        let mut notices = Vec::new();
                        if data.skipped_count > 0 {
                            notices.push(format!("{} fiche(s) ignorée(s) lors du dernier index.", data.skipped_count));
                        }
                        if !data.warnings.is_empty() {
                            notices.push(format!("{} sous-catégorie(s) non retenue(s).", data.warnings.len()));
                        }
                        let banner_text = if notices.is_empty() {
                            data.repository.last_synced_at.as_ref().map(|dt| format!("Dernière synchro : {dt}"))
                        } else {
                            Some(notices.join(" "))
                        };
                        let sync_btn = view! {
                            <button
                                type="button"
                                class="btn btn-primary btn-sm"
                                disabled=action_pending
                                on:click=move |_| on_action.run((sync_id.clone(), RepoAction::Sync))
                            >
                                "Synchroniser"
                            </button>
                        }
                        .into_any();
                        view! {
                            <StatusBanner
                                title=banner_title
                                text=banner_text
                                tone=banner_tone
                                action=Some(sync_btn)
                            />
                            {match active_tab {
                                RepoDrawerTab::Content => view! { <ContentTab detail=data previewed=previewed /> }.into_any(),
                                RepoDrawerTab::Sync => view! {
                                    <SyncTab detail=data on_action=on_action action_pending=action_pending />
                                }
                                .into_any(),
                                RepoDrawerTab::Info => view! { <InfoTab detail=data /> }.into_any(),
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
