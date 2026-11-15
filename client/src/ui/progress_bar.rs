//! Barre de progression accessible pour le suivi temps réel des tâches.

use super::metric_card::MetricTone;
use leptos::prelude::*;

const MIN_PERCENT: u32 = 0;
const MAX_PERCENT: u32 = 100;

/// Borne un pourcentage entre `0` et `100`.
pub fn clamp_percent(percent: u32) -> u32 {
    percent.clamp(MIN_PERCENT, MAX_PERCENT)
}

/// Barre de progression déterminée (`percent = Some`) ou indéterminée (`percent = None`).
#[component]
pub fn ProgressBar(
    #[prop(optional)] percent: Option<u32>,
    #[prop(into, optional)] label: Option<String>,
    #[prop(optional)] tone: MetricTone,
) -> impl IntoView {
    let clamped = percent.map(clamp_percent);
    let bar_class = if clamped.is_some() {
        "live-progress__bar"
    } else {
        "live-progress__bar is-indeterminate"
    };
    let bar_style = clamped.map(|value| format!("width:{value}%"));
    let aria_now = clamped.map(|value| value.to_string());
    let text = label.unwrap_or_else(|| {
        clamped.map_or_else(|| "En cours".to_owned(), |value| format!("{value} %"))
    });
    view! {
        <div class="live-progress" data-tone=tone.as_attr() role="progressbar"
            aria-valuemin=MIN_PERCENT.to_string() aria-valuemax=MAX_PERCENT.to_string()
            aria-valuenow=aria_now>
            <div class="live-progress__track">
                <div class=bar_class style=bar_style></div>
            </div>
            <span class="live-progress__label">{text}</span>
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clamp_percent_caps_above_hundred() {
        assert_eq!(clamp_percent(0), 0);
        assert_eq!(clamp_percent(42), 42);
        assert_eq!(clamp_percent(100), 100);
        assert_eq!(clamp_percent(250), 100);
    }
}
