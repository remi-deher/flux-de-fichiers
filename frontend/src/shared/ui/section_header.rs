//! Composant SectionHeader : En-tête générique de section de panneau.

use leptos::prelude::*;

#[component]
pub fn SectionHeader(
    #[prop(into)] title: String,
    #[prop(into, optional)] id: Option<&'static str>,
    #[prop(into, optional)] subtitle: Option<String>,
) -> impl IntoView {
    view! {
        <div class="section-panel-header">
            <div>
                <h2 id=id class="section-title">{title}</h2>
                {subtitle.map(|sub| view! {
                    <p class="section-subtitle">{sub}</p>
                })}
            </div>
        </div>
    }
}
