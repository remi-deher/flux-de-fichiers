//! Composant MetricCard : Tuile de métrique consolidée et réutilisable.

use leptos::prelude::*;

#[component]
pub fn MetricCard(
    #[prop(into)] label: String,
    #[prop(into)] value: String,
    #[prop(into, optional)] id: Option<&'static str>,
    #[prop(into, optional)] subtitle: Option<String>,
    #[prop(into, optional)] value_style: Option<&'static str>,
) -> impl IntoView {
    view! {
        <div class="metric-card">
            <div class="metric-label">{label}</div>
            <div id=id class="metric-value" style=value_style>{value}</div>
            {subtitle.map(|sub| view! {
                <div class="metric-subtitle">{sub}</div>
            })}
        </div>
    }
}
