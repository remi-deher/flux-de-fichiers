//! Module de gestion du thème et des styles CSS-in-Rust typés.

pub mod auth;
pub mod base;
pub mod components;
pub mod flows;
pub mod graph;
pub mod plugins;
pub mod styles;
pub mod tokens;
pub mod touch;
pub mod widgets;

pub use auth::AUTH_CSS;
pub use base::BASE_CSS;
pub use components::COMPONENTS_CSS;
pub use flows::FLOWS_CSS;
pub use graph::GRAPH_CSS;
pub use plugins::PLUGINS_CSS;
pub use styles::{generate_global_style, generate_global_tokens_css};
pub use tokens::{DARK_THEME, LIGHT_THEME, ThemeTokens};
pub use widgets::WIDGETS_CSS;

use std::sync::LazyLock;

static ALL_STYLES: LazyLock<String> = LazyLock::new(|| {
    let mut s = String::with_capacity(65_536);
    s.push_str(&styles::generate_global_tokens_css());
    s.push('\n');
    s.push_str(BASE_CSS);
    s.push('\n');
    s.push_str(COMPONENTS_CSS);
    s.push('\n');
    s.push_str(PLUGINS_CSS);
    s.push('\n');
    s.push_str(FLOWS_CSS);
    s.push('\n');
    s.push_str(GRAPH_CSS);
    s.push('\n');
    s.push_str(WIDGETS_CSS);
    s.push('\n');
    s.push_str(AUTH_CSS);
    s.push('\n');
    s.push_str(&touch::touch_css());
    s
});

/// Retourne l'ensemble des styles CSS-in-Rust consolidés du thème.
pub fn get_all_styles() -> &'static str {
    &ALL_STYLES
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_all_styles_contains_tokens_and_base() {
        let styles = get_all_styles();
        assert!(styles.contains(":root {"));
        assert!(styles.contains("--bg-app: #151416;"));
        assert!(styles.contains(".application-shell {"));
        assert!(styles.contains(".status-pill {"));
        assert!(styles.contains(".widget-grid {"));
        assert!(styles.contains(".auth-gate {"));
        assert!(styles.contains(".widget[data-state=\"loading\"]"));
        assert!(styles.contains(".live-progress {"));
        assert!(styles.contains(".toast-stack {"));
        assert!(styles.contains(".cmd-palette {"));
        assert!(styles.contains(".flow-graph__port-dot {"));
        assert!(styles.contains(".stat-strip {"));
        assert!(styles.contains(".overview-toolbar {"));
        assert!(styles.contains(".page-section {"));
        assert!(styles.contains(".resource-grid {"));
        assert!(styles.contains(".resource-card {"));
        assert!(styles.contains(".ui-tag {"));
        assert!(styles.contains(".tab-btn-badge {"));
        assert!(styles.contains(".settings-bento-grid {"));
        assert!(styles.contains(".studio-toolbar {"));
        assert!(styles.contains(".flow-right-dock {"));
        assert!(styles.contains(".flow-canvas-controls {"));
        assert!(styles.contains(".mini-viewport {"));
        assert!(styles.contains(".flow-quick-add {"));
        assert!(styles.contains(".flow-canvas__marquee {"));
        assert!(styles.contains(".flow-wire__popover {"));
        assert!(styles.contains(".application-shell.is-workbench .app-topbar {"));
        assert!(styles.contains(".auth-role-grid {"));
        assert!(styles.contains(".diagnostic-box {"));
        assert!(styles.contains(".ai-chat__messages {"));
    }

    /// Vérifie qu'une feuille CSS a ses accolades équilibrées et aucune imbrication hors `@media`/`@container`/`@keyframes`.
    fn assert_balanced_css(name: &str, css: &str) {
        let mut depth: usize = 0;
        let mut in_at_block = false;
        let mut prelude = String::new();
        let mut chars = css.char_indices().peekable();
        let mut in_comment = false;

        while let Some((_idx, ch)) = chars.next() {
            if in_comment {
                if ch == '*' && chars.peek().is_some_and(|(_, next)| *next == '/') {
                    chars.next();
                    in_comment = false;
                }
                continue;
            }
            if ch == '/' && chars.peek().is_some_and(|(_, next)| *next == '*') {
                chars.next();
                in_comment = true;
                continue;
            }
            match ch {
                '{' => {
                    if depth == 0 {
                        let trimmed = prelude.trim_start();
                        in_at_block = ["@media", "@container", "@keyframes"]
                            .iter()
                            .any(|rule| trimmed.starts_with(rule));
                    } else if depth == 1 && !in_at_block {
                        panic!(
                            "{name}: imbrication CSS accidentelle détectée près de `{}`",
                            prelude.trim()
                        );
                    } else if depth >= 2 {
                        panic!(
                            "{name}: profondeur d'accolades excessive ({depth}) près de `{}`",
                            prelude.trim()
                        );
                    }
                    depth += 1;
                    prelude.clear();
                }
                '}' => {
                    assert!(
                        depth > 0,
                        "{name}: accolade fermante orpheline détectée après `{}`",
                        prelude.trim()
                    );
                    depth -= 1;
                    if depth == 0 {
                        in_at_block = false;
                    }
                    prelude.clear();
                }
                ';' if depth == 0 => prelude.clear(),
                _ => prelude.push(ch),
            }
        }
        assert_eq!(depth, 0, "{name}: accolades non refermées en fin de CSS");
    }

    /// Valeur en pixels d'une déclaration `font-size` littérale, s'il y en a une.
    fn literal_font_px(value: &str) -> Option<f64> {
        let value = value.trim();
        if let Some(rem) = value.strip_suffix("rem") {
            return rem
                .parse::<f64>()
                .ok()
                .map(|rem| rem * tokens::ROOT_FONT_PX);
        }
        value.strip_suffix("px")?.parse().ok()
    }

    #[test]
    fn every_text_size_comes_from_the_type_scale_and_none_is_below_the_minimum() {
        for step in tokens::TYPE_SCALE {
            assert!(
                step.px >= tokens::MIN_FONT_PX,
                "échelon {} trop petit",
                step.name
            );
        }
        for declaration in get_all_styles().split("font-size:").skip(1) {
            let value = declaration
                .split([';', '}'])
                .next()
                .unwrap_or_default()
                .trim();
            if let Some(px) = literal_font_px(value) {
                panic!("taille littérale {value} ({px} px) : utiliser var(--font-size-*)");
            }
        }
    }

    #[test]
    fn container_queries_only_use_the_declared_widths_and_have_a_container() {
        let styles = get_all_styles();
        assert!(styles.matches("container-type: inline-size;").count() >= 2);
        let allowed: Vec<String> = tokens::ContainerBreakpoint::ALL
            .iter()
            .map(|point| format!("{}px", point.max_width_px()))
            .collect();
        let queries: Vec<&str> = styles.split("@container (max-width:").skip(1).collect();
        assert!(!queries.is_empty(), "aucune requête de conteneur");
        for query in queries {
            let width = query.split(')').next().unwrap_or_default().trim();
            assert!(
                allowed.iter().any(|allowed| allowed == width),
                "largeur de conteneur {width} hors de tokens::ContainerBreakpoint"
            );
        }
    }

    #[test]
    fn media_queries_only_use_the_declared_breakpoints() {
        let allowed: Vec<String> = tokens::Breakpoint::ALL
            .iter()
            .map(|point| format!("{}px", point.max_width_px()))
            .collect();
        for query in get_all_styles().split("@media (max-width:").skip(1) {
            let width = query.split(')').next().unwrap_or_default().trim();
            assert!(
                allowed.iter().any(|allowed| allowed == width),
                "point de rupture {width} hors de tokens::Breakpoint"
            );
        }
    }

    #[test]
    fn test_all_styles_has_balanced_braces_and_no_accidental_nesting() {
        let tokens_css = styles::generate_global_tokens_css();
        for (name, css) in [
            ("tokens", tokens_css.as_str()),
            ("BASE_CSS", BASE_CSS),
            ("COMPONENTS_CSS", COMPONENTS_CSS),
            ("PLUGINS_CSS", PLUGINS_CSS),
            ("FLOWS_CSS", FLOWS_CSS),
            ("GRAPH_CSS", GRAPH_CSS),
            ("WIDGETS_CSS", WIDGETS_CSS),
            ("AUTH_CSS", AUTH_CSS),
            ("TOUCH_CSS", touch::touch_css().as_str()),
            ("ALL_STYLES", get_all_styles()),
        ] {
            assert_balanced_css(name, css);
        }
    }
}
