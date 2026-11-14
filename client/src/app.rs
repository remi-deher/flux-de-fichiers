//! Racine du client : porte d'accès, coque (barre latérale, barre du haut) et écran courant.

use crate::access::AccessGate;
use crate::overview::{CentralOverview, WorkerOverviewPage};
use crate::plugins::PluginsPage;
use crate::route::{Route, Router, use_router};
use crate::settings::SettingsPage;
use crate::shell::{Shell, allowed};
use crate::{flows, jobs::JobsPage, libraries, workers};
use flux_shared::session::Me;
use flux_shared::{InstanceKind, Permission};
use leptos::prelude::*;

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
fn Session() -> impl IntoView {
    let me = expect_context::<Me>();
    // Le flux d'événements des tâches n'existe que sur le central.
    if me.instance_kind == InstanceKind::Central && me.allows(Permission::ViewJobs) {
        crate::jobs::LiveJobs::install();
    }
    view! { <Shell me=me.clone()><Screen me=me /></Shell> }
}

#[component]
pub fn App() -> impl IntoView {
    Router::install();
    crate::theme::restore();
    view! { <AccessGate><Session /></AccessGate> }
}
