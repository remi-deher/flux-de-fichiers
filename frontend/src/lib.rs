//! Bibliothèque frontend Leptos pour Flux de Fichiers.

pub mod app;
pub mod client;
pub mod core;
pub mod features;
pub mod shared;
pub mod theme;

use leptos::prelude::*;

/// Génère la page HTML complète avec les métadonnées W3C, les styles et le contrôleur client.
pub fn render_full_page() -> String {
    let body_html = app::App().to_html();
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
  <script>
{scripts}
  </script>
</body>
</html>"##,
        styles = theme::get_all_styles(),
        body = body_html,
        scripts = client::get_client_script()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_render_full_page_contains_modern_standards_metadata() {
        let html = render_full_page();
        assert!(html.contains("viewport-fit=cover"));
        assert!(html.contains("apple-mobile-web-app-capable"));
        assert!(html.contains("apple-mobile-web-app-status-bar-style"));
        assert!(html.contains("color-scheme"));
        assert!(html.contains("theme-color"));
        assert!(html.contains("format-detection"));
        assert!(html.contains("lang=\"fr\""));
    }

    #[test]
    fn test_render_full_page_contains_contextual_search_bar() {
        let html = render_full_page();
        assert!(html.contains("role=\"search\""));
        assert!(html.contains("id=\"app-topbar\""));
        assert!(html.contains("id=\"global-search-input\""));
        assert!(html.contains("id=\"search-match-count\""));
        assert!(html.contains("id=\"header-search-clear-btn\""));
        assert!(html.contains("search-kbd"));
        assert!(html.contains("applyContextualSearch"));
        assert!(html.contains("setupChromeAutoHide"));
        assert!(html.contains("id=\"header-workers-pill\""));
        assert!(!html.contains("id=\"header-theme-toggle-btn\""));
        assert!(!html.contains("id=\"header-refresh-btn\""));
    }

    #[test]
    fn test_render_full_page_contains_overview_hub() {
        let html = render_full_page();
        assert!(html.contains("Vue d'ensemble"));
        assert!(html.contains("overview-hero-card"));
        assert!(html.contains("id=\"metric-storage-volume\""));
        assert!(html.contains("id=\"metric-success-rate\""));
        assert!(html.contains("id=\"media-formats-bar\""));
        assert!(html.contains("updateOverviewStats"));
    }

    #[test]
    fn test_render_full_page_subnav_and_no_refresh_buttons() {
        let html = render_full_page();
        assert!(html.contains("class=\"app-subnav\""));
        assert!(html.contains("app-subnav__scroller"));
        assert!(html.contains("app-subnav__item"));
        assert!(html.contains("app-subnav-toolbar"));

        assert!(!html.contains("id=\"refresh-all-btn\""));
        assert!(!html.contains("id=\"files-refresh-btn\""));
        assert!(!html.contains("id=\"jobs-refresh-btn\""));
        assert!(!html.contains("id=\"btn-refresh-workers\""));
        assert!(!html.contains("id=\"header-refresh-btn\""));
        assert!(!html.contains("id=\"btn-sync-all-repos\""));
        assert!(!html.contains("repo-sync-btn"));
    }
}
