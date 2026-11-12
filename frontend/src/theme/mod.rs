//! Module de gestion du thème et des styles CSS-in-Rust typés.

pub mod base;
pub mod components;
pub mod flows;
pub mod plugins;
pub mod styles;
pub mod tokens;

pub use base::BASE_CSS;
pub use components::COMPONENTS_CSS;
pub use flows::FLOWS_CSS;
pub use plugins::PLUGINS_CSS;
pub use styles::generate_global_style;
pub use tokens::{DARK_THEME, LIGHT_THEME, ThemeTokens};

use std::sync::LazyLock;

static ALL_STYLES: LazyLock<String> = LazyLock::new(|| {
    let mut s = String::with_capacity(32_768);
    s.push_str(BASE_CSS);
    s.push('\n');
    s.push_str(COMPONENTS_CSS);
    s.push('\n');
    s.push_str(PLUGINS_CSS);
    s.push('\n');
    s.push_str(FLOWS_CSS);
    s
});

/// Retourne l'ensemble des styles CSS-in-Rust consolidés du thème.
pub fn get_all_styles() -> &'static str {
    &ALL_STYLES
}
