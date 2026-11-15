//! Bandeau horizontal de KPIs compacts (`StatStrip`), commun à plusieurs écrans.

use super::metric_card::MetricTone;
use leptos::prelude::*;

/// Indicateur individuel affiché dans un `StatStrip`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StatItem {
    pub label: &'static str,
    pub value: String,
    pub tone: MetricTone,
    pub detail: Option<String>,
}

impl StatItem {
    pub fn new(label: &'static str, value: impl Into<String>, tone: MetricTone) -> Self {
        Self {
            label,
            value: value.into(),
            tone,
            detail: None,
        }
    }

    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }
}

/// Rangée compacte de pilules statistiques sous un `PageHeader`.
#[component]
pub fn StatStrip(
    aria_label: &'static str,
    #[prop(into)] items: Signal<Vec<StatItem>>,
) -> impl IntoView {
    view! {
        <section class="stat-strip" aria-label=aria_label>
            {move || {
                items
                    .get()
                    .into_iter()
                    .map(|item| {
                        view! {
                            <div class="stat-pill" data-tone=item.tone.as_attr()>
                                <span class="stat-pill__label">{item.label}</span>
                                <span class="stat-pill__value">{item.value}</span>
                                {item.detail.map(|detail| view! {
                                    <span class="stat-pill__detail">{detail}</span>
                                })}
                            </div>
                        }
                    })
                    .collect_view()
            }}
        </section>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stat_item_builders_preserve_fields_and_tones() {
        let basic = StatItem::new("Prêtes", "3", MetricTone::Success);
        assert_eq!(basic.label, "Prêtes");
        assert_eq!(basic.value, "3");
        assert_eq!(basic.tone, MetricTone::Success);
        assert_eq!(basic.detail, None);

        let detailed =
            StatItem::new("Échouées", "2", MetricTone::Danger).with_detail("relançables");
        assert_eq!(detailed.detail.as_deref(), Some("relançables"));
        assert_eq!(detailed.tone.as_attr(), "danger");
    }
}
