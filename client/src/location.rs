//! Adresse complète d'un écran (ADR 0033, lot 2) : écran, onglet en second segment du chemin, et
//! paramètres de requête (`?voir=flux.3f2a&onglet=versions`). Tout se lit et s'écrit ici, sans le navigateur.

use crate::route::{Route, decode_segment, encode_segment};

/// Écrans dont le second segment du chemin est un onglet (`/plugins/installed`).
fn has_tabs(route: &Route) -> bool {
    matches!(
        route,
        Route::Jobs | Route::Libraries | Route::Plugins | Route::Settings
    )
}

/// Paramètres de requête, dans l'ordre d'écriture.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Query(Vec<(String, String)>);

impl Query {
    /// Lit `?a=1&b=2` (le point d'interrogation est facultatif) ; un paramètre sans valeur est ignoré.
    pub fn parse(search: &str) -> Self {
        let pairs = search
            .trim_start_matches('?')
            .split('&')
            .filter_map(|pair| pair.split_once('='))
            .filter(|(key, value)| !key.is_empty() && !value.is_empty())
            .map(|(key, value)| (decode_segment(key), decode_segment(value)))
            .collect();
        Self(pairs)
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.0
            .iter()
            .find(|(candidate, _)| candidate == key)
            .map(|(_, value)| value.as_str())
    }

    /// Même requête avec `key` posé à `value`, ou retiré si `value` est `None`.
    pub fn with(&self, key: &str, value: Option<&str>) -> Self {
        let mut pairs: Vec<(String, String)> = self
            .0
            .iter()
            .filter(|(candidate, _)| candidate != key)
            .cloned()
            .collect();
        if let Some(value) = value {
            pairs.push((key.to_owned(), value.to_owned()));
        }
        Self(pairs)
    }

    /// `?a=1&b=2`, ou le texte vide s'il n'y a aucun paramètre.
    pub fn render(&self) -> String {
        if self.0.is_empty() {
            return String::new();
        }
        let pairs: Vec<String> = self
            .0
            .iter()
            .map(|(key, value)| format!("{}={}", encode_segment(key), encode_segment(value)))
            .collect();
        format!("?{}", pairs.join("&"))
    }
}

/// Écran, onglet et requête : ce que l'adresse du navigateur désigne.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Location {
    pub route: Route,
    /// Onglet de l'écran ; `None` désigne l'onglet par défaut.
    pub tab: Option<String>,
    pub query: Query,
}

impl Location {
    /// Écran seul : onglet par défaut, aucun tiroir.
    pub fn of(route: Route) -> Self {
        Self {
            route,
            tab: None,
            query: Query::default(),
        }
    }

    pub fn with_tab(route: Route, tab: &str) -> Self {
        Self {
            tab: Some(tab.to_owned()),
            ..Self::of(route)
        }
    }

    /// Lit un chemin (`/app/plugins/installed`) et sa requête (`?voir=…`).
    pub fn parse(path: &str, search: &str) -> Self {
        let route = Route::parse(path);
        let tab = has_tabs(&route)
            .then(|| second_segment(path))
            .flatten()
            .map(|segment| decode_segment(&segment));
        Self {
            route,
            tab,
            query: Query::parse(search),
        }
    }

    /// Chemin et requête à écrire dans la barre d'adresse.
    pub fn url(&self) -> String {
        let path = self.route.path();
        let tab = match &self.tab {
            Some(tab) if has_tabs(&self.route) => format!("/{}", encode_segment(tab)),
            _ => String::new(),
        };
        format!("{path}{tab}{}", self.query.render())
    }

    pub fn with_query(&self, key: &str, value: Option<&str>) -> Self {
        Self {
            query: self.query.with(key, value),
            ..self.clone()
        }
    }

    pub fn with_tab_value(&self, tab: Option<&str>) -> Self {
        Self {
            tab: tab.map(str::to_owned),
            query: Query::default(),
            ..self.clone()
        }
    }
}

/// Deuxième segment du chemin, hors préfixe `/app` (le premier est l'écran).
fn second_segment(path: &str) -> Option<String> {
    let rest = path
        .strip_prefix(forgearr_shared::app::LEGACY_PREFIX)
        .unwrap_or(path)
        .trim_matches('/');
    rest.split('/')
        .nth(1)
        .filter(|segment| !segment.is_empty())
        .map(str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_query_reads_writes_and_removes_parameters() {
        let query = Query::parse("?voir=flux.3f2a&onglet=versions");
        assert_eq!(query.get("voir"), Some("flux.3f2a"));
        assert_eq!(query.get("onglet"), Some("versions"));
        assert_eq!(query.get("absent"), None);
        let swapped = query.with("voir", Some("worker.9"));
        assert_eq!(swapped.render(), "?onglet=versions&voir=worker.9");
        assert_eq!(swapped.with("onglet", None).with("voir", None).render(), "");
    }

    #[test]
    fn a_query_survives_special_characters_and_ignores_empty_parameters() {
        let query = Query::default().with("voir", Some("flux.a b&c=d"));
        assert_eq!(Query::parse(&query.render()), query);
        assert_eq!(Query::parse("?voir=&=x&seul").render(), "");
        assert_eq!(Query::parse("").render(), "");
    }

    #[test]
    fn a_location_round_trips_through_its_url() {
        for location in [
            Location::of(Route::Overview),
            Location::of(Route::Flows),
            Location::with_tab(Route::Plugins, "installed"),
            Location::with_tab(Route::Libraries, "scripts"),
            Location::with_tab(Route::Jobs, "logs").with_query("voir", Some("job.1")),
            Location::of(Route::Workers).with_query("nouveau", Some("worker")),
            Location::of(Route::FlowEditor("flux à part".into())),
        ] {
            let url = location.url();
            let (path, search) = url.split_once('?').unwrap_or((url.as_str(), ""));
            assert_eq!(Location::parse(path, search), location, "{url}");
        }
    }

    #[test]
    fn the_tab_is_the_second_segment_of_the_tabbed_screens_only() {
        let plugins = Location::parse("/app/plugins/repos", "");
        assert_eq!(plugins.route, Route::Plugins);
        assert_eq!(plugins.tab.as_deref(), Some("repos"));
        assert_eq!(Location::parse("/plugins", "").tab, None);
        assert_eq!(Location::parse("/libraries/scripts", "").tab.as_deref(), Some("scripts"));
        assert_eq!(Location::parse("/settings/keys", "").tab.as_deref(), Some("keys"));
        assert_eq!(Location::parse("/jobs/logs", "").tab.as_deref(), Some("logs"));
        // Les écrans sans onglet n'en lisent pas, et un chemin trop long reste inconnu.
        assert_eq!(Location::parse("/workers", "").tab, None);
        assert_eq!(Location::parse("/plugins/repos/x", "").route, Route::Overview);
    }

    #[test]
    fn changing_the_tab_closes_any_drawer() {
        let open = Location::with_tab(Route::Plugins, "installed").with_query("voir", Some("a.1"));
        let other = open.with_tab_value(Some("repos"));
        assert_eq!(other.url(), "/plugins/repos");
        assert_eq!(open.with_tab_value(None).url(), "/plugins");
    }
}
