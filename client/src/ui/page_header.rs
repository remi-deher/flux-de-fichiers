//! Résolution du fil d'Ariane par route pour la barre supérieure.

use crate::route::Route;

/// Paire `(groupe, écran)` affichée dans le fil d'Ariane de la barre supérieure.
pub fn breadcrumb_for(route: &Route) -> (&'static str, &'static str) {
    match route {
        Route::Overview => ("Vue générale", "Vue d'ensemble"),
        Route::Libraries => ("Médias", "Bibliothèques"),
        Route::Flows => ("Traitements", "Flows"),
        Route::NewFlow => ("Traitements", "Nouveau flux"),
        Route::FlowEditor(_) => ("Traitements", "Atelier de flux"),
        Route::Jobs => ("Traitements", "Activité"),
        Route::Workers => ("Infrastructure", "Workers"),
        Route::Plugins => ("Infrastructure", "Plugins"),
        Route::Settings => ("Système", "Paramètres"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_route_has_a_non_empty_breadcrumb_pair() {
        let routes = [
            Route::Overview,
            Route::Libraries,
            Route::Flows,
            Route::NewFlow,
            Route::FlowEditor("demo".into()),
            Route::Jobs,
            Route::Workers,
            Route::Plugins,
            Route::Settings,
        ];
        for route in routes {
            let (group, screen) = breadcrumb_for(&route);
            assert!(!group.is_empty());
            assert!(!screen.is_empty());
        }
        assert_eq!(
            breadcrumb_for(&Route::FlowEditor("abc".into())),
            ("Traitements", "Atelier de flux")
        );
    }
}
