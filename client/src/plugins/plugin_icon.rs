//! Icône SVG d'un plugin (ADR 0024), affichée comme masque d'image : elle prend la couleur du
//! texte du thème et n'est jamais chargée comme document.

use crate::route::encode_segment;
use leptos::prelude::*;

/// Chemin de l'icône d'un plugin du catalogue, servie par le central.
pub fn plugin_icon_url(plugin_id: &str) -> String {
    format!("/api/v1/plugins/{}/icon", encode_segment(plugin_id))
}

#[component]
pub fn PluginIcon(#[prop(into)] plugin_id: String) -> impl IntoView {
    let url = plugin_icon_url(&plugin_id);
    let style = format!("--plugin-icon-url: url('{url}')");
    view! { <span class="plugin-icon" style=style></span> }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn icon_url_targets_the_plugin_route_and_encodes_the_identifier() {
        let plain = plugin_icon_url("plex");
        assert_eq!(plain, "/api/v1/plugins/plex/icon");
        let dotted = plugin_icon_url("a.b-c_d");
        assert_eq!(dotted, "/api/v1/plugins/a.b-c_d/icon");
        let quoted = plugin_icon_url("a b'c");
        assert_eq!(quoted, "/api/v1/plugins/a%20b%27c/icon");
    }
}
