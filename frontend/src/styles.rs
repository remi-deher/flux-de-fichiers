//! Feuilles de styles et variables de design pour le poste de pilotage Flux de Fichiers en Rust pur.

use crate::theme::{BASE_CSS, COMPONENTS_CSS, FLOWS_CSS, PLUGINS_CSS};
use std::sync::LazyLock;

pub const CSS_BASE: &str = BASE_CSS;
pub const CSS_COMPONENTS: &str = COMPONENTS_CSS;
pub const CSS_PLUGINS: &str = PLUGINS_CSS;
pub const CSS_FLOWS: &str = FLOWS_CSS;

pub static CSS_STYLES: LazyLock<String> = LazyLock::new(|| {
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

pub fn get_all_styles() -> &'static str {
    &CSS_STYLES
}
