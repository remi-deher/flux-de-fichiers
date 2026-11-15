//! En-tête unifié d'écran (`PageHeader`) et résolution du fil d'Ariane par route.

use super::icon::{Icon, IconName};
use crate::route::Route;
use leptos::prelude::*;

const HEADER_ICON_SIZE: u32 = 20;

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

/// En-tête standard d'écran : fil conducteur, icône, titre, description et actions primaires.
#[component]
pub fn PageHeader(
    eyebrow: &'static str,
    #[prop(into)] title: String,
    #[prop(into, optional)] subtitle: Option<String>,
    #[prop(optional)] icon: Option<IconName>,
    #[prop(optional)] actions: Option<AnyView>,
) -> impl IntoView {
    view! {
        <header class="page-header">
            <div class="page-header__lead">
                {icon.map(|name| view! {
                    <span class="page-header__icon" aria-hidden="true">
                        <Icon name=name size=HEADER_ICON_SIZE />
                    </span>
                })}
                <div class="page-header__copy">
                    <span class="page-header__eyebrow">{eyebrow}</span>
                    <h1 class="page-header__title">{title}</h1>
                    {subtitle.map(|sub| view! {
                        <p class="page-header__subtitle">{sub}</p>
                    })}
                </div>
            </div>
            {actions.map(|content| view! {
                <div class="page-header__actions">{content}</div>
            })}
        </header>
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
