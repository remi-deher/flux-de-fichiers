//! Écran Bibliothèques : liste, état et édition (nom, flux, chemins par worker) (ADR 0014).

mod draft;
mod form;

use crate::ui::filters::{Filters, LibraryFilter, library_filter};
use crate::ui::search::{matches, query};
use crate::ui::topbar_action::{ActionSpec, provide_topbar_action};
use crate::ui::{
    Drawer, EmptyState, Icon, IconName, MetricTone, ResourceCard, StatItem, StatStrip, StatusBadge,
    Tag, TagTone,
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
    let filters = use_context::<Filters>();
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
        let active = library_filter();
        let set_filter = move |next: LibraryFilter| {
            Callback::new(move |()| {
                if let Some(filters) = filters {
                    filters.library.set(next);
                }
            })
        };
        vec![
            StatItem::new("Total", total.to_string(), MetricTone::Neutral)
                .with_filter(active == LibraryFilter::All, set_filter(LibraryFilter::All)),
            StatItem::new("Prêtes", ready.to_string(), MetricTone::Success)
                .with_detail("flux + worker")
                .with_filter(
                    active == LibraryFilter::Ready,
                    set_filter(LibraryFilter::Ready),
                ),
            StatItem::new("Avec flux lié", bound.to_string(), MetricTone::Neutral).with_filter(
                active == LibraryFilter::Bound,
                set_filter(LibraryFilter::Bound),
            ),
        ]
    });

    let grid = move || {
        let text = query();
        let active_filter = library_filter();
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
                    .filter(|library| {
                        active_filter.accepts(is_ready(library), library.flow_id.is_some())
                            && matches(&[&library.name], &text)
                    })
                    .collect();
                if shown.is_empty() {
                    let unfiltered = text.trim().is_empty() && active_filter == LibraryFilter::All;
                    let create_action = (can_manage && unfiltered)
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
                                title=if unfiltered {
                                    "Aucune bibliothèque déclarée"
                                } else {
                                    "Aucune bibliothèque correspondante"
                                }
                                message=if unfiltered {
                                    "Déclarez un dossier média pour associer vos workers et lancer vos flux."
                                } else {
                                    "Ajustez vos filtres pour retrouver une bibliothèque."
                                }
                                icon_name=IconName::Libraries
                                action=create_action
                            />
                        </div>
                    }
                    .into_any();
                }
                view! {
                    <div class="resource-grid">
                        {shown.into_iter().map(|library| {
                            let ready = is_ready(&library);
                            let has_flow = library.flow_id.is_some();
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
                            let open_edit = can_manage.then(|| {
                                let draft = draft.clone();
                                Callback::new(move |()| {
                                    editing.set(Some(draft.clone()));
                                    drawer.set(true);
                                })
                            });
                            let actions = can_manage.then(|| {
                                let draft = draft.clone();
                                view! {
                                    <button type="button" class="btn btn-secondary btn-sm"
                                        on:click=move |_| { editing.set(Some(draft.clone())); drawer.set(true); }>
                                        "Configurer"
                                    </button>
                                }.into_any()
                            });
                            let badge = view! {
                                <StatusBadge
                                    label=if ready { "Prête" } else { "À configurer" }
                                    variant=if ready { "success" } else { "warning" }
                                />
                            }.into_any();
                            view! {
                                <ResourceCard
                                    title=library.name
                                    icon=IconName::Folder
                                    tone={if ready { MetricTone::Success } else { MetricTone::Warning }}
                                    badge=Some(badge)
                                    active=Some(Signal::derive(move || editing.get().and_then(|d| d.id) == Some(id)))
                                    on_click=open_edit
                                    footer_meta=format!("{worker_count} worker(s) · {updated}")
                                    footer_actions=actions
                                >
                                    <div class="cluster">
                                        <Tag
                                            label=bound_flow
                                            tone={if has_flow { TagTone::Primary } else { TagTone::Neutral }}
                                            icon=Some(IconName::Flows)
                                        />
                                        {if paths_preview.is_empty() {
                                            view! { <Tag label="Aucun chemin configuré" tone=TagTone::Warning /> }.into_any()
                                        } else {
                                            view! {
                                                {paths_preview.into_iter().map(|(path, access)| view! {
                                                    <Tag label=format!("{path} ({access})") tone=TagTone::Mono />
                                                }).collect_view()}
                                                {(extra_workers > 0).then(|| view! {
                                                    <Tag label=format!("+{extra_workers} worker(s)") tone=TagTone::Neutral />
                                                })}
                                            }.into_any()
                                        }}
                                    </div>
                                </ResourceCard>
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

    view! {
        <div class="tab-panel" role="tabpanel">
            <StatStrip aria_label="Synthèse des bibliothèques" items=stats />
            {grid}
            <Drawer open=drawer eyebrow="Bibliothèque" title=title>{form}</Drawer>
        </div>
    }
}
