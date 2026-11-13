//! Page HTML du client web (ADR 0014) : métadonnées, styles du thème et chargement du module WASM.
//!
//! Toute l'interface est le client Leptos compilé en WASM (`client/`) ; ce crate ne génère que la
//! page qui le charge, avec les styles typés du thème.

pub mod theme;

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
  <meta name="theme-color" content="#0d1117" media="(prefers-color-scheme: dark)">
  <meta name="theme-color" content="#f6f8fa" media="(prefers-color-scheme: light)">
  <meta name="apple-mobile-web-app-capable" content="yes">
  <meta name="apple-mobile-web-app-status-bar-style" content="black-translucent">
  <meta name="apple-mobile-web-app-title" content="Flux de Fichiers">
  <meta name="application-name" content="Flux de Fichiers">
  <meta name="format-detection" content="telephone=no">
  <meta name="description" content="Console de pilotage et supervision de remuxage Flux de Fichiers.">
  <title>Flux de Fichiers — Pilotage et supervision</title>
  <style>
{styles}
  </style>
</head>
<body>
{body}
</body>
</html>"##,
        styles = theme::get_all_styles(),
    )
}

/// Page unique servie pour toute route d'interface : le corps est monté par le module WASM.
pub fn render_page() -> String {
    html_document(&format!(
        "  <noscript>Flux de Fichiers demande JavaScript et WebAssembly.</noscript>\n  <script type=\"module\">import init from \"{APP_MODULE}\"; init();</script>"
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
        assert!(html.contains("lang=\"fr\""));
        assert!(html.contains("viewport-fit=cover"));
        assert!(!html.contains("<script>\n"));
    }
}
