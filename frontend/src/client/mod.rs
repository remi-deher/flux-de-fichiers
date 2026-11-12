//! Module de logique cliente du poste de pilotage Flux de Fichiers en Rust pur.

pub mod auth;
pub mod core;
pub mod files;
pub mod flows;
pub mod init;
pub mod jobs;
pub mod libraries;
pub mod plugins;
pub mod pure;
pub mod tabs;
pub mod workers;

use std::sync::LazyLock;

static CLIENT_SCRIPT: LazyLock<String> = LazyLock::new(|| {
    let mut script = String::with_capacity(32_768);
    script.push_str("(function() {\n  'use strict';\n\n");
    script.push_str(&pure::script());
    script.push_str("\n\n");
    script.push_str(core::SCRIPT);
    script.push_str("\n\n");
    script.push_str(auth::SCRIPT);
    script.push_str("\n\n");
    script.push_str(&crate::core::panel::catalog_script());
    script.push_str("\n\n");
    script.push_str(tabs::SCRIPT);
    script.push_str("\n\n");
    script.push_str(files::SCRIPT);
    script.push_str("\n\n");
    script.push_str(jobs::SCRIPT);
    script.push_str("\n\n");
    script.push_str(workers::SCRIPT);
    script.push_str("\n\n");
    script.push_str(libraries::SCRIPT);
    script.push_str("\n\n");
    script.push_str(plugins::SCRIPT);
    script.push_str("\n\n");
    script.push_str(&flows::script());
    script.push_str("\n\n");
    script.push_str(init::SCRIPT);
    script.push_str("\n})();\n");
    script
});

/// Retourne le script client consolidé généré à partir des modules Rust.
pub fn get_client_script() -> &'static str {
    &CLIENT_SCRIPT
}
