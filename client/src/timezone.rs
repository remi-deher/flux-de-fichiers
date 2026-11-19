//! Fuseau horaire partagé dans l'interface WASM : synchronisé avec le central et mémorisé localement.

use crate::api;
#[cfg(target_arch = "wasm32")]
use crate::prefs;
use forgearr_shared::timezone::{ClockInfo, DEFAULT_TIMEZONE, canonical_timezone};
use leptos::prelude::*;

#[cfg(target_arch = "wasm32")]
const STORAGE_KEY: &str = "flux_timezone_pref";
pub const TIMEZONE_PATH: &str = "/api/v1/system/timezone";

/// Signal réactif du fuseau horaire IANA actif (`Europe/Paris`, `UTC`, `Etc/GMT`…).
#[derive(Clone, Copy)]
pub struct TimezoneState(pub RwSignal<String>);

/// Restaure le fuseau horaire mémorisé localement (ou `UTC` par défaut).
#[cfg(target_arch = "wasm32")]
pub fn restore() -> String {
    prefs::load(STORAGE_KEY)
        .and_then(|raw| canonical_timezone(&raw))
        .unwrap_or_else(|| DEFAULT_TIMEZONE.to_owned())
}

#[cfg(not(target_arch = "wasm32"))]
pub fn restore() -> String {
    DEFAULT_TIMEZONE.to_owned()
}

/// Installe le signal de fuseau horaire dans le contexte racine.
pub fn install() -> String {
    let saved = restore();
    provide_context(TimezoneState(RwSignal::new(saved.clone())));
    saved
}

/// Lit le fuseau horaire actif (réactif lorsqu'appelé dans une vue Leptos).
pub fn current_timezone() -> String {
    use_context::<TimezoneState>()
        .map(|state| state.0.get())
        .unwrap_or_else(restore)
}

/// Met à jour le fuseau horaire actif côté navigateur (stockage local + signal).
pub fn set_timezone(raw: &str) -> Option<String> {
    let canonical = canonical_timezone(raw)?;
    #[cfg(target_arch = "wasm32")]
    prefs::store(STORAGE_KEY, &canonical);
    if let Some(state) = use_context::<TimezoneState>() {
        state.0.set(canonical.clone());
    }
    Some(canonical)
}

/// Synchronise le signal local avec le fuseau configuré sur le central.
pub fn sync_from_server() {
    leptos::task::spawn_local(async move {
        if let Ok(info) = api::get::<ClockInfo>(TIMEZONE_PATH).await {
            let _ = set_timezone(&info.timezone);
        }
    });
}

/// Détecte le fuseau horaire IANA du navigateur via `Intl.DateTimeFormat().resolvedOptions().timeZone`.
#[cfg(target_arch = "wasm32")]
pub fn browser_timezone() -> Option<String> {
    let dtf = js_sys::Intl::DateTimeFormat::new(&js_sys::Array::new(), &js_sys::Object::new());
    let options = dtf.resolved_options();
    let tz = js_sys::Reflect::get(&options, &wasm_bindgen::JsValue::from_str("timeZone")).ok()?;
    let raw = tz.as_string()?;
    canonical_timezone(&raw)
}

#[cfg(not(target_arch = "wasm32"))]
pub fn browser_timezone() -> Option<String> {
    None
}
