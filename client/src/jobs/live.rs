//! Abonnement au flux SSE des tâches : progression en direct et relecture de la liste.

use super::model::{EVENT_NAMES, needs_reload, progress_of};
use flux_shared::jobs::JobEvent;
use leptos::prelude::*;
use std::collections::HashMap;
use uuid::Uuid;
use wasm_bindgen::{JsCast, closure::Closure};
use web_sys::{EventSource, MessageEvent};

const EVENTS_PATH: &str = "/api/v1/jobs/events";

/// Écoute `/api/v1/jobs/events` pour toute la vie de la page : chaque changement d'état
/// incrémente `reload`, chaque progression met à jour `progress`.
pub fn subscribe(reload: RwSignal<u32>, progress: RwSignal<HashMap<Uuid, u32>>) {
    let Ok(source) = EventSource::new(EVENTS_PATH) else {
        return;
    };
    for event_type in EVENT_NAMES {
        let listener = Closure::<dyn FnMut(MessageEvent)>::new(move |message: MessageEvent| {
            let Some(data) = message.data().as_string() else {
                return;
            };
            let Ok(event) = serde_json::from_str::<JobEvent>(&data) else {
                return;
            };
            if let Some((job_id, pct)) = progress_of(&event) {
                progress.update(|map| {
                    map.insert(job_id, pct);
                });
            }
            if needs_reload(event.event_type) {
                reload.update(|count| *count = count.wrapping_add(1));
            }
        });
        let _ = source.add_event_listener_with_callback(
            event_type.as_str(),
            listener.as_ref().unchecked_ref(),
        );
        // La page garde l'abonnement jusqu'à sa fermeture.
        listener.forget();
    }
    std::mem::forget(source);
}
