//! Racine du client : porte d'accès, navigation selon l'installation et les droits, écran courant.

use crate::access::AccessGate;
use crate::overview::{CentralOverview, WorkerOverviewPage};
use crate::plugins::PluginsPage;
use crate::route::{Link, Route, Router, use_router};
use crate::settings::{SettingsPage, logout};
use crate::{flows, jobs::JobsPage, libraries, workers};
use flux_shared::session::Me;
use flux_shared::{InstanceKind, Permission};
use leptos::prelude::*;

/// Entrée de navigation : écran, libellé, droit exigé et installations qui l'hébergent.
struct NavEntry {
    route: Route,
    label: &'static str,
    permission: Permission,
    on_worker: bool,
}

fn navigation() -> [NavEntry; 8] {
    let entry = |route, label, permission, on_worker| NavEntry {
        route,
        label,
        permission,
        on_worker,
    };
    [
        entry(
            Route::Overview,
            "Vue d'ensemble",
            Permission::ViewOverview,
            true,
        ),
        entry(Route::Jobs, "Tâches", Permission::ViewJobs, true),
        entry(Route::Flows, "Flux", Permission::ViewFlows, false),
        entry(
            Route::Libraries,
            "Bibliothèques",
            Permission::ViewLibraries,
            false,
        ),
        entry(Route::Workers, "Workers", Permission::ViewWorkers, false),
        entry(Route::Plugins, "Plugins", Permission::ViewPlugins, true),
        entry(
            Route::Settings,
            "Paramètres",
            Permission::ViewOverview,
            true,
        ),
        entry(Route::NewFlow, "", Permission::ManageFlows, false),
    ]
}

/// L'écran est-il accessible sur cette installation et pour ces droits ?
pub fn allowed(me: &Me, route: &Route) -> bool {
    let section = route.section();
    navigation().iter().any(|entry| {
        entry.route == section
            && me.allows(entry.permission)
            && (me.instance_kind == InstanceKind::Central || entry.on_worker)
    })
}

#[component]
fn Navigation(me: Me) -> impl IntoView {
    let router = use_router();
    view! {
        <nav class="theme-segmented" aria-label="Écrans">
            {navigation()
                .into_iter()
                .filter(|entry| !entry.label.is_empty() && allowed(&me, &entry.route))
                .map(|entry| {
                    let section = entry.route.clone();
                    view! {
                        <span aria-current=move || (router.current().section() == section).then_some("page")>
                            <Link to=entry.route class="theme-segmented__btn">{entry.label}</Link>
                        </span>
                    }
                })
                .collect_view()}
        </nav>
    }
}

#[component]
fn Screen(me: Me) -> impl IntoView {
    let router = use_router();
    move || {
        let me = me.clone();
        let route = router.current();
        if !allowed(&me, &route) {
            return view! { <p class="settings-alert">"Cet écran n'est pas disponible ici, ou votre rôle ne le permet pas."</p> }.into_any();
        }
        let worker = me.instance_kind == InstanceKind::Worker;
        match route {
            Route::Overview if worker => view! { <WorkerOverviewPage me=me /> }.into_any(),
            Route::Overview => view! { <CentralOverview me=me /> }.into_any(),
            Route::Jobs => view! { <JobsPage me=me /> }.into_any(),
            Route::Flows => view! { <flows::FlowsPage me=me /> }.into_any(),
            Route::NewFlow => view! { <flows::FlowEditor me=me flow_id=None /> }.into_any(),
            Route::FlowEditor(id) => {
                view! { <flows::FlowEditor me=me flow_id=Some(id) /> }.into_any()
            }
            Route::Libraries => view! { <libraries::LibrariesPage me=me /> }.into_any(),
            Route::Workers => view! { <workers::WorkersPage me=me /> }.into_any(),
            Route::Plugins => view! { <PluginsPage me=me /> }.into_any(),
            Route::Settings => view! { <SettingsPage me=me /> }.into_any(),
        }
    }
}

#[component]
fn Shell() -> impl IntoView {
    let me = expect_context::<Me>();
    let instance = me.instance_name.clone().unwrap_or_default();
    // Le flux d'événements des tâches n'existe que sur le central.
    if me.instance_kind == InstanceKind::Central && me.allows(Permission::ViewJobs) {
        crate::jobs::LiveJobs::install();
    }
    view! {
        <header class="cluster-spread">
            <div class="brand">
                <span class="brand-name">"Flux de Fichiers"</span>
                <span class="brand-version">{instance}</span>
            </div>
            <Navigation me=me.clone() />
            <button type="button" class="btn btn-secondary btn-sm"
                on:click=move |_| leptos::task::spawn_local(logout())>"Se déconnecter"</button>
        </header>
        <main><Screen me=me /></main>
    }
}

#[component]
pub fn App() -> impl IntoView {
    Router::install();
    crate::theme::restore();
    view! {
        <div class="main-content">
            <AccessGate><Shell /></AccessGate>
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
        let central = me(Role::Admin, InstanceKind::Central);
        assert!(allowed(&central, &Route::FlowEditor("f".into())));
        assert!(allowed(&central, &Route::NewFlow));
    }

    #[test]
    fn a_viewer_cannot_open_the_flow_creation() {
        let viewer = me(Role::Viewer, InstanceKind::Central);
        assert!(allowed(&viewer, &Route::Flows));
        assert!(!allowed(&viewer, &Route::NewFlow));
    }
}
