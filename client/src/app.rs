//! Racine du client : session, identité, navigation et écran courant.

use crate::route::{Link, Route, Router};
use crate::{api, flows, jobs::JobsPage, libraries, workers};
use flux_shared::session::{Me, SessionInfo};
use flux_shared::{InstanceKind, Permission};
use leptos::prelude::*;

const SESSION_PATH: &str = "/api/v1/auth/session";
const ME_PATH: &str = "/api/v1/me";
/// Interface historique, où l'on se connecte tant que le client n'a pas son écran d'accès.
const LEGACY_PATH: &str = "/";

/// Entrées de navigation et droit exigé pour chacune.
const NAVIGATION: [(Route, &str, Permission); 4] = [
    (Route::Jobs, "Tâches", Permission::ViewJobs),
    (Route::Flows, "Flux", Permission::ViewFlows),
    (Route::Libraries, "Bibliothèques", Permission::ViewLibraries),
    (Route::Workers, "Workers", Permission::ViewWorkers),
];

/// Ouvre la session (jeton CSRF) puis lit l'identité de l'appelant.
async fn load_identity() -> Result<Me, api::ApiError> {
    let session = api::get::<SessionInfo>(SESSION_PATH).await?;
    api::set_csrf_token(&session.csrf_token);
    api::get::<Me>(ME_PATH).await
}

fn notice(message: String, action: &'static str) -> AnyView {
    view! {
        <div class="empty-state-panel">
            <p class="empty-state-title">{message}</p>
            <a class="btn btn-primary" href=LEGACY_PATH>{action}</a>
        </div>
    }
    .into_any()
}

#[component]
fn Navigation(me: Me) -> impl IntoView {
    let router = crate::route::use_router();
    view! {
        <nav class="theme-segmented" aria-label="Écrans">
            {NAVIGATION
                .into_iter()
                .filter(|(_, _, permission)| me.allows(*permission))
                .map(|(route, label, _)| {
                    let section = route.clone();
                    view! {
                        <span aria-current=move || (router.current().section() == section).then_some("page")>
                            <Link to=route class="theme-segmented__btn">{label}</Link>
                        </span>
                    }
                })
                .collect_view()}
        </nav>
    }
}

#[component]
fn Screen(me: Me) -> impl IntoView {
    let router = crate::route::use_router();
    move || {
        let me = me.clone();
        match router.current() {
            Route::Jobs => view! { <JobsPage me=me /> }.into_any(),
            Route::Flows => view! { <flows::FlowsPage me=me /> }.into_any(),
            Route::NewFlow => view! { <flows::FlowEditor me=me flow_id=None /> }.into_any(),
            Route::FlowEditor(id) => {
                view! { <flows::FlowEditor me=me flow_id=Some(id) /> }.into_any()
            }
            Route::Libraries => view! { <libraries::LibrariesPage me=me /> }.into_any(),
            Route::Workers => view! { <workers::WorkersPage me=me /> }.into_any(),
        }
    }
}

#[component]
pub fn App() -> impl IntoView {
    Router::install();
    let identity = LocalResource::new(load_identity);
    view! {
        <div class="main-content">
            {move || match identity.get() {
                None => view! { <p class="text-dim">"Chargement de la session…"</p> }.into_any(),
                Some(Err(error)) => notice(error.message(), "Se connecter"),
                Some(Ok(me)) if me.instance_kind == InstanceKind::Worker => notice(
                    "Sur un worker, utilisez l'interface complète.".into(),
                    "Ouvrir l'interface complète",
                ),
                Some(Ok(me)) => view! {
                    <header class="cluster-spread">
                        <div class="brand">
                            <span class="brand-name">"Flux de Fichiers"</span>
                            <span class="hero-badge">"Nouvelle interface"</span>
                        </div>
                        <Navigation me=me.clone() />
                        <a class="btn btn-secondary btn-sm" href=LEGACY_PATH>"Interface complète"</a>
                    </header>
                    <main><Screen me=me /></main>
                }.into_any(),
            }}
        </div>
    }
}
