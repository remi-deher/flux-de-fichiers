//! Client web Leptos compilé en WASM (ADR 0014).
//!
//! Il remplace l'interface historique écran par écran ; il est servi sous `/app/` et partage
//! avec le central les types de `flux-shared`.

pub mod api;
mod app;
pub mod flows;
pub mod format;
pub mod jobs;
pub mod libraries;
pub mod route;
pub mod workers;

use wasm_bindgen::prelude::wasm_bindgen;

/// Point d'entrée appelé par le module ES généré par `wasm-bindgen`.
#[wasm_bindgen(start)]
pub fn start() {
    console_error_panic_hook::set_once();
    leptos::mount::mount_to_body(app::App);
}
