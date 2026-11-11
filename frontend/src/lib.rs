//! Bibliothèque frontend Leptos pour Flux de Fichiers.

pub mod app;
pub mod components;
pub mod core;
pub mod features;
pub mod scripts;
pub mod shared;
pub mod styles;
pub mod theme;
pub mod types;

use leptos::prelude::*;

/// Génère la page HTML complète avec les métadonnées W3C, les styles et le contrôleur client.
pub fn render_full_page() -> String {
    let body_html = app::App().to_html();
    format!(
        r#"<!doctype html>
<html lang="fr">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <meta name="description" content="Console de pilotage et supervision de remuxage Flux de Fichiers.">
  <title>Flux de Fichiers — Pilotage et supervision</title>
  <style>
{styles}
  </style>
</head>
<body>
{body}
  <script>
{scripts}
  </script>
</body>
</html>"#,
        styles = styles::get_all_styles(),
        body = body_html,
        scripts = scripts::CLIENT_SCRIPT
    )
}
