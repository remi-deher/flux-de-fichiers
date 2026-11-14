//! Tableau à onglets : la navigation est attachée au haut du tableau qu'elle commande.

use super::subnav::{SubTab, SubTabList};
use leptos::prelude::*;

#[component]
pub fn TabbedTable(
    aria_label: &'static str,
    tabs: Vec<SubTab>,
    selected: RwSignal<usize>,
    children: Children,
) -> impl IntoView {
    view! {
        <section class="tabbed-table">
            <nav class="tabbed-table__tabs" aria-label=aria_label>
                <SubTabList tabs=tabs selected=selected />
            </nav>
            <div class="tabbed-table__body" role="tabpanel">{children()}</div>
        </section>
    }
}
