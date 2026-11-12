//! Composant StatusBadge : Pastille sémantique d'état avec point lumineux pulsant.

use leptos::prelude::*;

#[component]
pub fn StatusBadge(
    #[prop(into)] label: String,
    #[prop(into, optional)] variant: Option<&'static str>,
    #[prop(into, optional)] id: Option<&'static str>,
    #[prop(into, optional)] pulse: Option<bool>,
    #[prop(into, optional)] title: Option<String>,
) -> impl IntoView {
    let var = variant.unwrap_or("info");
    let cls = format!("status-pill {}", var);
    let with_pulse = pulse.unwrap_or(false);
    view! {
        <span id=id class=cls title=title>
            <span class=if with_pulse { "status-dot pulse" } else { "status-dot" }></span>
            <span>{label}</span>
        </span>
    }
}
