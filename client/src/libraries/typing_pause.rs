//! Pause de frappe : une action n'est lancée qu'une fois la valeur lue restée la même pendant un délai.

use leptos::prelude::*;
use std::{rc::Rc, time::Duration};

/// Appelle `run` avec ce que lit `read` une fois `delay` écoulé sans nouvelle valeur. `run` reçoit aussi de
/// quoi savoir, plus tard, si cette valeur est toujours la dernière (une réponse tardive ne doit pas la
/// remplacer).
pub fn after_pause<T: 'static>(
    read: impl Fn() -> T + 'static,
    delay: Duration,
    run: impl Fn(T, Rc<dyn Fn() -> bool>) + Clone + 'static,
) {
    let generation = StoredValue::new(0_u64);
    Effect::new(move |_| {
        let value = read();
        generation.update_value(|count| *count += 1);
        let ticket = generation.get_value();
        let run = run.clone();
        set_timeout(
            move || {
                let current: Rc<dyn Fn() -> bool> =
                    Rc::new(move || generation.try_get_value() == Some(ticket));
                if current() {
                    run(value, current);
                }
            },
            delay,
        );
    });
}
