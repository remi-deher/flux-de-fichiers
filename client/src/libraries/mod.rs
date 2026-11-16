//! Écran Librairie : bibliothèques médias, flows & modèles, scripts locaux, ressources & modèles de blocs, et packs maison.

mod draft;
mod form;
mod packs;
mod resources;
mod scripts;

use crate::flows::FlowsSection;
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
use flux_shared::local::LocalLibrarySnapshot;
use flux_shared::session::Me;
use flux_shared::workers::Worker;
use form::LibraryForm;
use leptos::prelude::*;
use packs::LocalPacksSection;
use resources::ResourcesSection;
use scripts::ScriptsSection;

const LIBRARIES_PATH: &str = "/api/v1/libraries";
const FLOWS_PATH: &str = "/api/v1/flows";
const WORKERS_PATH: &str = "/api/v1/workers";
pub(super) const LOCAL_PATH: &str = "/api/v1/local";
pub(super) const LOCAL_SCRIPTS_PATH: &str = "/api/v1/local/scripts";
pub(super) const LOCAL_RESOURCES_PATH: &str = "/api/v1/local/resources";
pub(super) const LOCAL_BLOCK_TEMPLATES_PATH: &str = "/api/v1/local/block-templates";
pub(super) const LOCAL_PACKS_PATH: &str = "/api/v1/local/packs";
pub(super) const LOCAL_ITEMS_PATH: &str = "/api/v1/local/items";
const PREVIEW_WORKER_PATHS: usize = 2;

pub(super) fn bump(signal: RwSignal<u32>) {
    signal.update(|count| *count = count.wrapping_add(1));
}

/// Sous-onglets de l'écran Librairie.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LibraryTab {
    #[default]
    Libraries,
    Flows,
    Scripts,
    Resources,
    Packs,
}

impl LibraryTab {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Libraries => "Bibliothèques",
            Self::Flows => "Flows & Modèles",
            Self::Scripts => "Scripts",
            Self::Resources => "Ressources & Blocs",
            Self::Packs => "Packs",
        }
    }

    pub fn visible_tabs(me: &Me) -> Vec<Self> {
        let mut tabs = vec![Self::Libraries];
        if me.allows(Permission::ViewFlows) {
            tabs.push(Self::Flows);
        }
        tabs.push(Self::Scripts);
        tabs.push(Self::Resources);
        tabs.push(Self::Packs);
        tabs
    }
}

#[component]
fn MediaLibrariesSection(
    me: Me,
    libraries: LocalResource<Result<Vec<Library>, api::ApiError>>,
    flows: LocalResource<Result<Vec<FlowRecord>, api::ApiError>>,
    workers: LocalResource<Result<Vec<Worker>, api::ApiError>>,
    reload: RwSignal<u32>,
) -> impl IntoView {
    let can_manage = me.allows(Permission::ManageLibraries);
    let filters = use_context::<Filters>();
    let editing = RwSignal::new(None::<LibraryDraft>);
    let drawer = RwSignal::new(false);
    let open_create = Callback::new(move |()| {
        editing.set(Some(LibraryDraft::default()));
        drawer.set(true);
    });
    let on_done = Callback::new(move |()| {
        editing.set(None);
        drawer.set(false);
        bump(reload);
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
        let counts = library_counts(&list);
        let active = library_filter();
        let set_filter = move |next: LibraryFilter| {
            Callback::new(move |()| {
                if let Some(filters) = filters {
                    filters.library.set(next);
                }
            })
        };
        let tone_for = |count: usize| {
            if count > 0 {
                MetricTone::Warning
            } else {
                MetricTone::Neutral
            }
        };
        vec![
            StatItem::new("Toutes", counts.total.to_string(), MetricTone::Neutral)
                .with_filter(active == LibraryFilter::All, set_filter(LibraryFilter::All)),
            StatItem::new("Prêtes", counts.ready.to_string(), MetricTone::Success)
                .with_detail("flux + worker")
                .with_filter(
                    active == LibraryFilter::Ready,
                    set_filter(LibraryFilter::Ready),
                ),
            StatItem::new(
                "Sans flux",
                counts.missing_flow.to_string(),
                tone_for(counts.missing_flow),
            )
            .with_filter(
                active == LibraryFilter::MissingFlow,
                set_filter(LibraryFilter::MissingFlow),
            ),
            StatItem::new(
                "Sans worker",
                counts.missing_worker.to_string(),
                tone_for(counts.missing_worker),
            )
            .with_filter(
                active == LibraryFilter::MissingWorker,
                set_filter(LibraryFilter::MissingWorker),
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
                        active_filter
                            .accepts(library.flow_id.is_some(), !library.workers.is_empty())
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
        <StatStrip aria_label="Synthèse des bibliothèques" items=stats />
        {grid}
        <Drawer open=drawer eyebrow="Bibliothèque" title=title>{form}</Drawer>
    }
}

#[component]
pub fn LibrariesPage(me: Me) -> impl IntoView {
    let can_manage_local = me.allows(Permission::ManageSettings);
    let active_tab = RwSignal::new(LibraryTab::Libraries);
    let reload_libs = RwSignal::new(0_u32);
    let reload_local = RwSignal::new(0_u32);

    let libraries = LocalResource::new(move || {
        reload_libs.track();
        api::get::<Vec<Library>>(LIBRARIES_PATH)
    });
    let flows = LocalResource::new(|| api::get::<Vec<FlowRecord>>(FLOWS_PATH));
    let workers = LocalResource::new(|| api::get::<Vec<Worker>>(WORKERS_PATH));
    let snapshot = LocalResource::new(move || {
        reload_local.track();
        api::get::<LocalLibrarySnapshot>(LOCAL_PATH)
    });
    let tabs = LibraryTab::visible_tabs(&me);

    view! {
        <div class="tab-panel" role="tabpanel">
            <nav class="page-subnav" role="tablist" aria-label="Sections de la librairie">
                {tabs.into_iter().map(|tab| {
                    let is_active = move || active_tab.get() == tab;
                    view! {
                        <button
                            type="button"
                            role="tab"
                            class="page-subnav__tab"
                            class:is-active=is_active
                            aria-selected=move || is_active().to_string()
                            on:click=move |_| active_tab.set(tab)
                        >
                            {tab.label()}
                        </button>
                    }
                }).collect_view()}
            </nav>
            {move || match active_tab.get() {
                LibraryTab::Libraries => view! {
                    <MediaLibrariesSection
                        me=me.clone()
                        libraries=libraries
                        flows=flows
                        workers=workers
                        reload=reload_libs
                    />
                }.into_any(),
                LibraryTab::Flows => view! {
                    <FlowsSection me=me.clone() with_topbar=false />
                }.into_any(),
                LibraryTab::Scripts => view! {
                    <ScriptsSection snapshot=snapshot reload=reload_local can_manage=can_manage_local />
                }.into_any(),
                LibraryTab::Resources => view! {
                    <ResourcesSection
                        snapshot=snapshot
                        flows=flows
                        reload=reload_local
                        can_manage=can_manage_local
                    />
                }.into_any(),
                LibraryTab::Packs => view! {
                    <LocalPacksSection snapshot=snapshot reload=reload_local can_manage=can_manage_local />
                }.into_any(),
            }}
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use flux_shared::InstanceKind;
    use flux_shared::Role;
    use flux_shared::session::CallerKind;

    #[test]
    fn library_tabs_respect_flow_view_permission() {
        let admin = Me::new(
            CallerKind::Account,
            "admin".into(),
            Role::Admin,
            InstanceKind::Central,
            None,
        );
        assert_eq!(LibraryTab::visible_tabs(&admin).len(), 5);
        assert_eq!(LibraryTab::Libraries.label(), "Bibliothèques");
        assert_eq!(LibraryTab::Flows.label(), "Flows & Modèles");
        assert_eq!(LibraryTab::Resources.label(), "Ressources & Blocs");
    }
}
