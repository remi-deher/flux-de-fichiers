//! Routes du client sous `/app/` : chemin ↔ écran, et navigation sans rechargement.

use leptos::prelude::*;
use wasm_bindgen::{JsCast, JsValue, closure::Closure};

use flux_shared::app;

/// Préfixe servi par le central pour le client (ADR 0014).
pub const BASE: &str = app::BASE;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Route {
    Jobs,
    Flows,
    NewFlow,
    FlowEditor(String),
    Libraries,
    Workers,
}

impl Route {
    /// Chemin complet sous `/app`.
    pub fn path(&self) -> String {
        match self {
            Self::Jobs => app::JOBS.to_owned(),
            Self::Flows => app::FLOWS.to_owned(),
            Self::NewFlow => app::NEW_FLOW.to_owned(),
            Self::FlowEditor(id) => format!("{}/{}", app::FLOWS, encode_segment(id)),
            Self::Libraries => app::LIBRARIES.to_owned(),
            Self::Workers => app::WORKERS.to_owned(),
        }
    }

    /// Écran d'un chemin ; un chemin inconnu mène aux tâches.
    pub fn parse(path: &str) -> Self {
        let rest = path.strip_prefix(BASE).unwrap_or(path).trim_matches('/');
        let mut parts = rest.split('/');
        match (parts.next(), parts.next(), parts.next()) {
            (Some("flows"), None, _) => Self::Flows,
            (Some("flows"), Some("new"), None) => Self::NewFlow,
            (Some("flows"), Some(id), None) if !id.is_empty() => {
                Self::FlowEditor(decode_segment(id))
            }
            (Some("libraries"), None, _) => Self::Libraries,
            (Some("workers"), None, _) => Self::Workers,
            _ => Self::Jobs,
        }
    }

    /// Entrée de navigation à laquelle l'écran appartient.
    pub fn section(&self) -> Self {
        match self {
            Self::NewFlow | Self::FlowEditor(_) => Self::Flows,
            other => other.clone(),
        }
    }
}

/// Encode un identifiant pour un segment de chemin (lettres, chiffres, `.-_` en clair).
pub fn encode_segment(value: &str) -> String {
    value
        .bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'.' | b'-' | b'_' => {
                (byte as char).to_string()
            }
            other => format!("%{other:02X}"),
        })
        .collect()
}

fn decode_segment(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        let decoded = (bytes[index] == b'%')
            .then(|| value.get(index + 1..index + 3))
            .flatten()
            .and_then(|hex| u8::from_str_radix(hex, 16).ok());
        match decoded {
            Some(byte) => {
                out.push(byte);
                index += 3;
            }
            None => {
                out.push(bytes[index]);
                index += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Écran courant, partagé par contexte.
#[derive(Clone, Copy)]
pub struct Router {
    current: RwSignal<Route>,
}

impl Router {
    /// Lit l'adresse du navigateur et suit les retours arrière.
    pub fn install() -> Self {
        let current = RwSignal::new(Route::parse(
            &window().location().pathname().unwrap_or_default(),
        ));
        let on_pop = Closure::<dyn FnMut()>::new(move || {
            current.set(Route::parse(
                &window().location().pathname().unwrap_or_default(),
            ));
        });
        window().set_onpopstate(Some(on_pop.as_ref().unchecked_ref()));
        on_pop.forget();
        let router = Self { current };
        provide_context(router);
        router
    }

    pub fn current(&self) -> Route {
        self.current.get()
    }

    pub fn go(&self, route: Route) {
        let _ = window().history().and_then(|history| {
            history.push_state_with_url(&JsValue::NULL, "", Some(&route.path()))
        });
        self.current.set(route);
    }
}

pub fn use_router() -> Router {
    use_context::<Router>().unwrap_or_else(Router::install)
}

/// Lien interne : change d'écran sans recharger la page.
#[component]
pub fn Link(to: Route, #[prop(optional)] class: &'static str, children: Children) -> impl IntoView {
    let router = use_router();
    let href = to.path();
    view! {
        <a
            href=href
            class=class
            on:click=move |event| {
                event.prevent_default();
                router.go(to.clone());
            }
        >
            {children()}
        </a>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_route_parses_back_from_its_path() {
        for route in [
            Route::Jobs,
            Route::Flows,
            Route::NewFlow,
            Route::FlowEditor("flow.films-abc".into()),
            Route::FlowEditor("flux à part".into()),
            Route::Libraries,
            Route::Workers,
        ] {
            assert_eq!(Route::parse(&route.path()), route);
        }
    }

    #[test]
    fn unknown_paths_lead_to_jobs_and_sections_group_the_editor() {
        assert_eq!(Route::parse("/app"), Route::Jobs);
        assert_eq!(Route::parse("/app/inconnu"), Route::Jobs);
        assert_eq!(Route::parse("/app/flows/a/b"), Route::Jobs);
        assert_eq!(Route::FlowEditor("x".into()).section(), Route::Flows);
        assert_eq!(Route::NewFlow.section(), Route::Flows);
    }
}
