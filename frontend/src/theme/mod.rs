//! Module de gestion du thème et des styles CSS-in-Rust typés.

pub mod auth;
pub mod base;
pub mod components;
pub mod flows;
pub mod graph;
pub mod plugins;
pub mod styles;
pub mod tokens;
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
        assert!(styles.contains("--bg-app: #0d1117;"));
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
        assert!(styles.contains(".libraries-grid {"));
        assert!(styles.contains(".workers-grid {"));
        assert!(styles.contains(".plugins-grid {"));
        assert!(styles.contains(".settings-bento-grid {"));
        assert!(styles.contains(".studio-toolbar {"));
        assert!(styles.contains(".auth-role-grid {"));
        assert!(styles.contains(".diagnostic-box {"));
    }

    /// Vérifie qu'une feuille CSS a ses accolades équilibrées et aucune imbrication hors `@media`/`@keyframes`.
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
                        in_at_block =
                            trimmed.starts_with("@media") || trimmed.starts_with("@keyframes");
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
            ("ALL_STYLES", get_all_styles()),
        ] {
            assert_balanced_css(name, css);
        }
    }
}
