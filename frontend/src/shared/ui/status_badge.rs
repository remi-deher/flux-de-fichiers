//! Composant StatusBadge : Pastille sémantique d'état avec point lumineux pulsant.

use leptos::prelude::*;

#[component]
pub fn StatusBadge(
    #[prop(into)] label: String,
    #[prop(into, optional)] variant: Option<String>,
    #[prop(into, optional)] pulse: Option<bool>,
) -> impl IntoView {
    let var = variant.unwrap_or_else(|| "info".to_string());
    let with_pulse = pulse.unwrap_or(false);
    view! {
        <span class=format!("status-pill {}", var)>
            <span class=if with_pulse { "status-dot pulse" } else { "status-dot" }></span>
            <span>{label}</span>
        </span>
    }
}
