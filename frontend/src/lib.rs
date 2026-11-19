//! Page HTML du client web (ADR 0014) : métadonnées, styles du thème et chargement du module WASM.
//!
//! Toute l'interface est le client Leptos compilé en WASM (`client/`) ; ce crate ne génère que la
//! page qui le charge, avec les styles typés du thème.

pub mod theme;

use forgearr_shared::brand::{MARK_PATH, NAME};
use theme::tokens::{DARK_THEME, LIGHT_THEME};

/// Module ES produit par `wasm-bindgen --target web` dans `dist/pkg`, chargé en chemin absolu
/// pour que toute route du client (`/flows/…`) le retrouve.
pub const APP_MODULE: &str = "/pkg/flux_client.js";

/// Document HTML commun : métadonnées W3C et styles du thème, autour d'un corps donné.
fn html_document(body: &str) -> String {
    format!(
        r##"<!doctype html>
<html lang="fr">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover">
  <meta name="color-scheme" content="dark light">
  <meta name="theme-color" content="{dark_bg}" media="(prefers-color-scheme: dark)">
  <meta name="theme-color" content="{light_bg}" media="(prefers-color-scheme: light)">
  <meta name="apple-mobile-web-app-capable" content="yes">
  <meta name="apple-mobile-web-app-status-bar-style" content="black-translucent">
  <meta name="apple-mobile-web-app-title" content="{NAME}">
  <meta name="application-name" content="{NAME}">
  <meta name="format-detection" content="telephone=no">
  <meta name="description" content="{NAME}, le moteur de traitement média pour les stacks *arr.">
  <link rel="icon" type="image/svg+xml" href="{MARK_PATH}">
  <title>{NAME} — Pilotage et supervision</title>
  <style>
{styles}
  </style>
</head>
<body>
{body}
</body>
</html>"##,
        dark_bg = DARK_THEME.bg_app,
        light_bg = LIGHT_THEME.bg_app,
        styles = theme::get_all_styles(),
    )
}

/// Page unique servie pour toute route d'interface : le corps est monté par le module WASM.
pub fn render_page() -> String {
    html_document(&format!(
        "  <noscript>{NAME} demande JavaScript et WebAssembly.</noscript>\n  <script type=\"module\">import init from \"{APP_MODULE}\"; init();</script>"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_page_loads_the_wasm_module_with_the_theme_styles() {
        let html = render_page();
        assert!(html.contains("type=\"module\""));
        assert!(html.contains("\"/pkg/flux_client.js\""));
        assert!(html.contains("<style>"));
        assert!(html.contains("<title>Forgearr —"));
        assert!(html.contains(r#"rel="icon" type="image/svg+xml" href="/forgearr.svg""#));
        assert!(html.contains("lang=\"fr\""));
        assert!(html.contains("viewport-fit=cover"));
        assert!(!html.contains("<script>\n"));
    }

    #[test]
    fn narrow_screens_get_a_navigation_drawer_and_its_menu_button() {
        let html = render_page();
        for rule in [
            ".topbar-menu-btn",
            ".application-sidebar.is-open",
            ".sidebar-scrim",
        ] {
            assert!(html.contains(rule), "règle absente : {rule}");
        }
    }
}
