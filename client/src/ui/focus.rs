//! Focus des dialogues modaux (WAI-ARIA « Dialog (Modal) ») : à l'ouverture, mémorise l'élément
//! actif puis place le focus dans le dialogue ; Tab et Maj+Tab bouclent à l'intérieur ; à la
//! fermeture, le focus revient sur l'élément qui a ouvert le dialogue (WCAG 2.4.3).

use leptos::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{Element, HtmlElement, KeyboardEvent};

/// Éléments pouvant recevoir le focus clavier dans un dialogue.
const FOCUSABLE: &str = "a[href], button:not([disabled]), input:not([disabled]), \
     select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex=\"-1\"])";

/// Touche qui déplace le focus.
const TAB_KEY: &str = "Tab";

/// Touches qui activent un élément focalisable (comportement d'un bouton natif).
const ACTIVATION_KEYS: [&str; 2] = ["Enter", " "];

/// La touche active-t-elle une ligne ou une carte cliquable ?
pub fn is_activation_key(key: &str) -> bool {
    ACTIVATION_KEYS.contains(&key)
}

/// Gestionnaire `keydown` d'une ligne ou d'une carte cliquable (`tabindex="0"`) : Entrée ou Espace
/// lancent `action`, comme un clic (WCAG 2.1.1). Les touches venues d'un contrôle imbriqué
/// (case à cocher, bouton d'action) restent à ce contrôle.
pub fn on_activate(action: impl Fn() + 'static) -> impl Fn(KeyboardEvent) + 'static {
    move |event: KeyboardEvent| {
        if is_activation_key(&event.key()) && event.target() == event.current_target() {
            event.prevent_default();
            action();
        }
    }
}

/// Où envoyer le focus quand Tab sort d'un dialogue : `None` laisse le navigateur faire.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Wrap {
    /// Maj+Tab depuis le premier élément (ou le dialogue lui-même) : aller au dernier.
    ToLast,
    /// Tab depuis le dernier élément : revenir au premier.
    ToFirst,
}

/// Décide du rebouclage. `position` : rang de l'élément actif parmi `count` focusables, `None`
/// s'il n'en fait pas partie (le conteneur du dialogue, par exemple).
pub fn wrap_for(position: Option<usize>, count: usize, backwards: bool) -> Option<Wrap> {
    if count == 0 {
        return None;
    }
    let last = count - 1;
    match (position, backwards) {
        (None | Some(0), true) => Some(Wrap::ToLast),
        (None, false) => Some(Wrap::ToFirst),
        (Some(index), false) if index == last => Some(Wrap::ToFirst),
        _ => None,
    }
}

fn focusables(container: &Element) -> Vec<HtmlElement> {
    let Ok(list) = container.query_selector_all(FOCUSABLE) else {
        return Vec::new();
    };
    (0..list.length())
        .filter_map(|index| list.item(index))
        .filter_map(|node| node.dyn_into::<HtmlElement>().ok())
        .filter(|element| element.offset_parent().is_some())
        .collect()
}

/// Garde Tab et Maj+Tab dans `container` ; à brancher sur le `keydown` du dialogue.
pub fn trap_tab(event: &KeyboardEvent, container: &Element) {
    if event.key() != TAB_KEY {
        return;
    }
    let items = focusables(container);
    let active = document().active_element();
    let position = items.iter().position(|item| {
        active
            .as_ref()
            .is_some_and(|active| active == item.unchecked_ref::<Element>())
    });
    let target = match wrap_for(position, items.len(), event.shift_key()) {
        Some(Wrap::ToFirst) => items.first(),
        Some(Wrap::ToLast) => items.last(),
        None => return,
    };
    if let Some(target) = target {
        event.prevent_default();
        let _ = target.focus();
    }
}

/// Mémorise l'élément actif à l'ouverture et lance `focus_inside` ; rend le focus à la fermeture.
pub fn use_modal_focus(open: Signal<bool>, focus_inside: impl Fn() + 'static) {
    let previous = StoredValue::new_local(None::<HtmlElement>);
    Effect::new(move |was_open: Option<bool>| {
        let is_open = open.get();
        let was_open = was_open.unwrap_or(false);
        if is_open && !was_open {
            previous.set_value(
                document()
                    .active_element()
                    .and_then(|element| element.dyn_into::<HtmlElement>().ok()),
            );
            focus_inside();
        } else if !is_open
            && was_open
            && let Some(element) = previous.get_value()
        {
            let _ = element.focus();
        }
        is_open
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enter_and_space_activate_like_a_button() {
        assert!(is_activation_key("Enter"));
        assert!(is_activation_key(" "));
        assert!(!is_activation_key("Tab"));
        assert!(!is_activation_key("a"));
    }

    #[test]
    fn tab_wraps_from_the_last_element_and_shift_tab_from_the_first() {
        assert_eq!(wrap_for(Some(2), 3, false), Some(Wrap::ToFirst));
        assert_eq!(wrap_for(Some(0), 3, true), Some(Wrap::ToLast));
        assert_eq!(wrap_for(Some(1), 3, false), None);
        assert_eq!(wrap_for(Some(1), 3, true), None);
    }

    #[test]
    fn focus_on_the_dialog_itself_enters_at_either_end_and_empty_dialogs_are_left_alone() {
        assert_eq!(wrap_for(None, 3, false), Some(Wrap::ToFirst));
        assert_eq!(wrap_for(None, 3, true), Some(Wrap::ToLast));
        assert_eq!(wrap_for(None, 0, false), None);
        assert_eq!(wrap_for(Some(0), 1, false), Some(Wrap::ToFirst));
    }
}
