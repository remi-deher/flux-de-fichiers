//! Client web Leptos compilé en WASM (ADR 0014) : toute l'interface, servie à la racine.
//!
//! Il partage avec le central les types de `forgearr-shared`.

mod access;
pub mod ai;
pub mod api;
mod app;
pub mod flows;
pub mod format;
pub mod jobs;
pub mod libraries;
mod live;
pub mod location;
mod overview;
mod plugins;
mod prefs;
pub mod route;
mod settings;
mod shell;
mod sleep;
pub mod theme;
pub mod timezone;
mod ui;
mod url_state;
pub mod workers;

use wasm_bindgen::prelude::wasm_bindgen;

/// Point d'entrée appelé par le module ES généré par `wasm-bindgen`.
#[wasm_bindgen(start)]
pub fn start() {
    console_error_panic_hook::set_once();
    leptos::mount::mount_to_body(app::App);
}
