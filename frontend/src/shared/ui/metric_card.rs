//! Composant MetricCard : Tuile de métrique consolidée et réutilisable.

use super::icon::{Icon, IconName};
use leptos::prelude::*;
use serde::{Deserialize, Serialize};

/// Taille des icônes de tuile de métrique, en pixels.
const METRIC_ICON_SIZE: u32 = 18;

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
) -> impl IntoView {
    view! {
        <div class="metric-card" data-tone=tone.as_attr()>
            <div class="metric-head">
                {icon.map(|name| view! {
                    <span class="metric-icon"><Icon name=name size=METRIC_ICON_SIZE /></span>
                })}
                <span class="metric-label">{label}</span>
            </div>
            <div id=id class="metric-value">{value}</div>
            {subtitle.map(|sub| view! {
                <div class="metric-subtitle">{sub}</div>
            })}
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tone_attributes_match_serde_names() {
        for tone in [
            MetricTone::Neutral,
            MetricTone::Success,
            MetricTone::Warning,
            MetricTone::Danger,
        ] {
            let serialized = serde_json::to_string(&tone).unwrap_or_default();
            assert_eq!(serialized, format!("\"{}\"", tone.as_attr()));
        }
    }

    #[test]
    fn test_metric_card_renders_tone_icon_and_value() {
        let html = view! {
            <MetricCard
                label="Taux"
                value="98%"
                id="metric-test"
                subtitle="Baux"
                icon=IconName::Check
                tone=MetricTone::Success
            />
        }
        .to_html();
        assert!(html.contains("data-tone=\"success\""));
        assert!(html.contains("class=\"metric-icon\""));
        assert!(html.contains("id=\"metric-test\""));
        assert!(html.contains("Baux"));
    }

    #[test]
    fn test_metric_card_without_icon_has_no_icon_chip() {
        let html = view! { <MetricCard label="Taux" value="1" /> }.to_html();
        assert!(html.contains("data-tone=\"neutral\""));
        assert!(!html.contains("metric-icon"));
    }
}
