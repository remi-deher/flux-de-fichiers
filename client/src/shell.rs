//! Coque de l'application : barre latérale (marque, navigation par groupes, compteurs live, thème, déconnexion)
//! et barre du haut (filtre instantané, période, workers en ligne et action contextuelle).

use crate::route::{Link, Route, use_router};
use crate::settings::{logout, role_label};
use crate::theme::ThemePicker;
use crate::ui::filters::{FilterSheet, Filters};
use crate::ui::search::Search;
use crate::ui::topbar_action::{TopbarAction, TopbarActionSlot};
use crate::ui::{
    CommandPalette, CommandPaletteState, Icon, IconName, TagTone, ToastViewport, Toasts,
    breadcrumb_for,
};
use chrono::{DateTime, Utc};
use flux_shared::jobs::{Job, JobStatus};
use flux_shared::session::Me;
use flux_shared::workers::Worker;
use flux_shared::{InstanceKind, Permission};
use leptos::prelude::*;

/// Écran de la navigation : route, libellé, pictogramme, droit, hébergé sur un worker, recherche.
pub struct Screen {
    pub route: Route,
    pub label: &'static str,
    pub icon: IconName,
    pub permission: Permission,
    pub on_worker: bool,
    pub search: &'static str,
}

const fn screen(
    route: Route,
    label: &'static str,
    icon: IconName,
    permission: Permission,
    on_worker: bool,
    search: &'static str,
) -> Screen {
    Screen {
        route,
        label,
        icon,
        permission,
        on_worker,
        search,
    }
}

/// Groupes de la barre latérale, dans l'ordre d'affichage.
pub fn groups() -> Vec<(&'static str, Vec<Screen>)> {
    vec![
        (
            "Vue générale",
            vec![screen(
                Route::Overview,
                "Vue d'ensemble",
                IconName::Dashboard,
                Permission::ViewOverview,
                true,
                "Rechercher dans la vue d'ensemble…",
            )],
        ),
        (
            "Médias",
            vec![screen(
                Route::Libraries,
                "Bibliothèques",
                IconName::Libraries,
                Permission::ViewLibraries,
                false,
                "Filtrer les bibliothèques…",
            )],
        ),
        (
            "Traitements",
            vec![
                screen(
                    Route::Flows,
                    "Flows",
                    IconName::Flows,
                    Permission::ViewFlows,
                    false,
                    "Filtrer les flux…",
                ),
                screen(
                    Route::Jobs,
                    "Activité",
                    IconName::Activity,
                    Permission::ViewJobs,
                    true,
                    "Filtrer les tâches…",
                ),
            ],
        ),
        (
            "Infrastructure",
            vec![
                screen(
                    Route::Workers,
                    "Workers",
                    IconName::Workers,
                    Permission::ViewWorkers,
                    false,
                    "Filtrer les workers…",
                ),
                screen(
                    Route::Plugins,
                    "Plugins",
                    IconName::Plugins,
                    Permission::ViewPlugins,
                    true,
                    "Filtrer le catalogue…",
                ),
            ],
        ),
        (
            "Système",
            vec![screen(
                Route::Settings,
                "Paramètres",
                IconName::Settings,
                Permission::ViewOverview,
                true,
                "Filtrer les paramètres…",
            )],
        ),
    ]
}

/// L'écran (ou sa section) est-il visible pour ces droits, sur cette installation ?
pub fn allowed(me: &Me, route: &Route) -> bool {
    let section = route.section();
    if section == Route::NewFlow {
        return me.instance_kind == InstanceKind::Central && me.allows(Permission::ManageFlows);
    }
    groups()
        .into_iter()
        .flat_map(|(_, screens)| screens)
        .any(|entry| {
            entry.route == section
                && me.allows(entry.permission)
                && (me.instance_kind == InstanceKind::Central || entry.on_worker)
        })
}

fn search_placeholder(route: &Route) -> &'static str {
    let section = route.section();
    groups()
        .into_iter()
        .flat_map(|(_, screens)| screens)
        .find(|entry| entry.route == section)
        .map_or("Rechercher…", |entry| entry.search)
}

/// Tiroir de navigation des écrans étroits : ouvert par la barre du haut, fermé par le voile,
/// Échap ou le choix d'un écran.
#[derive(Clone, Copy)]
struct NavDrawer(RwSignal<bool>);

/// Initiales (1 ou 2 lettres majuscules) affichées dans l'avatar utilisateur.
pub fn user_initials(name: &str) -> String {
    let mut parts = name
        .split(|ch: char| ch.is_whitespace() || matches!(ch, '.' | '_' | '-'))
        .filter(|part| !part.is_empty());
    match (parts.next(), parts.next()) {
        (Some(first), Some(second)) => first
            .chars()
            .take(1)
            .chain(second.chars().take(1))
            .flat_map(char::to_uppercase)
            .collect(),
        (Some(single), None) => single
            .chars()
            .take(2)
            .flat_map(char::to_uppercase)
            .collect(),
        _ => "?".to_owned(),
    }
}

/// Badge d'identité du nœud affiché sous la marque de l'application.
pub const fn instance_role_badge(kind: InstanceKind) -> &'static str {
    match kind {
        InstanceKind::Central => "Central",
        InstanceKind::Worker => "Worker",
    }
}

/// Compteur affiché dans la navigation latérale sur l'onglet Activité.
pub fn activity_badge(jobs: &[Job]) -> Option<(usize, TagTone)> {
    let active = jobs
        .iter()
        .filter(|job| matches!(job.status, JobStatus::Running | JobStatus::Queued))
        .count();
    if active > 0 {
        return Some((active, TagTone::Primary));
    }
    let failed = jobs
        .iter()
        .filter(|job| job.status == JobStatus::Failed)
        .count();
    (failed > 0).then_some((failed, TagTone::Danger))
}

/// Compteur d'alerte affiché dans la navigation latérale lorsque des workers sont hors ligne.
pub fn workers_alert_badge(workers: &[Worker], now: DateTime<Utc>) -> Option<(usize, TagTone)> {
    let offline = workers
        .iter()
        .filter(|worker| !worker.is_online(now))
        .count();
    if offline == 0 {
        None
    } else if offline == workers.len() {
        Some((offline, TagTone::Danger))
    } else {
        Some((offline, TagTone::Warning))
    }
}

fn screen_badge(
    route: &Route,
    jobs: &[Job],
    workers: &[Worker],
    now: DateTime<Utc>,
) -> Option<(usize, TagTone)> {
    match route {
        Route::Jobs => activity_badge(jobs),
        Route::Workers => workers_alert_badge(workers, now),
        _ => None,
    }
}

/// La route courante appartient-elle à l'atelier plein écran de création/édition de flux ?
pub fn is_workbench_route(route: &Route) -> bool {
    matches!(route, Route::FlowEditor(_) | Route::NewFlow)
}

#[component]
fn Sidebar(me: Me, jobs: Signal<Vec<Job>>, workers: Signal<Vec<Worker>>) -> impl IntoView {
    let router = use_router();
    let collapsed = RwSignal::new(false);
    let saved_collapsed = RwSignal::new(false);
    let was_workbench = RwSignal::new(false);
    Effect::new(move |_| {
        let in_workbench = is_workbench_route(&router.current());
        let prev = was_workbench.get_untracked();
        if in_workbench && !prev {
            saved_collapsed.set(collapsed.get_untracked());
            collapsed.set(true);
        } else if !in_workbench && prev {
            collapsed.set(saved_collapsed.get_untracked());
        }
        was_workbench.set(in_workbench);
    });
    let drawer = use_context::<NavDrawer>().map(|drawer| drawer.0);
    let palette = use_context::<CommandPaletteState>();
    let user_name = me.name.clone();
    let initials = user_initials(&user_name);
    let user_role = role_label(me.role);
    let node_badge = instance_role_badge(me.instance_kind);
    view! {
        <aside class="application-sidebar" class:is-collapsed=move || collapsed.get()
            class:is-open=move || drawer.is_some_and(|open| open.get()) aria-label="Navigation principale">
            <div class="sidebar-header">
                <a href="/" class="brand" title="Flux de Fichiers">
                    <span class="brand-mark" aria-hidden="true">
                        <Icon name=IconName::Flows size=18 />
                    </span>
                    <div class="brand-info">
                        <span class="brand-name">"Flux de Fichiers"</span>
                        <div class="brand-meta">
                            <span class="brand-node-badge">{node_badge}</span>
                            <span class="brand-version">{format!("v{}", env!("CARGO_PKG_VERSION"))}</span>
                        </div>
                    </div>
                </a>
                <button
                    type="button"
                    class="sidebar-cmd-btn"
                    title="Palette de commandes (Ctrl+K)"
                    aria-label="Ouvrir la palette de commandes"
                    on:click=move |_| if let Some(palette) = palette { palette.show(); }
                >
                    <Icon name=IconName::Search size=14 />
                    <span class="sidebar-cmd-btn__label">"Aller à…"</span>
                    <kbd class="sidebar-cmd-btn__kbd">"Ctrl K"</kbd>
                </button>
            </div>
            <nav class="tab-navigation" aria-label="Navigation principale par groupes métier">
                <div class="tab-list" role="tablist">
                    {groups().into_iter().filter_map(|(label, screens)| {
                        let visible: Vec<Screen> = screens.into_iter().filter(|entry| allowed(&me, &entry.route)).collect();
                        (!visible.is_empty()).then(|| view! {
                            <span class="nav-group-label" aria-hidden="true">{label}</span>
                            {visible.into_iter().map(|entry| {
                                let section = entry.route.clone();
                                let target = entry.route.clone();
                                let badge_slot = matches!(entry.route, Route::Jobs | Route::Workers).then(|| {
                                    let badge_route = entry.route.clone();
                                    move || {
                                        let badge = workers.with(|workers| {
                                            jobs.with(|jobs| screen_badge(&badge_route, jobs, workers, Utc::now()))
                                        });
                                        badge.map(|(count, tone)| view! {
                                            <span class="tab-btn-badge" data-tone=tone.as_attr()>{count}</span>
                                        })
                                    }
                                });
                                view! {
                                    <button type="button" role="tab" class="tab-btn" title=entry.label
                                        aria-selected=move || (router.current().section() == section).to_string()
                                        on:click=move |_| router.go(target.clone())>
                                        <Icon name=entry.icon size=18 class="tab-role-icon" />
                                        <span class="tab-btn-label">{entry.label}</span>
                                        {badge_slot}
                                    </button>
                                }
                            }).collect_view()}
                        })
                    }).collect_view()}
                </div>
            </nav>
            <div class="sidebar-footer">
                <div class="sidebar-user-card">
                    <span class="sidebar-user-avatar" aria-hidden="true">{initials}</span>
                    <div class="sidebar-user-meta">
                        <span class="sidebar-user-name">{user_name}</span>
                        <span class="sidebar-user-role">{user_role}</span>
                    </div>
                    <button type="button" class="sidebar-logout-btn" aria-label="Se déconnecter" title="Se déconnecter"
                        on:click=move |_| leptos::task::spawn_local(logout())>
                        <Icon name=IconName::Logout size=15 />
                    </button>
                </div>
                <div class="sidebar-footer-tools">
                    <button type="button" class="sidebar-collapse-btn"
                        aria-label=move || if collapsed.get() { "Déplier la navigation" } else { "Replier la navigation" }
                        on:click=move |_| collapsed.update(|value| *value = !*value)>
                        <Icon name=IconName::ChevronLeft size=16 class="collapse-icon" />
                        <span class="collapse-label">"Replier"</span>
                    </button>
                    <ThemePicker />
                </div>
            </div>
        </aside>
    }
}

/// Indicateur cliquable des workers en ligne (central seulement), relu à chaque événement SSE.
#[component]
fn WorkersPill(workers: Signal<Option<Vec<Worker>>>) -> impl IntoView {
    move || {
        let list = workers.get()?;
        let online = list
            .iter()
            .filter(|worker| worker.is_online(Utc::now()))
            .count();
        let (class, text) = match online {
            0 => ("status-pill offline", "Aucun worker en ligne".to_owned()),
            1 => ("status-pill online", "1 worker en ligne".to_owned()),
            count => ("status-pill online", format!("{count} workers en ligne")),
        };
        Some(view! {
            <Link to=Route::Workers class="status-pill-link">
                <span class=class aria-live="polite" title="Ouvrir les workers (central inclus)">
                    <span class="status-dot"></span>
                    <span>{text}</span>
                </span>
            </Link>
        })
    }
}

#[component]
fn Topbar(me: Me, workers: Signal<Option<Vec<Worker>>>) -> impl IntoView {
    let router = use_router();
    let search = use_context::<Search>().unwrap_or_else(Search::install);
    let nav = use_context::<NavDrawer>().map(|drawer| drawer.0);
    let filters = use_context::<Filters>().unwrap_or_else(Filters::install);
    let palette = use_context::<CommandPaletteState>();
    Effect::new(move |_| {
        router.current();
        search.text.set(String::new());
        filters.apply_route_change();
        if let Some(nav) = nav {
            nav.set(false);
        }
    });
    let central = me.instance_kind == InstanceKind::Central && me.allows(Permission::ViewWorkers);
    view! {
        <header class="app-topbar" role="banner">
            <div class="topbar-container">
                <div class="topbar-lead">
                    <button type="button" class="topbar-menu-btn" aria-label="Ouvrir la navigation"
                        aria-expanded=move || nav.is_some_and(|open| open.get()).to_string()
                        on:click=move |_| if let Some(nav) = nav { nav.update(|open| *open = !*open) }>
                        <Icon name=IconName::Menu size=20 />
                    </button>
                    <nav class="topbar-breadcrumb" aria-label="Fil d'Ariane">
                        {move || {
                            let (group, screen) = breadcrumb_for(&router.current());
                            view! {
                                <span class="topbar-breadcrumb__group">{group}</span>
                                <span class="topbar-breadcrumb__sep" aria-hidden="true">"/"</span>
                                <span class="topbar-breadcrumb__page">{screen}</span>
                            }
                        }}
                    </nav>
                </div>
                <div class="topbar-search-anchor">
                    <div class="topbar-search" role="search">
                        <div class="search-capsule__query">
                            <Icon name=IconName::Search size=15 class="search-icon" />
                            <input type="search" class="topbar-search-input" autocomplete="off" spellcheck="false"
                                enterkeyhint="search" aria-label="Filtrer l'écran actif"
                                placeholder=move || search_placeholder(&router.current())
                                prop:value=move || search.text.get()
                                on:input=move |event| search.text.set(event_target_value(&event)) />
                            {move || (!search.text.get().is_empty()).then(|| view! {
                                <button
                                    type="button"
                                    class="topbar-search-clear"
                                    aria-label="Effacer le filtre"
                                    title="Effacer le filtre"
                                    on:click=move |_| search.text.set(String::new())
                                >
                                    <Icon name=IconName::Close size=13 />
                                </button>
                            })}
                            <button
                                type="button"
                                class="kbd-shortcut"
                                title="Ouvrir la palette de commandes (Ctrl+K)"
                                aria-label="Ouvrir la palette de commandes"
                                on:click=move |_| if let Some(palette) = palette { palette.show(); }
                            >
                                "Ctrl K"
                            </button>
                        </div>
                    </div>
                </div>
                <div class="topbar-actions">
                    <FilterSheet route=Signal::derive(move || router.current()) />
                    {central.then(|| view! { <WorkersPill workers=workers /> })}
                    <TopbarActionSlot />
                </div>
            </div>
        </header>
    }
}

/// Gabarit complet : barre latérale, barre du haut et contenu.
#[component]
pub fn Shell(me: Me, children: Children) -> impl IntoView {
    let router = use_router();
    Search::install();
    Filters::install();
    TopbarAction::install();
    Toasts::install();
    CommandPaletteState::install();
    let reload = use_context::<crate::jobs::LiveJobs>().map(|live| live.reload);
    let can_central_workers =
        me.instance_kind == InstanceKind::Central && me.allows(Permission::ViewWorkers);
    let can_central_jobs =
        me.instance_kind == InstanceKind::Central && me.allows(Permission::ViewJobs);
    let central_workers = LocalResource::new(move || {
        if let Some(reload) = reload {
            reload.track();
        }
        async move {
            if can_central_workers {
                crate::api::get::<Vec<Worker>>("/api/v1/workers").await.ok()
            } else {
                None
            }
        }
    });
    let central_jobs = LocalResource::new(move || {
        if let Some(reload) = reload {
            reload.track();
        }
        async move {
            if can_central_jobs {
                crate::api::get::<Vec<Job>>("/api/v1/jobs?limit=200")
                    .await
                    .ok()
            } else {
                None
            }
        }
    });
    let workers_opt = Signal::derive(move || central_workers.get().flatten());
    let workers_list = Signal::derive(move || workers_opt.get().unwrap_or_default());
    let jobs_list = Signal::derive(move || central_jobs.get().flatten().unwrap_or_default());
    let nav = RwSignal::new(false);
    provide_context(NavDrawer(nav));
    let _escape = window_event_listener(leptos::ev::keydown, move |event| {
        if event.key() == "Escape" {
            nav.set(false);
        }
    });
    let is_workbench = Signal::derive(move || is_workbench_route(&router.current()));
    view! {
        <a href="#main-content" class="skip-link">"Aller au contenu principal"</a>
        <div class="application-shell" class:is-workbench=is_workbench>
            <div class="application-navigation-layout">
                <button type="button" class="sidebar-scrim" aria-label="Fermer la navigation"
                    hidden=move || !nav.get() on:click=move |_| nav.set(false)></button>
                <Sidebar me=me.clone() jobs=jobs_list workers=workers_list />
                <div class="page-transition-region">
                    <Topbar me=me.clone() workers=workers_opt />
                    <main
                        id="main-content"
                        class="main-content"
                        class:is-workbench=is_workbench
                    >
                        {children()}
                    </main>
                </div>
            </div>
        </div>
        <CommandPalette me=me />
        <ToastViewport />
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use flux_shared::Role;
    use flux_shared::session::CallerKind;

    fn me(role: Role, instance_kind: InstanceKind) -> Me {
        Me::new(
            CallerKind::Account,
            "remi".into(),
            role,
            instance_kind,
            None,
        )
    }

    #[test]
    fn a_worker_only_offers_its_own_screens() {
        let worker = me(Role::Admin, InstanceKind::Worker);
        assert!(allowed(&worker, &Route::Overview));
        assert!(allowed(&worker, &Route::Plugins));
        assert!(!allowed(&worker, &Route::Flows));
        assert!(!allowed(&worker, &Route::FlowEditor("f".into())));
        assert!(!allowed(&worker, &Route::NewFlow));
        let central = me(Role::Admin, InstanceKind::Central);
        assert!(allowed(&central, &Route::FlowEditor("f".into())));
        assert!(allowed(&central, &Route::NewFlow));
    }

    #[test]
    fn a_viewer_cannot_open_the_flow_creation_and_every_screen_has_a_placeholder() {
        let viewer = me(Role::Viewer, InstanceKind::Central);
        assert!(allowed(&viewer, &Route::Flows));
        assert!(!allowed(&viewer, &Route::NewFlow));
        assert_eq!(
            search_placeholder(&Route::FlowEditor("f".into())),
            "Filtrer les flux…"
        );
    }

    #[test]
    fn workbench_route_detection_matches_editor_screens_only() {
        assert!(is_workbench_route(&Route::NewFlow));
        assert!(is_workbench_route(&Route::FlowEditor("flow.1".into())));
        assert!(!is_workbench_route(&Route::Flows));
        assert!(!is_workbench_route(&Route::Overview));
    }

    #[test]
    fn user_initials_and_node_badge_cover_edge_cases() {
        assert_eq!(user_initials("Rémi Dupont"), "RD");
        assert_eq!(user_initials("remi.dupont"), "RD");
        assert_eq!(user_initials("admin"), "AD");
        assert_eq!(user_initials(" "), "?");
        assert_eq!(instance_role_badge(InstanceKind::Central), "Central");
        assert_eq!(instance_role_badge(InstanceKind::Worker), "Worker");
    }

    fn job_with_status(id: u128, status: JobStatus) -> Result<Job, serde_json::Error> {
        let mut job: Job = serde_json::from_value(serde_json::json!({
            "id": uuid::Uuid::from_u128(id), "source_name": "a.mkv", "source_sha256": null,
            "source_bytes": null, "output_sha256": null, "output_bytes": null, "warning": false,
            "target_worker_id": null, "worker_id": null, "status": "queued", "attempts": 0,
            "leased_until": null, "output_name": null, "error_code": null, "flow_id": null,
            "flow_version": null, "created_at": "2026-11-14T10:00:00Z", "updated_at": "2026-11-14T10:00:00Z"
        }))?;
        job.status = status;
        Ok(job)
    }

    #[test]
    fn activity_badge_prefers_active_then_failed() -> Result<(), serde_json::Error> {
        let done = job_with_status(1, JobStatus::Succeeded)?;
        assert_eq!(activity_badge(&[]), None);
        assert_eq!(activity_badge(std::slice::from_ref(&done)), None);

        let failed = job_with_status(2, JobStatus::Failed)?;
        assert_eq!(
            activity_badge(&[done.clone(), failed.clone()]),
            Some((1, TagTone::Danger))
        );

        let running = job_with_status(3, JobStatus::Running)?;
        let queued = job_with_status(4, JobStatus::Queued)?;
        assert_eq!(
            activity_badge(&[done, failed, running, queued]),
            Some((2, TagTone::Primary))
        );
        Ok(())
    }

    #[test]
    fn workers_alert_badge_reflects_offline_count() {
        let now = Utc::now();
        let online = Worker {
            id: uuid::Uuid::from_u128(1),
            name: "w1".into(),
            last_seen: now,
            version: None,
            remote_addr: None,
            telemetry: None,
        };
        let offline = Worker {
            id: uuid::Uuid::from_u128(2),
            name: "w2".into(),
            last_seen: now - chrono::Duration::minutes(10),
            version: None,
            remote_addr: None,
            telemetry: None,
        };
        assert_eq!(workers_alert_badge(&[], now), None);
        assert_eq!(
            workers_alert_badge(std::slice::from_ref(&online), now),
            None
        );
        assert_eq!(
            workers_alert_badge(&[online, offline.clone()], now),
            Some((1, TagTone::Warning))
        );
        assert_eq!(
            workers_alert_badge(&[offline], now),
            Some((1, TagTone::Danger))
        );
    }
}
