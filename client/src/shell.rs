//! Coque de l'application : barre latérale (marque, navigation par groupes, compteurs live, thème, déconnexion)
//! et header commun (titre, recherche, action contextuelle, statut), voir [`header`].

mod header;

use crate::ai::{AiAssistantDrawer, AiAssistantState};
use crate::route::{Link, Route, use_router};
use crate::settings::{logout, role_label};
use crate::theme::ThemePicker;
use crate::ui::filters::Filters;
use crate::ui::search::Search;
use crate::ui::topbar_action::TopbarAction;
use crate::ui::{
    BrandMark, CommandPalette, CommandPaletteState, Icon, IconName, TagTone, ToastViewport, Toasts,
};
use chrono::{DateTime, Utc};
use forgearr_shared::brand::NAME as APP_NAME;
use forgearr_shared::jobs::{Job, JobStatus};
use forgearr_shared::session::Me;
use forgearr_shared::system::UpdateCheckStatus;
use forgearr_shared::workers::Worker;
use forgearr_shared::{InstanceKind, Permission};
use header::Topbar;
use leptos::prelude::*;

const SYSTEM_UPDATES_PATH: &str = "/api/v1/system/updates";

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

/// Groupe d'ouverture de la barre latérale : affiché sans libellé.
const GENERAL_GROUP: &str = "Général";
/// Groupe épinglé en pied de barre latérale, au-dessus de la carte utilisateur.
const SETTINGS_GROUP: &str = "Système";

/// Groupes de la barre latérale, dans l'ordre d'affichage.
pub fn groups() -> Vec<(&'static str, Vec<Screen>)> {
    vec![
        (
            GENERAL_GROUP,
            vec![
                screen(
                    Route::Overview,
                    "Vue d'ensemble",
                    IconName::Dashboard,
                    Permission::ViewOverview,
                    true,
                    "Rechercher dans la vue d'ensemble…",
                ),
                screen(
                    Route::Libraries,
                    "Librairies",
                    IconName::Libraries,
                    Permission::ViewLibraries,
                    false,
                    "Filtrer les librairies…",
                ),
            ],
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
                    "Extensions",
                    IconName::Plugins,
                    Permission::ViewPlugins,
                    true,
                    "Filtrer les extensions…",
                ),
            ],
        ),
        (
            SETTINGS_GROUP,
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

/// Version affichée dans la barre latérale : celle du serveur, sinon la version compilée.
fn displayed_version(me: &Me) -> String {
    if me.version.is_empty() {
        env!("CARGO_PKG_VERSION").to_owned()
    } else {
        me.version.clone()
    }
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

/// Titre de l'écran : `h1` de la page et début du titre de l'onglet (WCAG 2.4.2, 2.4.6).
pub fn screen_title(route: &Route) -> &'static str {
    match route {
        Route::NewFlow => "Nouveau flux",
        Route::FlowEditor(_) => "Éditeur de flux",
        other => groups()
            .into_iter()
            .flat_map(|(_, screens)| screens)
            .find(|entry| entry.route == *other)
            .map_or("Vue d'ensemble", |entry| entry.label),
    }
}

/// Groupe de la barre latérale auquel appartient l'écran : fil d'Ariane du header.
fn screen_group(route: &Route) -> Option<&'static str> {
    let section = route.section();
    groups()
        .into_iter()
        .find(|(_, screens)| screens.iter().any(|entry| entry.route == section))
        .map(|(label, _)| label)
}

/// Titre du document pour un écran : « Écran — Forgearr ».
pub fn document_title(route: &Route) -> String {
    format!("{} — {APP_NAME}", screen_title(route))
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

/// Écrans visibles pour ces droits sur cette installation.
fn visible_screens(me: &Me, screens: Vec<Screen>) -> Vec<Screen> {
    screens
        .into_iter()
        .filter(|entry| allowed(me, &entry.route))
        .collect()
}

/// Entrée de navigation : lien, pictogramme, libellé et badge live (activité, workers).
#[component]
fn NavEntry(entry: Screen, jobs: Signal<Vec<Job>>, workers: Signal<Vec<Worker>>) -> impl IntoView {
    let router = use_router();
    let section = entry.route.clone();
    let target = entry.route.clone();
    let current = Signal::derive(move || router.current().section() == section);
    let badge_slot = matches!(entry.route, Route::Jobs | Route::Workers).then(|| {
        let badge_route = entry.route.clone();
        move || {
            let badge = workers.with(|workers| {
                jobs.with(|jobs| screen_badge(&badge_route, jobs, workers, Utc::now()))
            });
            badge.map(|(count, tone)| {
                view! { <span class="tab-btn-badge" data-tone=tone.as_attr()>{count}</span> }
            })
        }
    });
    view! {
        <Link to=target class="tab-btn" title=entry.label current=current>
            <Icon name=entry.icon size=18 class="tab-role-icon" />
            <span class="tab-btn-label">{entry.label}</span>
            {badge_slot}
        </Link>
    }
}

#[component]
fn Sidebar(me: Me, jobs: Signal<Vec<Job>>, workers: Signal<Vec<Worker>>) -> impl IntoView {
    let router = use_router();
    let collapsed = RwSignal::new(false);
    let saved_collapsed = RwSignal::new(false);
    let collapse_label = move || {
        if collapsed.get() {
            "Déplier la navigation"
        } else {
            "Replier la navigation"
        }
    };
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
    let user_name = me.name.clone();
    let initials = user_initials(&user_name);
    let user_role = role_label(me.role);
    let node_badge = instance_role_badge(me.instance_kind);
    let app_version = displayed_version(&me);
    view! {
        <aside class="application-sidebar" class:is-collapsed=move || collapsed.get()
            class:is-open=move || drawer.is_some_and(|open| open.get()) aria-label="Navigation principale">
            <div class="sidebar-header">
                <a href="/" class="brand" title=APP_NAME aria-label=APP_NAME>
                    <BrandMark />
                    <div class="brand-info">
                        <span class="brand-name">{APP_NAME}</span>
                        <div class="brand-meta">
                            <span class="brand-node-badge">{node_badge}</span>
                            <span class="brand-version">{format!("v{app_version}")}</span>
                        </div>
                    </div>
                </a>
            </div>
            <nav class="tab-navigation" aria-label="Navigation principale par groupes métier">
                <div class="tab-list">
                    {groups().into_iter().filter(|(label, _)| *label != SETTINGS_GROUP).filter_map(|(label, screens)| {
                        let visible = visible_screens(&me, screens);
                        (!visible.is_empty()).then(|| view! {
                            {(label != GENERAL_GROUP).then(|| view! {
                                <span class="nav-group-label" aria-hidden="true">{label}</span>
                            })}
                            {visible.into_iter().map(|entry| view! {
                                <NavEntry entry=entry jobs=jobs workers=workers />
                            }).collect_view()}
                        })
                    }).collect_view()}
                </div>
            </nav>
            <div class="sidebar-footer">
                <div class="sidebar-footer-nav">
                    {groups().into_iter().filter(|(label, _)| *label == SETTINGS_GROUP).flat_map(|(_, screens)| visible_screens(&me, screens)).map(|entry| view! {
                        <NavEntry entry=entry jobs=jobs workers=workers />
                    }).collect_view()}
                </div>
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
                        aria-label=collapse_label
                        title=collapse_label
                        on:click=move |_| collapsed.update(|value| *value = !*value)>
                        <Icon name=IconName::ChevronLeft size=16 class="collapse-icon" />
                    </button>
                    <ThemePicker />
                </div>
            </div>
        </aside>
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
    let is_central = me.instance_kind == InstanceKind::Central;
    if is_central {
        AiAssistantState::install();
    }
    let reload = use_context::<crate::jobs::LiveJobs>().map(|live| live.reload);
    let can_central_workers =
        me.instance_kind == InstanceKind::Central && me.allows(Permission::ViewWorkers);
    let can_central_jobs =
        me.instance_kind == InstanceKind::Central && me.allows(Permission::ViewJobs);
    let can_check_updates =
        me.instance_kind == InstanceKind::Central && me.allows(Permission::ManageSettings);
    let update_resource = LocalResource::new(move || async move {
        if can_check_updates {
            crate::api::get::<UpdateCheckStatus>(SYSTEM_UPDATES_PATH)
                .await
                .ok()
        } else {
            None
        }
    });
    let update_opt = Signal::derive(move || update_resource.get().flatten());
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
    crate::ui::listen_window(leptos::ev::keydown, move |event| {
        if event.key() == "Escape" {
            nav.set(false);
        }
    });
    let is_workbench = Signal::derive(move || is_workbench_route(&router.current()));
    Effect::new(move |_| document().set_title(&document_title(&router.current())));
    view! {
        <a href="#main-content" class="skip-link">"Aller au contenu principal"</a>
        <div class="application-shell" class:is-workbench=is_workbench>
            <div class="application-navigation-layout">
                <button type="button" class="sidebar-scrim" aria-label="Fermer la navigation"
                    hidden=move || !nav.get() on:click=move |_| nav.set(false)></button>
                <Sidebar me=me.clone() jobs=jobs_list workers=workers_list />
                <div class="page-transition-region">
                    <Topbar me=me.clone() workers=workers_opt update=update_opt />
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
        <CommandPalette me=me.clone() />
        {is_central.then(|| view! { <AiAssistantDrawer me=me /> })}
        <ToastViewport />
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use forgearr_shared::Role;
    use forgearr_shared::session::CallerKind;

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
    fn every_screen_has_a_title_for_its_heading_and_its_tab() {
        assert_eq!(screen_title(&Route::Jobs), "Activité");
        assert_eq!(screen_title(&Route::NewFlow), "Nouveau flux");
        assert_eq!(
            screen_title(&Route::FlowEditor("f".into())),
            "Éditeur de flux"
        );
        for (_, screens) in groups() {
            for entry in screens {
                assert_eq!(screen_title(&entry.route), entry.label);
            }
        }
        assert_eq!(
            document_title(&Route::Overview),
            "Vue d'ensemble — Forgearr"
        );
    }

    #[test]
    fn sidebar_groups_pin_settings_alone_and_open_with_the_general_group() {
        let all = groups();
        assert_eq!(all.first().map(|(label, _)| *label), Some(GENERAL_GROUP));
        let settings: Vec<_> = all
            .iter()
            .filter(|(label, _)| *label == SETTINGS_GROUP)
            .flat_map(|(_, screens)| screens.iter().map(|entry| entry.route.clone()))
            .collect();
        assert_eq!(settings, vec![Route::Settings]);
        let general: Vec<_> = all[0].1.iter().map(|entry| entry.route.clone()).collect();
        assert_eq!(general, vec![Route::Overview, Route::Libraries]);
    }

    #[test]
    fn the_breadcrumb_is_the_sidebar_group_of_the_screen() {
        assert_eq!(screen_group(&Route::Jobs), Some("Traitements"));
        assert_eq!(screen_group(&Route::Workers), Some("Infrastructure"));
        assert_eq!(screen_group(&Route::Settings), Some(SETTINGS_GROUP));
        assert_eq!(screen_group(&Route::NewFlow), None);
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
            max_concurrency: 1,
            max_cpu_jobs: None,
            max_gpu_jobs: None,
            schedule: None,
            transfer_enabled: false,
        };
        let offline = Worker {
            id: uuid::Uuid::from_u128(2),
            name: "w2".into(),
            last_seen: now - chrono::Duration::minutes(10),
            version: None,
            remote_addr: None,
            telemetry: None,
            max_concurrency: 1,
            max_cpu_jobs: None,
            max_gpu_jobs: None,
            schedule: None,
            transfer_enabled: false,
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
