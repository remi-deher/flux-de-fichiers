//! Bibliothèque frontend Leptos pour Flux de Fichiers.

// Les vues Leptos imbriquées (page Paramètres, barre latérale) dépassent la limite par défaut de 128.
#![recursion_limit = "512"]

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
        assert!(!html.contains("id=\"header-search-clear-btn\""));
        assert!(!html.contains("search-kbd"));
        assert!(html.contains("search-capsule__tools"));
        assert!(html.contains("id=\"search-capsule\""));
        assert!(html.contains("id=\"search-filter-btn\""));
        assert!(html.contains("id=\"filter-sheet\""));
        assert!(html.contains("data-capsule-tab=\"flows\""));
        assert!(html.contains("data-capsule-tab=\"files\""));
        assert!(html.contains("data-capsule-tab=\"flow-editor\""));
        assert!(html.contains("id=\"btn-canvas-save-flow\""));
        assert!(html.contains("function syncCapsuleActions"));
        assert!(html.contains(&format!(
            "syncCapsuleActions('{}')",
            crate::core::CapsuleContext::FlowEditor.as_attr()
        )));
        assert!(!html.contains("topbar-context"));
        assert!(!html.contains("breadcrumb"));
        assert!(!html.contains("current-section-title"));
        assert!(!html.contains("TAB_TITLES"));
        assert!(!html.contains("registre-toolbar"));
        assert!(html.contains("data-filter-page=\"jobs\""));
        assert!(html.contains("function toggleFilterSheet"));
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
        assert!(html.contains("function successRateTone"));
        assert!(html.contains("function setHeroStatus"));
        assert!(html.contains("id=\"hero-status-badge\""));
        assert!(!html.contains("id=\"quick-goto-settings-btn\""));
    }

    #[test]
    fn test_widgets_reach_the_page_with_their_client_renderers() {
        let html = render_full_page();
        for def in crate::features::widgets::WIDGETS {
            assert!(
                html.contains(&format!("data-widget=\"{}\"", def.kind.as_attr())),
                "widget absent : {}",
                def.kind.as_attr()
            );
        }
        assert!(html.contains("id=\"widget-dashboard-attention\""));
        assert!(html.contains("id=\"widget-activity-running\""));
        assert!(html.contains("id=\"widget-workers-tools\""));
        assert!(html.contains("function renderWidgets"));
        assert!(html.contains("function installTool"));
        assert!(html.contains("function scheduleToolsStatusRefresh"));
        assert!(!html.contains("'locked'"));
        assert!(html.contains("field.isSet"));
        assert!(html.contains("function markWidgetSource"));
        assert!(html.contains("[data-goto]"));
        assert!(html.contains(".widget-grid {"));
    }

    #[test]
    fn test_activity_uses_the_common_registre_and_drawer() {
        let html = render_full_page();
        assert!(html.contains("id=\"jobs-fact\""));
        assert!(html.contains("id=\"jobs-registre\""));
        assert!(!html.contains("job-detail-dialog"));
        assert!(!html.contains("activity-view-diagnostic"));
        assert!(html.contains("function setJobFilter"));
        assert!(html.contains("function openJob"));
        assert!(html.contains("function fillJobDrawer"));
        assert!(html.contains("JOBS_VIEW_FILTERS"));
        assert!(html.contains("if (factId === 'jobs-fact') return 'Tâche';"));
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

    #[test]
    fn test_settings_page_is_sections_with_account_and_theme_in_the_sidebar() {
        let html = render_full_page();
        for id in [
            "btn-subtab-settings-overview",
            "btn-subtab-settings-keys",
            "settings-section-keys",
            "settings-tile-database",
            "security-rotate-cmd",
            "account-menu-btn",
            "theme-segmented",
        ] {
            assert!(html.contains(&format!("id=\"{id}\"")), "{id}");
        }
        for function in [
            "loadSystemStatus",
            "renderSettingsOverview",
            "setupAccountMenu",
            "accountInitials",
        ] {
            assert!(html.contains(&format!("function {function}")), "{function}");
        }
        // Le compte et le thème n'ont plus de formulaire dans la page des paramètres.
        assert_eq!(html.matches("id=\"account-password-form\"").count(), 1);
        assert_eq!(html.matches("id=\"theme-select\"").count(), 0);
        assert!(html.contains("[data-theme-choice]"));
    }

    #[test]
    fn test_every_copy_button_goes_through_the_one_clipboard_helper() {
        let html = render_full_page();
        // Un seul appel direct à l'API du navigateur : le reste passe par le repli HTTP et le retour visuel.
        assert_eq!(html.matches("navigator.clipboard.writeText").count(), 1);
        assert!(html.contains("document.execCommand('copy')"));
        assert!(html.contains("copyWithFeedback(el('api-key-copy-btn')"));
        assert!(html.contains("copyWithFeedback(btnCopyCmd"));
        assert!(html.contains(".btn[data-copied=\"true\"]"));
        assert!(html.contains(".clipboard-proxy"));
    }

    #[test]
    fn test_catalog_ids_and_pure_functions_reach_the_page() {
        let html = render_full_page();
        for panel in crate::core::panels() {
            assert!(html.contains(&format!("id=\"{}\"", panel.dom_id)));
            assert!(html.contains(&format!("id=\"{}\"", panel.button_id)));
            assert!(html.contains(panel.search_placeholder));
        }
        assert!(html.contains("function matchesQuery"));
        assert!(html.contains("function isVideoFlowName"));
        assert!(html.contains("function emptyStateRow"));
        assert!(html.contains("function loadFlows"));
        assert!(html.contains("function bootstrapAuth"));
        assert!(html.contains("function submitSetup"));
        assert!(html.contains("function submitWorkerSetup"));
        assert!(html.contains("function setupAuthGate"));
        assert!(html.contains("function loadNodeOverview"));
        assert!(html.contains("function copyToClipboard"));
        assert!(html.contains("function copyWithFeedback"));
        assert!(html.contains("function applyAccess"));
        assert!(html.contains("function loadApiKeys"));
        assert!(!html.contains("volatileAdminKey"));
        assert!(!html.contains("checkSession"));
        assert!(html.contains("function openCanvas"));
        assert!(html.contains("class=\"registre\""));
        assert!(html.contains("class=\"inspection-drawer\""));
        assert!(html.contains("class=\"inspection-drawer__header\""));
        assert!(html.contains("class=\"inspection-drawer__subnav\""));
        assert!(html.contains(">Aperçu<"));
        assert!(html.contains("data-inspection-close"));
        assert!(!html.contains("class=\"registre-split\""));
        assert!(html.contains("id=\"libraries-table-body\""));
        assert!(html.contains("id=\"libraries-fact-pane-overview\""));
        assert!(!html.contains("id=\"libraries-fact-tab-nodes\""));
        assert!(!html.contains("id=\"libraries-fact-pane-nodes\""));
        assert!(!html.contains("id=\"libraries-fact-pane-flow\""));
        assert!(html.contains("id=\"library-name-input\""));
        assert!(html.contains("id=\"library-flow-select\""));
        assert!(html.contains("function loadLibraries"));
        assert!(html.contains("function saveLibrary"));
        assert!(!html.contains("id=\"files-table-body\""));
        assert!(!html.contains("id=\"confirm-remux-dialog\""));
        assert!(!html.contains("btn-subtab-files-browser"));
        assert!(!html.contains("Volume /input"));
        assert!(html.contains("id=\"flows-fact-action\""));
        assert!(html.contains(">Éditer<"));
        assert!(html.contains("id=\"btn-workers-connect\""));
        assert!(html.contains("id=\"workers-fact-tab-connect\""));
        assert!(!html.contains("registre-fact"));
        assert!(html.contains("Raccorder un nœud"));
        assert!(html.contains("subtab-plugins-app"));
        assert!(html.contains("subtab-plugins-pack"));
        assert!(html.contains("subtab-plugins-resource"));
        assert!(html.contains("subtab-plugins-repos"));
        assert!(html.contains("Retour aux flux"));
        assert!(html.contains("function showRegistreFact"));
        assert!(!html.contains("btn-subtab-flows-canvas"));
        assert!(html.contains("function resetSimulation"));
    }

    #[test]
    fn plugin_form_comes_from_the_manifest_schema() {
        let html = render_full_page();
        assert!(html.contains("id=\"plugin-config-fields\""));
        assert!(html.contains("function renderPluginConfigFields"));
        assert!(html.contains("data-setting-name"));
        assert!(html.contains("data-tags"));
        assert!(!html.contains("192.168.1.50"));
        assert!(!html.contains("plugin-cfg-url"));
        assert!(!html.contains("plugin-cfg-apikey"));
        assert!(!html.contains("plugin-cfg-local-path"));
        assert!(!html.contains("plugin-cfg-remote-path"));
        assert!(!html.contains("value=\"/data\""));
        assert!(!html.contains("value=\"/media\""));
        assert!(!html.contains("'/tv'"));
        assert!(!html.contains("'/movies'"));
    }
}
