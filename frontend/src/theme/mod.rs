//! Module de gestion du thème et des styles CSS-in-Rust typés.

pub mod base;
pub mod components;
pub mod flows;
pub mod plugins;
pub mod styles;
pub mod tokens;
pub mod widgets;

pub use base::BASE_CSS;
pub use components::COMPONENTS_CSS;
pub use flows::FLOWS_CSS;
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
    s.push_str(WIDGETS_CSS);
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
        assert!(styles.contains(".widget[data-state=\"loading\"]"));
    }
}
