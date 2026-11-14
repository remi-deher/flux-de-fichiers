//! Coque de l'application : barre latérale (marque, navigation par groupes, thème, déconnexion)
//! et barre du haut (recherche contextuelle, workers en ligne), sur le gabarit du thème.

use crate::route::{Route, use_router};
use crate::settings::logout;
use crate::theme::ThemePicker;
use crate::ui::search::Search;
use crate::ui::{Icon, IconName};
use chrono::Utc;
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

#[component]
fn Sidebar(me: Me) -> impl IntoView {
    let router = use_router();
    let collapsed = RwSignal::new(false);
    view! {
        <aside class="application-sidebar" class:is-collapsed=move || collapsed.get() aria-label="Navigation principale">
            <div class="sidebar-header">
                <a href="/" class="brand" title="Flux de Fichiers">
                    <span class="brand-mark">"MKV"</span>
                    <div class="brand-info">
                        <span class="brand-name">"Flux de Fichiers"</span>
                        <span class="brand-version">{format!("v{}", env!("CARGO_PKG_VERSION"))}</span>
                    </div>
                </a>
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
                                view! {
                                    <button type="button" role="tab" class="tab-btn" title=entry.label
                                        aria-selected=move || (router.current().section() == section).to_string()
                                        on:click=move |_| router.go(target.clone())>
                                        <Icon name=entry.icon size=18 class="tab-role-icon" />
                                        <span class="tab-btn-label">{entry.label}</span>
                                    </button>
                                }
                            }).collect_view()}
                        })
                    }).collect_view()}
                </div>
            </nav>
            <div class="sidebar-footer">
                <button type="button" class="sidebar-collapse-btn"
                    aria-label=move || if collapsed.get() { "Déplier la navigation" } else { "Replier la navigation" }
                    on:click=move |_| collapsed.update(|value| *value = !*value)>
                    <Icon name=IconName::ChevronLeft size=16 class="collapse-icon" />
                    <span class="collapse-label">"Replier"</span>
                </button>
                <div class="sidebar-footer-tools">
                    <ThemePicker />
                    <button type="button" class="sidebar-logout-btn" aria-label="Se déconnecter" title="Se déconnecter"
                        on:click=move |_| leptos::task::spawn_local(logout())>
                        <Icon name=IconName::Logout size=16 />
                    </button>
                </div>
            </div>
        </aside>
    }
}

/// Indicateur des workers en ligne (central seulement), relu à chaque changement d'état des tâches.
#[component]
fn WorkersPill() -> impl IntoView {
    let reload = use_context::<crate::jobs::LiveJobs>().map(|live| live.reload);
    let workers = LocalResource::new(move || {
        if let Some(reload) = reload {
            reload.track();
        }
        crate::api::get::<Vec<Worker>>("/api/v1/workers")
    });
    move || {
        let list = workers.get().and_then(Result::ok)?;
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
            <div class=class aria-live="polite" title="Workers actifs (central inclus)">
                <span class="status-dot"></span>
                <span>{text}</span>
            </div>
        })
    }
}

#[component]
fn Topbar(me: Me) -> impl IntoView {
    let router = use_router();
    let search = use_context::<Search>().unwrap_or_else(Search::install);
    // Changer d'écran vide la recherche.
    Effect::new(move |_| {
        router.current();
        search.0.set(String::new());
    });
    let central = me.instance_kind == InstanceKind::Central && me.allows(Permission::ViewWorkers);
    view! {
        <header class="app-topbar" role="banner">
            <div class="topbar-container">
                <div class="topbar-search-anchor">
                    <div class="topbar-search" role="search">
                        <div class="search-capsule__query">
                            <Icon name=IconName::Search size=15 class="search-icon" />
                            <input type="search" class="topbar-search-input" autocomplete="off" spellcheck="false"
                                enterkeyhint="search" aria-label="Recherche contextuelle"
                                placeholder=move || search_placeholder(&router.current())
                                prop:value=move || search.0.get()
                                on:input=move |event| search.0.set(event_target_value(&event)) />
                        </div>
                    </div>
                </div>
                <div class="topbar-actions">
                    {central.then(|| view! { <WorkersPill /> })}
                </div>
            </div>
        </header>
    }
}

/// Gabarit complet : barre latérale, barre du haut et contenu.
#[component]
pub fn Shell(me: Me, children: Children) -> impl IntoView {
    Search::install();
    view! {
        <a href="#main-content" class="skip-link">"Aller au contenu principal"</a>
        <div class="application-shell">
            <div class="application-navigation-layout">
                <Sidebar me=me.clone() />
                <div class="page-transition-region">
                    <Topbar me=me />
                    <main id="main-content" class="main-content">{children()}</main>
                </div>
            </div>
        </div>
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
}
