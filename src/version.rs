//! Version affichée de l'application.
//!
//! L'image est compilée une fois sur `test`, puis promue en `X.Y.Z` sans recompiler : la version
//! compilée (`Cargo.toml`) est alors celle de la release précédente. La release pose donc
//! `FLUX_VERSION` dans l'image ; la version compilée ne sert que de repli et de témoin de
//! cohérence avec l'interface WASM.

use std::sync::OnceLock;

pub const VERSION_ENV: &str = "FLUX_VERSION";
/// Version de `Cargo.toml` au moment de la compilation.
pub const BUILD_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Choisit la version à afficher : la valeur de l'image si elle est renseignée, sinon la compilée.
fn resolve_version(configured: Option<&str>) -> String {
    configured
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(BUILD_VERSION)
        .to_owned()
}

/// Version de la release en cours d'exécution.
pub fn app_version() -> &'static str {
    static VERSION: OnceLock<String> = OnceLock::new();
    VERSION.get_or_init(|| resolve_version(std::env::var(VERSION_ENV).ok().as_deref()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn image_version_wins_over_the_compiled_one() {
        assert_eq!(resolve_version(Some("9.9.9")), "9.9.9");
        assert_eq!(resolve_version(Some("  1.2.3 \n")), "1.2.3");
    }

    #[test]
    fn compiled_version_is_the_fallback() {
        assert_eq!(resolve_version(None), BUILD_VERSION);
        assert_eq!(resolve_version(Some("   ")), BUILD_VERSION);
    }
}
