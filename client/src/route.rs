//! Routes du client sous `/app/` : chemin ↔ écran, et navigation sans rechargement.

use leptos::prelude::*;
use wasm_bindgen::{JsCast, JsValue, closure::Closure};

use crate::flows::FlowsTab;
use crate::location::{Location, Query};
use crate::url_state::UrlTab;
use forgearr_shared::app;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Route {
    Overview,
    Jobs,
    Flows,
    NewFlow,
    FlowEditor(String),
    Libraries,
    NewScript,
    ScriptEditor(String),
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
            Self::NewScript => app::NEW_SCRIPT.to_owned(),
            Self::ScriptEditor(id) => format!("{}/{}", app::SCRIPTS, encode_segment(id)),
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
        let segments = (parts.next(), parts.next(), parts.next());
        // Un segment de trop (`/libraries/scripts/a/b`) n'est pas un écran connu.
        let extra = parts.next().is_some();
        match segments {
            (Some("jobs"), None, _) | (Some("jobs"), Some(_), None) => Self::Jobs,
            (Some("flows"), None, _) => Self::Flows,
            (Some("flows"), Some("new"), None) => Self::NewFlow,
            // Les onglets de l'écran Flux passent avant l'identifiant d'un flux.
            (Some("flows"), Some(tab), None) if FlowsTab::from_slug(tab).is_some() => Self::Flows,
            (Some("flows"), Some(id), None) if !id.is_empty() => {
                Self::FlowEditor(decode_segment(id))
            }
            (Some("libraries"), None, _) => Self::Libraries,
            (Some("libraries"), Some(_), None) => Self::Libraries,
            (Some("flows"), Some("scripts"), Some("new")) if !extra => Self::NewScript,
            (Some("flows"), Some("scripts"), Some(id)) if !id.is_empty() && !extra => {
                Self::ScriptEditor(decode_segment(id))
            }
            // Anciennes adresses de l'éditeur (`/libraries/scripts/…`), avant le lot 4.
            (Some("libraries"), Some("scripts"), Some("new")) if !extra => Self::NewScript,
            (Some("libraries"), Some("scripts"), Some(id)) if !id.is_empty() && !extra => {
                Self::ScriptEditor(decode_segment(id))
            }
            (Some("workers"), None, _) => Self::Workers,
            (Some("plugins"), None, _) | (Some("plugins"), Some(_), None) => Self::Plugins,
            (Some("settings"), None, _) | (Some("settings"), Some(_), None) => Self::Settings,
            _ => Self::Overview,
        }
    }

    /// Entrée de navigation à laquelle l'écran appartient.
    pub fn section(&self) -> Self {
        match self {
            Self::FlowEditor(_) => Self::Flows,
            Self::NewScript | Self::ScriptEditor(_) => Self::Flows,
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

pub(crate) fn decode_segment(value: &str) -> String {
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
    /// Onglet de l'écran (`None` : l'onglet par défaut) et paramètres de requête (tiroir ouvert…).
    tab: RwSignal<Option<String>>,
    query: RwSignal<Query>,
    /// Vrai tant que l'écran affiché porte des modifications non enregistrées.
    unsaved: RwSignal<bool>,
}

impl Router {
    /// Lit l'adresse du navigateur et suit les retours arrière.
    pub fn install() -> Self {
        let location = browser_location();
        let current = RwSignal::new(location.route);
        let tab = RwSignal::new(location.tab);
        let query = RwSignal::new(location.query);
        let unsaved = RwSignal::new(false);
        let on_pop = Closure::<dyn FnMut()>::new(move || {
            let location = browser_location();
            let target = location.route.clone();
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
            tab.set(location.tab);
            query.set(location.query);
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
        let router = Self {
            current,
            tab,
            query,
            unsaved,
        };
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
        self.go_to(Location::of(route));
    }

    /// Ouvre un écran sur l'onglet `tab`.
    pub fn go_tab(&self, route: Route, tab: &str) {
        self.go_to(Location::with_tab(route, tab));
    }

    /// Ouvre l'adresse `location` (écran, onglet et requête), avec la garde de sortie.
    pub fn go_to(&self, location: Location) {
        if needs_leave_confirmation(
            self.unsaved.get_untracked(),
            &self.current.get_untracked(),
            &location.route,
        ) && !confirm_leaving()
        {
            return;
        }
        self.write_history(&location, HistoryWrite::Push);
        self.show(location);
    }

    /// Onglet courant de l'écran, `None` pour l'onglet par défaut.
    pub fn tab(&self) -> Option<String> {
        self.tab.get()
    }

    pub fn tab_untracked(&self) -> Option<String> {
        self.tab.get_untracked()
    }

    /// Paramètres de requête courants.
    pub fn query(&self) -> Query {
        self.query.get()
    }

    pub fn query_untracked(&self) -> Query {
        self.query.get_untracked()
    }

    fn location(&self) -> Location {
        Location {
            route: self.current.get_untracked(),
            tab: self.tab.get_untracked(),
            query: self.query.get_untracked(),
        }
    }

    fn show(&self, location: Location) {
        self.tab.set(location.tab);
        self.query.set(location.query);
        self.current.set(location.route);
    }

    /// Change d'onglet sur place : un onglet de plus dans l'historique, sans tiroir ouvert.
    pub fn set_tab(&self, tab: Option<&str>) {
        let current = self.location();
        if current.tab.as_deref() == tab {
            return;
        }
        let next = current.with_tab_value(tab);
        self.write_history(&next, HistoryWrite::Push);
        self.show(next);
    }

    /// Pose ou retire un paramètre de requête (tiroir ouvert, onglet du tiroir).
    ///
    /// Ouvrir un tiroir depuis une adresse sans requête pousse une entrée d'historique : Retour ferme
    /// le tiroir. Tout autre changement (fermer, passer d'un tiroir à l'autre) remplace l'adresse.
    pub fn set_param(&self, key: &str, value: Option<&str>) {
        let current = self.location();
        if current.query.get(key) == value {
            return;
        }
        let next = current.with_query(key, value);
        let opening = current.query.render().is_empty() && value.is_some();
        let write = if opening {
            HistoryWrite::Push
        } else {
            HistoryWrite::Replace
        };
        self.write_history(&next, write);
        self.show(next);
    }

    fn write_history(&self, location: &Location, write: HistoryWrite) {
        let url = location.url();
        let _ = window().history().and_then(|history| match write {
            HistoryWrite::Push => history.push_state_with_url(&JsValue::NULL, "", Some(&url)),
            HistoryWrite::Replace => history.replace_state_with_url(&JsValue::NULL, "", Some(&url)),
        });
    }
}

/// Façon d'écrire une adresse dans l'historique du navigateur.
#[derive(Clone, Copy)]
enum HistoryWrite {
    Push,
    Replace,
}

/// Adresse actuellement affichée par le navigateur.
fn browser_location() -> Location {
    let location = window().location();
    Location::parse(
        &location.pathname().unwrap_or_default(),
        &location.search().unwrap_or_default(),
    )
}

fn confirm_leaving() -> bool {
    window().confirm_with_message(LEAVE_PROMPT).unwrap_or(true)
}

pub fn use_router() -> Router {
    use_context::<Router>().unwrap_or_else(Router::install)
}

/// Bouton principal de la souris (`MouseEvent.button`).
pub const PRIMARY_BUTTON: i16 = 0;

/// Un clic simple reste dans l'application ; Ctrl, Cmd, Maj, Alt ou un autre bouton laissent le
/// navigateur ouvrir le lien (nouvel onglet, nouvelle fenêtre, téléchargement).
pub fn is_plain_click(button: i16, ctrl: bool, meta: bool, shift: bool, alt: bool) -> bool {
    button == PRIMARY_BUTTON && !(ctrl || meta || shift || alt)
}

/// Lien interne : change d'écran sans recharger la page. `current` pose `aria-current="page"`
/// sur l'entrée de l'écran affiché (WAI-ARIA, navigation par liens).
#[component]
pub fn Link(
    to: Route,
    #[prop(optional)] class: &'static str,
    #[prop(optional, into)] current: Option<Signal<bool>>,
    #[prop(optional)] title: Option<&'static str>,
    children: Children,
) -> impl IntoView {
    let router = use_router();
    let href = to.path();
    view! {
        <a
            href=href
            class=class
            title=title
            aria-current=move || current.is_some_and(|current| current.get()).then_some("page")
            on:click=move |event| {
                if is_plain_click(
                    event.button(),
                    event.ctrl_key(),
                    event.meta_key(),
                    event.shift_key(),
                    event.alt_key(),
                ) {
                    event.prevent_default();
                    router.go(to.clone());
                }
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
            Route::NewScript,
            Route::ScriptEditor("script.mon-script".into()),
            Route::ScriptEditor("script à part".into()),
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
    fn only_a_plain_primary_click_stays_in_the_application() {
        assert!(is_plain_click(0, false, false, false, false));
        assert!(!is_plain_click(1, false, false, false, false));
        assert!(!is_plain_click(0, true, false, false, false));
        assert!(!is_plain_click(0, false, true, false, false));
        assert!(!is_plain_click(0, false, false, true, false));
        assert!(!is_plain_click(0, false, false, false, true));
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

    #[test]
    fn the_flows_screen_has_tabs_that_come_before_flow_identifiers() {
        assert_eq!(Route::parse("/app/flows/modeles"), Route::Flows);
        assert_eq!(Route::parse("/flows/scripts"), Route::Flows);
        assert_eq!(Route::parse("/flows/flux"), Route::Flows);
        assert_eq!(Route::parse("/flows/new"), Route::NewFlow);
        assert_eq!(
            Route::parse("/flows/mon-flux.a1"),
            Route::FlowEditor("mon-flux.a1".into())
        );
        assert_eq!(Route::parse("/flows/scripts/x"), Route::Overview);
    }

    #[test]
    fn script_editor_routes_belong_to_the_flows_section() {
        assert_eq!(Route::NewScript.section(), Route::Flows);
        assert_eq!(Route::ScriptEditor("s".into()).section(), Route::Flows);
        assert_eq!(Route::parse("/flows/scripts/new"), Route::NewScript);
        assert_eq!(Route::NewScript.path(), "/flows/scripts/new");
        assert_eq!(Route::ScriptEditor("s.1".into()).path(), "/flows/scripts/s.1");
        assert_eq!(
            Route::parse("/flows/scripts/script.a"),
            Route::ScriptEditor("script.a".into())
        );
        assert_eq!(Route::parse("/flows/scripts/a/b"), Route::Overview);
        // Les anciennes adresses restent lues.
        assert_eq!(Route::parse("/libraries/scripts/new"), Route::NewScript);
        assert_eq!(
            Route::parse("/app/libraries/scripts/script.a"),
            Route::ScriptEditor("script.a".into())
        );
        assert_eq!(Route::parse("/libraries/scripts"), Route::Libraries);
        assert_eq!(Route::parse("/libraries/scripts/a/b"), Route::Overview);
        assert_eq!(Route::parse("/libraries/scripts/new/x"), Route::Overview);
    }

    #[test]
    fn tabbed_screens_accept_one_extra_segment() {
        assert_eq!(Route::parse("/app/plugins/installed"), Route::Plugins);
        assert_eq!(Route::parse("/jobs/logs"), Route::Jobs);
        assert_eq!(Route::parse("/settings/keys"), Route::Settings);
        assert_eq!(Route::parse("/libraries/packs"), Route::Libraries);
        assert_eq!(Route::parse("/workers/x"), Route::Overview);
        assert_eq!(Route::parse("/plugins/installed/x"), Route::Overview);
    }
}
