//! Sous-navigation d'un écran : une rangée d'onglets liée au signal d'onglet de l'écran (ADR 0035).

use leptos::prelude::*;

/// Onglets d'un écran. `tabs` donne chaque onglet et son libellé, dans l'ordre d'affichage.
#[component]
pub fn PageSubnav<T>(
    aria_label: &'static str,
    tabs: Vec<(T, &'static str)>,
    active: RwSignal<T>,
) -> impl IntoView
where
    T: Copy + PartialEq + Send + Sync + 'static,
{
    view! {
        <nav class="page-subnav" role="tablist" aria-label=aria_label>
            {tabs.into_iter().map(|(tab, label)| {
                let is_active = move || active.get() == tab;
                view! {
                    <button
                        type="button"
                        role="tab"
                        class="page-subnav__tab"
                        class:is-active=is_active
                        aria-selected=move || is_active().to_string()
                        on:click=move |_| active.set(tab)
                    >
                        {label}
                    </button>
                }
            }).collect_view()}
        </nav>
    }
}
