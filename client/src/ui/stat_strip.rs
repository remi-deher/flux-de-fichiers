//! Bandeau horizontal de KPIs compacts (`StatStrip`), commun à tous les écrans et cliquable en 1 clic.

use super::metric_card::MetricTone;
use leptos::prelude::*;

/// Indicateur individuel affiché dans un `StatStrip`, éventuellement filtrant ou navigant au clic.
#[derive(Clone)]
pub struct StatItem {
    pub label: &'static str,
    pub value: String,
    pub tone: MetricTone,
    pub detail: Option<String>,
    pub active: bool,
    pub on_click: Option<Callback<()>>,
    pub href: Option<String>,
}

impl StatItem {
    pub fn new(label: &'static str, value: impl Into<String>, tone: MetricTone) -> Self {
        Self {
            label,
            value: value.into(),
            tone,
            detail: None,
            active: false,
            on_click: None,
            href: None,
        }
    }

    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    pub fn with_link(mut self, href: impl Into<String>) -> Self {
        self.href = Some(href.into());
        self
    }

    pub fn with_filter(mut self, active: bool, on_click: Callback<()>) -> Self {
        self.active = active;
        self.on_click = Some(on_click);
        self
    }
}

fn stat_pill_inner(label: &'static str, detail: Option<String>, value: String) -> impl IntoView {
    view! {
        <div class="stat-pill__top">
            <span class="stat-pill__label">{label}</span>
            {detail.map(|detail| view! {
                <span class="stat-pill__detail">{detail}</span>
            })}
        </div>
        <span class="stat-pill__value">{value}</span>
    }
}

/// Rangée compacte et alignée de tuiles statistiques en tête d'écran.
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
                        let tone = item.tone.as_attr();
                        let inner = stat_pill_inner(item.label, item.detail, item.value);
                        if let Some(href) = item.href {
                            view! { <a href=href class="stat-pill is-interactive" data-tone=tone>{inner}</a> }.into_any()
                        } else if let Some(cb) = item.on_click {
                            view! {
                                <button
                                    type="button"
                                    class="stat-pill is-interactive"
                                    class:is-active=item.active
                                    data-tone=tone
                                    aria-pressed=item.active.to_string()
                                    on:click=move |_| cb.run(())
                                >
                                    {inner}
                                </button>
                            }
                            .into_any()
                        } else {
                            view! {
                                <div class="stat-pill" data-tone=tone>
                                    {inner}
                                </div>
                            }
                            .into_any()
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
    fn stat_item_builders_preserve_fields_tones_and_filters() {
        let basic = StatItem::new("Prêtes", "3", MetricTone::Success);
        assert_eq!(basic.label, "Prêtes");
        assert_eq!(basic.value, "3");
        assert_eq!(basic.tone, MetricTone::Success);
        assert_eq!(basic.detail, None);
        assert!(!basic.active);
        assert!(basic.on_click.is_none());

        let detailed = StatItem::new("Échouées", "2", MetricTone::Danger)
            .with_detail("relançables")
            .with_filter(true, Callback::new(|()| {}));
        assert_eq!(detailed.detail.as_deref(), Some("relançables"));
        assert_eq!(detailed.tone.as_attr(), "danger");
        assert!(detailed.active);
        assert!(detailed.on_click.is_some());
    }
}
