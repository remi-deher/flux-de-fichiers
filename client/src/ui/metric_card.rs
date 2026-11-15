//! Composant MetricCard : Tuile de métrique consolidée et réutilisable.

use super::icon::{Icon, IconName};
use leptos::prelude::*;
use serde::{Deserialize, Serialize};

/// Taille des icônes de tuile de métrique, en pixels.
const METRIC_ICON_SIZE: u32 = 18;
const METER_MIN_PERCENT: f32 = 0.0;
const METER_MAX_PERCENT: f32 = 100.0;

/// Ton sémantique d'une tuile ; le thème le traduit en couleurs via `data-tone`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MetricTone {
    #[default]
    Neutral,
    Success,
    Warning,
    Danger,
}

impl MetricTone {
    pub const fn as_attr(self) -> &'static str {
        match self {
            Self::Neutral => "neutral",
            Self::Success => "success",
            Self::Warning => "warning",
            Self::Danger => "danger",
        }
    }
}

#[component]
pub fn MetricCard(
    #[prop(into)] label: String,
    #[prop(into)] value: String,
    #[prop(into, optional)] id: Option<&'static str>,
    #[prop(into, optional)] subtitle: Option<String>,
    #[prop(optional)] icon: Option<IconName>,
    #[prop(optional)] tone: MetricTone,
    #[prop(optional)] progress: Option<f32>,
) -> impl IntoView {
    let meter = progress.map(|pct| pct.clamp(METER_MIN_PERCENT, METER_MAX_PERCENT));
    view! {
        <div class="metric-card" data-tone=tone.as_attr()>
            <div class="metric-head">
                {icon.map(|name| view! {
                    <span class="metric-icon"><Icon name=name size=METRIC_ICON_SIZE /></span>
                })}
                <span class="metric-label">{label}</span>
            </div>
            <div id=id class="metric-value">{value}</div>
            {meter.map(|pct| view! {
                <div class="metric-meter" aria-hidden="true">
                    <div class="metric-meter__fill" style=format!("width:{pct:.0}%")></div>
                </div>
            })}
            {subtitle.map(|sub| view! {
                <div class="metric-subtitle">{sub}</div>
            })}
        </div>
    }
}
