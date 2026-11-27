//! Écran Librairie : bibliothèques médias, flows & modèles, scripts locaux, ressources & modèles de blocs, et packs maison.

mod block_templates;
mod code_editor;
mod draft;
mod form;
mod history;
mod packs;
mod requirements_editor;
mod resources;
mod rows;
mod script_draft;
mod script_editor;
mod script_origin;
mod script_trial;
mod script_trial_panel;
mod scripts;
mod suggestions;
mod transfer_workers;
mod variables_editor;

use crate::live::{load, use_live};
use crate::route::encode_segment;
use crate::ui::filters::{Filters, LibraryFilter, library_filter};
use crate::ui::search::{matches, query};
use crate::ui::topbar_action::{ActionSpec, provide_topbar_action};
use crate::ui::{
    Drawer, EmptyState, IconName, MetricTone, PageSubnav, ResourceCard, StatItem, StatStrip,
    StatusBadge, Tag, TagTone, Toasts,
};
use crate::url_state::{DrawerKind, UrlTab, use_detail_drawer, use_tab};
use crate::{api, format};
use draft::{LibraryDraft, access_label, is_ready, library_counts};
use forgearr_shared::Permission;
use forgearr_shared::flows::FlowRecord;
use forgearr_shared::libraries::{Library, LibraryOutputPolicy};
use forgearr_shared::live::ChangeTopic;
use forgearr_shared::local::LocalLibrarySnapshot;
use forgearr_shared::session::Me;
use forgearr_shared::workers::Worker;
use form::LibraryForm;
use leptos::prelude::*;
use packs::LocalPacksSection;
use resources::ResourcesSection;
pub(crate) use script_draft::starter_script_code;
pub use script_editor::ScriptEditor;
pub(crate) use script_origin::script_editor_location;
use scripts::ScriptsSection;

const LIBRARIES_PATH: &str = "/api/v1/libraries";
const FLOWS_PATH: &str = "/api/v1/flows";
pub(super) const WORKERS_PATH: &str = "/api/v1/workers";
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

/// Sujets lus par la librairie locale.
pub(super) const LOCAL_TOPICS: &[ChangeTopic] = &[ChangeTopic::Catalog];

pub(super) fn use_local_delete(
    success_msg: &'static str,
) -> Action<String, Result<(), api::ApiError>> {
    let live = use_live();
    let remove = Action::new_local(move |id: &String| {
        let path = format!("{LOCAL_ITEMS_PATH}/{}", encode_segment(id));
        async move { api::delete(&path).await }
    });
    Effect::new(move |_| match remove.value().get() {
        Some(Ok(())) => {
            Toasts::notify_success(success_msg);
            live.touch(LOCAL_TOPICS);
        }
        Some(Err(err)) => Toasts::notify_error(err.message()),
        None => {}
    });
    remove
}

#[component]
pub(super) fn LocalDeleteButton(
    id: String,
    remove: Action<String, Result<(), api::ApiError>>,
) -> impl IntoView {
    view! {
        <button
            type="button"
            class="btn btn-sm btn-danger-quiet"
            disabled=move || remove.pending().get()
            on:click=move |ev| {
                ev.stop_propagation();
                remove.dispatch(id.clone());
            }
        >
            "Supprimer"
        </button>
    }
}

/// Sous-onglets de l'écran Bibliothèques (les flux, modèles et scripts sont sur l'écran Flux).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LibraryTab {
    #[default]
    Libraries,
    Resources,
    Packs,
}

impl LibraryTab {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Libraries => "Bibliothèques",
            Self::Resources => "Ressources & Blocs",
            Self::Packs => "Packs",
        }
    }

    pub const ALL: [Self; 3] = [Self::Libraries, Self::Resources, Self::Packs];
}

impl UrlTab for LibraryTab {
    fn slug(self) -> &'static str {
        match self {
            Self::Libraries => "libraries",
            Self::Resources => "resources",
            Self::Packs => "packs",
        }
    }

    fn from_slug(slug: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|tab| tab.slug() == slug)
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
    let can_view_jobs = me.allows(Permission::ViewJobs);
    let can_manage_jobs = me.allows(Permission::ManageJobs);
    let (files_library_id, files_drawer) = use_detail_drawer(DrawerKind::Library);
    // La bibliothèque vient de la liste ; l'adresse ne porte que son identifiant.
    let selected_library = Signal::derive(move || {
        let id = files_library_id.get()?;
        libraries
            .get()
            .and_then(Result::ok)?
            .into_iter()
            .find(|library| library.id.to_string() == id)
    });
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
                            let transfer_count = library.transfer_workers.len();
                            let extra_workers = worker_count.saturating_sub(PREVIEW_WORKER_PATHS);
                            let paths_preview: Vec<(String, &'static str)> = library
                                .workers
                                .iter()
                                .take(PREVIEW_WORKER_PATHS)
                                .map(|entry| (entry.local_path.clone(), access_label(entry.access)))
                                .collect();
                            let bound_flow = flow_name(&library.flow_id);
                            let scan_badge = (library.scan_interval_minutes > 0)
                                .then(|| format!("Auto {} min", library.scan_interval_minutes));
                            let policy_badge = match library.output_policy {
                                LibraryOutputPolicy::OutputDir => None,
                                LibraryOutputPolicy::ReplaceWithBackup => Some("Remplace (.bak)"),
                                LibraryOutputPolicy::ReplaceInPlace => Some("Remplace sur place"),
                            };
                            let updated = format::datetime(library.updated_at);
                            let open_edit = can_manage.then(|| {
                                let draft = draft.clone();
                                Callback::new(move |()| {
                                    editing.set(Some(draft.clone()));
                                    drawer.set(true);
                                })
                            });
                            let actions = {
                                let selected = library.clone();
                                Some(view! {
                                    {can_view_jobs.then(|| view! {
                                        <button type="button" class="btn btn-secondary btn-sm"
                                            on:click=move |event: leptos::ev::MouseEvent| { event.stop_propagation(); files_library_id.set(Some(selected.id.to_string())); files_drawer.set(true); }>
                                            "Fichiers"
                                        </button>
                                    })}
                                    {can_manage.then(|| view! {
                                        <button type="button" class="btn btn-secondary btn-sm"
                                            on:click=move |event: leptos::ev::MouseEvent| { event.stop_propagation(); editing.set(Some(draft.clone())); drawer.set(true); }>
                                            "Configurer"
                                        </button>
                                    })}
                                }.into_any())
                            };
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
                                    footer_meta=format!("{worker_count} dossier(s) · {updated}")
                                    footer_actions=actions
                                >
                                    <div class="cluster">
                                        <Tag
                                            label=bound_flow
                                            tone={if has_flow { TagTone::Primary } else { TagTone::Neutral }}
                                            icon=Some(IconName::Flows)
                                        />
                                        {scan_badge.map(|label| view! {
                                            <Tag label=label tone=TagTone::Success icon=Some(IconName::Refresh) />
                                        })}
                                        {policy_badge.map(|label| view! {
                                            <Tag label=label tone=TagTone::Neutral />
                                        })}
                                        {if paths_preview.is_empty() {
                                            view! { <Tag label="Aucun chemin configuré" tone=TagTone::Warning /> }.into_any()
                                        } else {
                                            view! {
                                                {paths_preview.into_iter().map(|(path, access)| view! {
                                                    <Tag label=format!("{path} ({access})") tone=TagTone::Mono />
                                                }).collect_view()}
                                                {(extra_workers > 0).then(|| view! {
                                                    <Tag label=format!("+{extra_workers} dossier(s)") tone=TagTone::Neutral />
                                                })}
                                            }.into_any()
                                        }}
                                        {(transfer_count > 0).then(|| view! {
                                            <Tag label=format!("{transfer_count} worker(s) par transfert") tone=TagTone::Primary />
                                        })}
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
        <Drawer open=files_drawer eyebrow="Fichiers" title=Signal::derive(move || selected_library.get().map(|library| library.name).unwrap_or_default())>
            {move || files_drawer.get().then(|| selected_library.get()).flatten().map(|library| view! {
                <history::LibraryHistory library=library can_scan=can_manage can_manage_jobs=can_manage_jobs
                    workers=workers.get().and_then(Result::ok).unwrap_or_default() />
            })}
        </Drawer>

    }
}

/// Liste des scripts locaux : onglet Scripts de l'écran Flux.
#[component]
pub fn ScriptsPanel(me: Me) -> impl IntoView {
    let snapshot = load::<LocalLibrarySnapshot>(LOCAL_TOPICS, LOCAL_PATH);
    let can_manage = me.allows(Permission::ManageSettings);
    view! { <ScriptsSection snapshot=snapshot can_manage=can_manage /> }
}

#[component]
pub fn LibrariesPage(me: Me) -> impl IntoView {
    let can_manage_local = me.allows(Permission::ManageSettings);
    let active_tab = use_tab(LibraryTab::default(), LibraryTab::ALL.to_vec());
    let reload_libs = RwSignal::new(0_u32);

    let libraries = LocalResource::new(move || {
        reload_libs.track();
        api::get::<Vec<Library>>(LIBRARIES_PATH)
    });
    let flows = LocalResource::new(|| api::get::<Vec<FlowRecord>>(FLOWS_PATH));
    let workers = LocalResource::new(|| api::get::<Vec<Worker>>(WORKERS_PATH));
    let snapshot = load::<LocalLibrarySnapshot>(LOCAL_TOPICS, LOCAL_PATH);

    view! {
        <div class="tab-panel" role="tabpanel">
            <PageSubnav
                aria_label="Sections des bibliothèques"
                tabs=LibraryTab::ALL.into_iter().map(|tab| (tab, tab.label())).collect()
                active=active_tab
            />
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
                LibraryTab::Resources => view! {
                    <ResourcesSection
                        snapshot=snapshot
                        flows=flows
                        can_manage=can_manage_local
                    />
                }.into_any(),
                LibraryTab::Packs => view! {
                    <LocalPacksSection snapshot=snapshot can_manage=can_manage_local />
                }.into_any(),
            }}
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn library_tabs_leave_flows_templates_and_scripts_to_the_flows_screen() {
        assert_eq!(LibraryTab::ALL.len(), 3);
        assert_eq!(LibraryTab::Libraries.label(), "Bibliothèques");
        assert_eq!(LibraryTab::Resources.label(), "Ressources & Blocs");
        assert_eq!(LibraryTab::from_slug("flows"), None);
        assert_eq!(LibraryTab::from_slug("scripts"), None);
    }

    #[test]
    fn library_tabs_are_named_in_the_address_and_read_back() {
        for tab in LibraryTab::ALL {
            assert_eq!(LibraryTab::from_slug(tab.slug()), Some(tab));
        }
        assert_eq!(LibraryTab::from_slug("inconnu"), None);
    }
}
