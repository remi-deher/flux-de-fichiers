//! Composant MetricCard : Tuile de métrique héroïque avec indicateur de tendance et valeur tabulaire.

use leptos::prelude::*;

#[component]
pub fn MetricCard(
    #[prop(into)] label: String,
    #[prop(into)] value: String,
    #[prop(into, optional)] hint: Option<String>,
    #[prop(into, optional)] icon: Option<String>,
    #[prop(into, optional)] status_class: Option<String>,
) -> impl IntoView {
    let cls = status_class.unwrap_or_else(|| "default".to_string());
    view! {
        <div class=format!("metric-tile {}", cls)>
            <div class="metric-tile-header">
                {icon.map(|ic| view! {
                    <span class="metric-icon" aria-hidden="true">{ic}</span>
                })}
                <span class="metric-label">{label}</span>
            </div>
            <div class="metric-value-display">
                <span class="metric-number">{value}</span>
            </div>
            {hint.map(|h| view! {
                <span class="metric-hint">{h}</span>
            })}
        </div>
    }
}
