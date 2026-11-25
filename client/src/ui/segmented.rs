//! Choix exclusif parmi quelques options, en boutons accolés.

use leptos::prelude::*;

#[component]
pub fn Segmented<T>(
    options: &'static [T],
    selected: RwSignal<T>,
    label: fn(T) -> &'static str,
    aria_label: &'static str,
) -> impl IntoView
where
    T: Copy + PartialEq + Send + Sync + 'static,
{
    view! {
        <div class="store-segmented" role="group" aria-label=aria_label>
            {options.iter().copied().map(|option| {
                let is_active = move || selected.get() == option;
                view! {
                    <button
                        type="button"
                        class="store-segmented__btn"
                        class:is-active=is_active
                        aria-pressed=move || is_active().to_string()
                        on:click=move |_| selected.set(option)
                    >
                        {label(option)}
                    </button>
                }
            }).collect_view()}
        </div>
    }
}
