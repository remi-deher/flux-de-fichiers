//! Événements de pointeur du canevas : une même logique pour la souris, le doigt et le stylet.

use crate::route::PRIMARY_BUTTON;
use wasm_bindgen::JsCast;
use web_sys::{Element, PointerEvent};

/// Appui à traiter : bouton principal du pointeur principal (le premier doigt d'un geste tactile
/// multiple, la souris, le stylet). Les doigts suivants sont ignorés.
pub fn is_primary_press(event: &PointerEvent) -> bool {
    event.is_primary() && event.button() == PRIMARY_BUTTON
}

/// Au toucher, le navigateur capture le pointeur sur l'élément pressé : `pointerup` reviendrait
/// à la sortie d'où part la liaison. Relâcher cette capture l'envoie à l'élément sous le doigt
/// (l'entrée visée), comme à la souris.
pub fn release_implicit_capture(event: &PointerEvent) {
    let pointer = event.pointer_id();
    if let Some(element) = event
        .target()
        .and_then(|target| target.dyn_into::<Element>().ok())
        && element.has_pointer_capture(pointer)
    {
        let _ = element.release_pointer_capture(pointer);
    }
}
