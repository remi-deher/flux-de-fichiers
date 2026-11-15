//! Écran Bibliothèques : liste, état et édition (nom, flux, chemins par worker) (ADR 0014).

mod draft;
mod form;

use crate::ui::search::{matches, query};
use crate::ui::topbar_action::{ActionSpec, provide_topbar_action};
use crate::ui::{
    Drawer, EmptyState, Icon, IconName, MetricTone, PageHeader, StatItem, StatStrip, StatusBadge,
};
use crate::{api, format};
use draft::{LibraryDraft, access_label, is_ready, library_counts};
use flux_shared::Permission;
use flux_shared::flows::FlowRecord;
use flux_shared::libraries::Library;
use flux_shared::session::Me;
use flux_shared::workers::Worker;
use form::LibraryForm;
use leptos::prelude::*;

const LIBRARIES_PATH: &str = "/api/v1/libraries";
const FLOWS_PATH: &str = "/api/v1/flows";
const WORKERS_PATH: &str = "/api/v1/workers";
const PREVIEW_WORKER_PATHS: usize = 2;

#[component]
pub fn LibrariesPage(me: Me) -> impl IntoView {
    let can_manage = me.allows(Permission::ManageLibraries);
    let reload = RwSignal::new(0_u32);
    let libraries = LocalResource::new(move || {
        reload.track();
        api::get::<Vec<Library>>(LIBRARIES_PATH)
    });
    let flows = LocalResource::new(|| api::get::<Vec<FlowRecord>>(FLOWS_PATH));
    let workers = LocalResource::new(|| api::get::<Vec<Worker>>(WORKERS_PATH));
    let editing = RwSignal::new(None::<LibraryDraft>);
    let drawer = RwSignal::new(false);
    let open_create = Callback::new(move |()| {
        editing.set(Some(LibraryDraft::default()));
        drawer.set(true);
    });
    let on_done = Callback::new(move |()| {
        editing.set(None);
        drawer.set(false);
        reload.update(|count| *count = count.wrapping_add(1));
    });
    let flow_name = move |id: &Option<String>| {
        let Some(id) = id else {
            return "Aucun flux lié".to_owned();
        };
        flows
            .get()
            .and_then(Result::ok)
            .and_then(|list| {
                list.into_iter()
                    .find(|flow| &flow.id == id)
                    .map(|flow| flow.name)
            })
            .unwrap_or_else(|| id.clone())
    };

    Effect::new(move |_| {
        if !drawer.get() {
            editing.set(None);
        }
    });

    let stats = Signal::derive(move || {
        let list = libraries.get().and_then(Result::ok).unwrap_or_default();
        let (total, ready, bound) = library_counts(&list);
        vec![
            StatItem::new("Total", total.to_string(), MetricTone::Neutral),
            StatItem::new("Prêtes", ready.to_string(), MetricTone::Success)
                .with_detail("flux + worker"),
            StatItem::new("Avec flux lié", bound.to_string(), MetricTone::Neutral),
        ]
    });

    let grid = move || {
        let text = query();
        match libraries.get() {
            None => view! {
                <div class="empty-state-panel">
                    <p class="text-dim">"Chargement des bibliothèques…"</p>
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
                let shown: Vec<Library> = list
                    .into_iter()
                    .filter(|library| matches(&[&library.name], &text))
                    .collect();
                if shown.is_empty() {
                    let create_action = (can_manage && text.trim().is_empty())
                        .then(|| {
                            view! {
                                <button type="button" class="btn btn-primary" on:click=move |_| open_create.run(())>
                                    <Icon name=IconName::Plus size=15 />
                                    <span>"Nouvelle bibliothèque"</span>
                                </button>
                            }
                        })
                        .into_any();
                    return view! {
                        <div class="empty-state-panel">
                            <EmptyState
                                title=if text.trim().is_empty() {
                                    "Aucune bibliothèque déclarée"
                                } else {
                                    "Aucune bibliothèque correspondante"
                                }
                                message=if text.trim().is_empty() {
                                    "Déclarez un dossier média pour associer vos workers et lancer vos flux."
                                } else {
                                    "Ajustez votre filtre de recherche pour retrouver une bibliothèque."
                                }
                                icon_name=IconName::Libraries
                                action=create_action
                            />
                        </div>
                    }
                    .into_any();
                }
                view! {
                    <div class="libraries-grid">
                        {shown.into_iter().map(|library| {
                            let ready = is_ready(&library);
                            let draft = LibraryDraft::editing(&library);
                            let id = library.id;
                            let worker_count = library.workers.len();
                            let extra_workers = worker_count.saturating_sub(PREVIEW_WORKER_PATHS);
                            let paths_preview: Vec<(String, &'static str)> = library
                                .workers
                                .iter()
                                .take(PREVIEW_WORKER_PATHS)
                                .map(|entry| (entry.local_path.clone(), access_label(entry.access)))
                                .collect();
                            let bound_flow = flow_name(&library.flow_id);
                            let updated = format::datetime(library.updated_at);
                            view! {
                                <article
                                    class="library-card"
                                    class:is-open=move || editing.get().and_then(|d| d.id) == Some(id)
                                >
                                    <div class="library-card__head">
                                        <div class="library-card__title-wrap">
                                            <span class="library-card__icon" aria-hidden="true">
                                                <Icon name=IconName::Folder size=17 />
                                            </span>
                                            <h2 class="library-card__title">{library.name}</h2>
                                        </div>
                                        <StatusBadge
                                            label=if ready { "Prête" } else { "À configurer" }
                                            variant=if ready { "success" } else { "warning" }
                                        />
                                    </div>
                                    <div class="library-card__body">
                                        <div class="flow-binding-pill">
                                            <Icon name=IconName::Flows size=14 />
                                            <span>{bound_flow}</span>
                                        </div>
                                        <div class="library-card__paths">
                                            {if paths_preview.is_empty() {
                                                view! { <span class="text-dim text-xs">"Aucun chemin worker configuré"</span> }.into_any()
                                            } else {
                                                view! {
                                                    <div class="library-card__path-list">
                                                        {paths_preview.into_iter().map(|(path, access)| view! {
                                                            <div class="library-card__path-row">
                                                                <code class="code-badge truncate">{path}</code>
                                                                <span class="text-dim text-xs">{access}</span>
                                                            </div>
                                                        }).collect_view()}
                                                        {(extra_workers > 0).then(|| view! {
                                                            <span class="text-dim text-xs">{format!("+ {extra_workers} autre(s) worker(s)")}</span>
                                                        })}
                                                    </div>
                                                }.into_any()
                                            }}
                                        </div>
                                    </div>
                                    <div class="library-card__foot">
                                        <span class="text-dim text-xs">{format!("{worker_count} worker(s) · {updated}")}</span>
                                        {can_manage.then(|| view! {
                                            <button type="button" class="btn btn-secondary btn-sm"
                                                on:click=move |_| { editing.set(Some(draft.clone())); drawer.set(true); }>
                                                "Configurer"
                                            </button>
                                        })}
                                    </div>
                                </article>
                            }
                        }).collect_view()}
                    </div>
                }
                .into_any()
            }
        }
    };
    let form = move || {
        let draft = editing.get()?;
        let flows = flows.get().and_then(Result::ok).unwrap_or_default();
        let workers = workers.get().and_then(Result::ok).unwrap_or_default();
        Some(view! { <LibraryForm draft=draft flows=flows workers=workers on_done=on_done /> })
    };
    let title = Signal::derive(move || match editing.get() {
        Some(draft) if draft.id.is_some() => draft.name,
        _ => "Nouvelle bibliothèque".to_owned(),
    });
    if can_manage {
        provide_topbar_action(ActionSpec {
            label: "Nouvelle bibliothèque",
            on_click: open_create,
        });
    }
    let header_actions = can_manage
        .then(|| {
            view! {
                <button type="button" class="btn btn-primary" on:click=move |_| open_create.run(())>
                    <Icon name=IconName::Plus size=15 />
                    <span>"Nouvelle bibliothèque"</span>
                </button>
            }
        })
        .into_any();

    view! {
        <div class="tab-panel" role="tabpanel">
            <PageHeader
                eyebrow="Médias"
                title="Bibliothèques"
                subtitle="Répertoires surveillés, accès worker et flux de traitement associés."
                icon=IconName::Libraries
                actions=header_actions
            />
            <StatStrip aria_label="Synthèse des bibliothèques" items=stats />
            {grid}
            <Drawer open=drawer eyebrow="Bibliothèque" title=title>{form}</Drawer>
        </div>
    }
}
