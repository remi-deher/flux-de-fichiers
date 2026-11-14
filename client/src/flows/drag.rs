//! Élément glissable, et cible de dépôt s'il porte un identifiant de carte.

use super::dnd::{DragPayload, drop_outcome};
use super::state::Editor;
use leptos::prelude::*;
use wasm_bindgen::JsCast;

/// Type MIME posé sur le glisser : Firefox exige une donnée pour démarrer.
const DRAG_MIME: &str = "text/plain";

#[component]
pub fn DragItem(
    editor: Editor,
    payload: DragPayload,
    /// Identifiant de la carte, si l'élément accepte aussi un dépôt.
    #[prop(optional, into)]
    target: Option<String>,
    #[prop(optional)] class: &'static str,
    #[prop(optional, into)] style: String,
    children: Children,
) -> impl IntoView {
    let class_payload = payload.clone();
    let start_payload = payload;
    let over_target = target.clone();
    let drop_target = target.clone();
    let class_target = target;
    let drop_state = move || {
        let id = class_target.as_deref()?;
        if editor.drop_target.get().as_deref() != Some(id) {
            return None;
        }
        let payload = editor.drag.get()?;
        Some(
            editor
                .graph
                .with(|graph| drop_outcome(graph, &payload, id).is_ok()),
        )
    };
    let accepting = drop_state.clone();
    view! {
        <div
            class=class
            class:is-dragging=move || editor.drag.get().as_ref() == Some(&class_payload)
            class:is-drop-ok=move || drop_state() == Some(true)
            class:is-drop-refused=move || accepting() == Some(false)
            style=style
            draggable="true"
            on:dragstart=move |event| {
                editor.drop_notice.set(None);
                editor.drag.set(Some(start_payload.clone()));
                if let Some(transfer) = event.data_transfer() {
                    let _ = transfer.set_data(DRAG_MIME, "flux");
                    transfer.set_effect_allowed("move");
                }
            }
            on:dragend=move |_| editor.end_drag()
            on:dragover=move |event| {
                let Some(id) = over_target.clone() else { return };
                if editor.drag.get_untracked().is_some() {
                    event.prevent_default();
                    if editor.drop_target.get_untracked().as_deref() != Some(id.as_str()) {
                        editor.drop_target.set(Some(id));
                    }
                }
            }
            on:dragleave=move |event| {
                // Sortir vers un enfant de la carte ne compte pas comme sortir de la carte.
                let inside = event
                    .related_target()
                    .and_then(|node| node.dyn_into::<web_sys::Node>().ok())
                    .zip(event.current_target().and_then(|node| node.dyn_into::<web_sys::Node>().ok()))
                    .is_some_and(|(related, card)| card.contains(Some(&related)));
                if !inside {
                    editor.drop_target.set(None);
                }
            }
            on:drop=move |event| {
                let Some(id) = drop_target.clone() else { return };
                event.prevent_default();
                editor.drop_notice.set(editor.drop_on(&id).err());
            }
        >
            {children()}
        </div>
    }
}
