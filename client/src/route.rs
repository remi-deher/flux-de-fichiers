//! Routes du client sous `/app/` : chemin ↔ écran, et navigation sans rechargement.

use leptos::prelude::*;
use wasm_bindgen::{JsCast, JsValue, closure::Closure};

use flux_shared::app;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Route {
    Overview,
    Jobs,
    Flows,
    NewFlow,
    FlowEditor(String),
    Libraries,
    Workers,
    Plugins,
    Settings,
}

impl Route {
    /// Chemin complet sous `/app`.
    pub fn path(&self) -> String {
        match self {
            Self::Overview => app::OVERVIEW.to_owned(),
            Self::Jobs => app::JOBS.to_owned(),
            Self::Flows => app::FLOWS.to_owned(),
            Self::NewFlow => app::NEW_FLOW.to_owned(),
            Self::FlowEditor(id) => format!("{}/{}", app::FLOWS, encode_segment(id)),
            Self::Libraries => app::LIBRARIES.to_owned(),
            Self::Workers => app::WORKERS.to_owned(),
            Self::Plugins => app::PLUGINS.to_owned(),
            Self::Settings => app::SETTINGS.to_owned(),
        }
    }

    /// Écran d'un chemin ; un chemin inconnu mène à la vue d'ensemble.
    pub fn parse(path: &str) -> Self {
        let rest = path
            .strip_prefix(app::LEGACY_PREFIX)
            .unwrap_or(path)
            .trim_matches('/');
        let mut parts = rest.split('/');
        match (parts.next(), parts.next(), parts.next()) {
            (Some("jobs"), None, _) => Self::Jobs,
            (Some("flows"), None, _) => Self::Flows,
            (Some("flows"), Some("new"), None) => Self::NewFlow,
            (Some("flows"), Some(id), None) if !id.is_empty() => {
                Self::FlowEditor(decode_segment(id))
            }
            (Some("libraries"), None, _) => Self::Libraries,
            (Some("workers"), None, _) => Self::Workers,
            (Some("plugins"), None, _) => Self::Plugins,
            (Some("settings"), None, _) => Self::Settings,
            _ => Self::Overview,
        }
    }

    /// Entrée de navigation à laquelle l'écran appartient.
    pub fn section(&self) -> Self {
        match self {
            Self::FlowEditor(_) => Self::Flows,
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

/// Question posée avant d'abandonner des modifications non enregistrées.
const LEAVE_PROMPT: &str =
    "Des modifications ne sont pas enregistrées. Quitter cet écran sans les enregistrer ?";

/// Faut-il demander confirmation avant de passer de `current` à `target` ?
pub fn needs_leave_confirmation(unsaved: bool, current: &Route, target: &Route) -> bool {
    unsaved && current != target
}

/// Écran courant, partagé par contexte.
#[derive(Clone, Copy)]
pub struct Router {
    current: RwSignal<Route>,
    /// Vrai tant que l'écran affiché porte des modifications non enregistrées.
    unsaved: RwSignal<bool>,
}

impl Router {
    /// Lit l'adresse du navigateur et suit les retours arrière.
    pub fn install() -> Self {
        let current = RwSignal::new(Route::parse(
            &window().location().pathname().unwrap_or_default(),
        ));
        let unsaved = RwSignal::new(false);
        let on_pop = Closure::<dyn FnMut()>::new(move || {
            let target = Route::parse(&window().location().pathname().unwrap_or_default());
            let shown = current.get_untracked();
            if needs_leave_confirmation(unsaved.get_untracked(), &shown, &target)
                && !confirm_leaving()
            {
                // Le navigateur a déjà changé d'adresse : on remet celle de l'écran gardé.
                let _ = window().history().and_then(|history| {
                    history.push_state_with_url(&JsValue::NULL, "", Some(&shown.path()))
                });
                return;
            }
            current.set(target);
        });
        window().set_onpopstate(Some(on_pop.as_ref().unchecked_ref()));
        on_pop.forget();
        let on_unload = Closure::<dyn FnMut(web_sys::BeforeUnloadEvent)>::new(
            move |event: web_sys::BeforeUnloadEvent| {
                if unsaved.get_untracked() {
                    event.prevent_default();
                    event.set_return_value(LEAVE_PROMPT);
                }
            },
        );
        window().set_onbeforeunload(Some(on_unload.as_ref().unchecked_ref()));
        on_unload.forget();
        let router = Self { current, unsaved };
        provide_context(router);
        router
    }

    pub fn current(&self) -> Route {
        self.current.get()
    }

    /// Déclare (ou lève) la garde : les changements d'écran demandent alors confirmation.
    pub fn set_unsaved(&self, unsaved: bool) {
        self.unsaved.set(unsaved);
    }

    pub fn go(&self, route: Route) {
        if needs_leave_confirmation(
            self.unsaved.get_untracked(),
            &self.current.get_untracked(),
            &route,
        ) && !confirm_leaving()
        {
            return;
        }
        let _ = window().history().and_then(|history| {
            history.push_state_with_url(&JsValue::NULL, "", Some(&route.path()))
        });
        self.current.set(route);
    }
}

fn confirm_leaving() -> bool {
    window().confirm_with_message(LEAVE_PROMPT).unwrap_or(true)
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
            Route::Overview,
            Route::Jobs,
            Route::Flows,
            Route::NewFlow,
            Route::FlowEditor("flow.films-abc".into()),
            Route::FlowEditor("flux à part".into()),
            Route::Libraries,
            Route::Workers,
            Route::Plugins,
            Route::Settings,
        ] {
            assert_eq!(Route::parse(&route.path()), route);
        }
    }

    #[test]
    fn only_a_real_change_of_screen_with_unsaved_edits_asks_to_confirm() {
        let editor = Route::FlowEditor("a".into());
        assert!(needs_leave_confirmation(true, &editor, &Route::Flows));
        assert!(needs_leave_confirmation(
            true,
            &editor,
            &Route::FlowEditor("b".into())
        ));
        assert!(!needs_leave_confirmation(true, &editor, &editor));
        assert!(!needs_leave_confirmation(false, &editor, &Route::Flows));
    }

    #[test]
    fn unknown_paths_lead_to_the_overview_and_old_app_links_still_work() {
        assert_eq!(Route::parse("/"), Route::Overview);
        assert_eq!(Route::parse("/inconnu"), Route::Overview);
        assert_eq!(Route::parse("/flows/a/b"), Route::Overview);
        assert_eq!(Route::parse("/app/flows"), Route::Flows);
        assert_eq!(Route::parse("/app"), Route::Overview);
        assert_eq!(Route::FlowEditor("x".into()).section(), Route::Flows);
        assert_eq!(Route::NewFlow.section(), Route::NewFlow);
    }
}
