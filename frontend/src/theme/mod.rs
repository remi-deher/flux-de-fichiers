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
