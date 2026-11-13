//! Racine du client : session, identité, puis l'écran demandé.

use crate::{api, jobs::JobsPage};
use flux_shared::InstanceKind;
use flux_shared::session::{Me, SessionInfo};
use leptos::prelude::*;

const SESSION_PATH: &str = "/api/v1/auth/session";
const ME_PATH: &str = "/api/v1/me";
/// Interface historique, où l'on se connecte tant que le client n'a pas son écran d'accès.
const LEGACY_PATH: &str = "/";

/// Ouvre la session (jeton CSRF) puis lit l'identité de l'appelant.
async fn load_identity() -> Result<Me, api::ApiError> {
    let session = api::get::<SessionInfo>(SESSION_PATH).await?;
    api::set_csrf_token(&session.csrf_token);
    api::get::<Me>(ME_PATH).await
}

#[component]
pub fn App() -> impl IntoView {
    let identity = LocalResource::new(load_identity);
    view! {
        <div class="main-content">
            <header class="cluster-spread">
                <div class="brand">
                    <span class="brand-name">"Flux de Fichiers"</span>
                    <span class="hero-badge">"Nouvelle interface"</span>
                </div>
                <a class="btn btn-secondary btn-sm" href=LEGACY_PATH>"Interface complète"</a>
            </header>
            <main>
                {move || match identity.get() {
                    None => view! { <p class="text-dim">"Chargement de la session…"</p> }.into_any(),
                    Some(Ok(me)) if me.instance_kind == InstanceKind::Worker => view! {
                        <div class="empty-state-panel">
                            <p class="empty-state-title">"Sur un worker, les tâches se suivent dans l'interface complète."</p>
                            <a class="btn btn-primary" href=LEGACY_PATH>"Ouvrir l'interface complète"</a>
                        </div>
                    }.into_any(),
                    Some(Ok(me)) => view! { <JobsPage me=me /> }.into_any(),
                    Some(Err(error)) => view! {
                        <div class="empty-state-panel">
                            <p class="empty-state-title">{error.message()}</p>
                            <a class="btn btn-primary" href=LEGACY_PATH>"Se connecter"</a>
                        </div>
                    }.into_any(),
                }}
            </main>
        </div>
    }
}
