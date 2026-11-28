//! D'où l'on ouvre l'éditeur de script : de l'onglet Scripts, ou d'un flux (`?depuis=<flux>`) auquel
//! l'éditeur revient à sa fermeture (ADR 0035, lot 4).

use crate::flows::FlowsTab;
use crate::location::{FROM_PARAM, Location};
use crate::route::{Route, Router};
use crate::url_state::UrlTab;

/// Adresse d'un écran de l'éditeur (`NewScript` ou `ScriptEditor`), qui garde le flux d'origine.
pub(crate) fn script_editor_location(route: Route, from: Option<&str>) -> Location {
    Location::of(route).with_query(FROM_PARAM, from)
}

/// Flux d'origine de l'éditeur affiché, lu dans l'adresse courante.
pub(super) fn origin_flow(router: &Router) -> Option<String> {
    router.query_untracked().get(FROM_PARAM).map(str::to_owned)
}

/// Où va l'éditeur en se fermant : le flux d'origine, sinon la liste des scripts.
pub(super) fn leave_location(from: Option<&str>) -> Location {
    match from {
        Some(flow) => Location::of(Route::FlowEditor(flow.to_owned())),
        None => Location::with_tab(Route::Flows, FlowsTab::Scripts.slug()),
    }
}

/// Libellé du bouton de fermeture.
pub(super) fn leave_label(from: Option<&str>) -> &'static str {
    if from.is_some() {
        "← Flux"
    } else {
        "← Scripts"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_editor_keeps_the_flow_it_was_opened_from() {
        let location = script_editor_location(Route::NewScript, Some("flux.a"));
        assert_eq!(location.url(), "/flows/scripts/new?depuis=flux.a");
        assert_eq!(location.query.get(FROM_PARAM), Some("flux.a"));
        let alone = script_editor_location(Route::ScriptEditor("s.1".into()), None);
        assert_eq!(alone.url(), "/flows/scripts/s.1");
        assert_eq!(alone.query.get(FROM_PARAM), None);
    }

    #[test]
    fn closing_the_editor_goes_back_to_the_flow_or_to_the_scripts() {
        let flow = Route::FlowEditor("flux.a".into());
        assert_eq!(leave_location(Some("flux.a")).route, flow);
        assert_eq!(leave_location(None).url(), "/flows/scripts");
        assert_eq!(leave_label(Some("flux.a")), "← Flux");
        assert_eq!(leave_label(None), "← Scripts");
    }
}
