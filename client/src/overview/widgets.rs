//! Widgets de la vue d'ensemble : lesquels sont affichés, préférence gardée dans le navigateur.

use crate::prefs;
use leptos::prelude::*;
use serde::{Deserialize, Serialize};

/// Clé de stockage de la liste des widgets masqués.
const STORAGE_KEY: &str = "flux_overview_hidden";

/// Bloc de la vue d'ensemble que l'on peut afficher ou masquer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Widget {
    Metrics,
    Hardware,
    Attention,
    RecentJobs,
}

impl Widget {
    pub const ALL: [Self; 4] = [
        Self::Metrics,
        Self::Hardware,
        Self::Attention,
        Self::RecentJobs,
    ];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Metrics => "Chiffres clés",
            Self::Hardware => "Matériel du central",
            Self::Attention => "À surveiller",
            Self::RecentJobs => "Tâches récentes",
        }
    }
}

/// Widgets masqués ; on retient les masqués pour qu'un nouveau widget apparaisse par défaut.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WidgetLayout {
    hidden: Vec<Widget>,
}

impl WidgetLayout {
    /// Relit une préférence enregistrée ; une valeur absente ou illisible montre tout.
    pub fn parse(stored: Option<&str>) -> Self {
        stored
            .and_then(|text| serde_json::from_str(text).ok())
            .unwrap_or_default()
    }

    pub fn to_stored(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }

    pub fn is_visible(&self, widget: Widget) -> bool {
        !self.hidden.contains(&widget)
    }

    pub fn set_visible(&mut self, widget: Widget, visible: bool) {
        self.hidden.retain(|other| *other != widget);
        if !visible {
            self.hidden.push(widget);
        }
    }

    pub fn restore() -> Self {
        Self::parse(prefs::load(STORAGE_KEY).as_deref())
    }

    pub fn persist(&self) {
        prefs::store(STORAGE_KEY, &self.to_stored());
    }
}

/// Panneau repliable de cases à cocher, une par widget.
#[component]
pub fn WidgetPicker(layout: RwSignal<WidgetLayout>) -> impl IntoView {
    view! {
        <details class="widget-picker">
            <summary class="btn btn-secondary btn-sm">"Personnaliser"</summary>
            <div class="widget-picker__list">
                {Widget::ALL.into_iter().map(|widget| view! {
                    <label class="cluster">
                        <input type="checkbox"
                            prop:checked=move || layout.with(|current| current.is_visible(widget))
                            on:change=move |event| {
                                layout.update(|current| current.set_visible(widget, event_target_checked(&event)));
                                layout.with(WidgetLayout::persist);
                            } />
                        <span>{widget.label()}</span>
                    </label>
                }).collect_view()}
            </div>
        </details>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn everything_is_visible_without_a_valid_preference() {
        for stored in [None, Some(""), Some("pas du json"), Some("[\"inconnu\"]")] {
            let layout = WidgetLayout::parse(stored);
            assert!(
                Widget::ALL
                    .into_iter()
                    .all(|widget| layout.is_visible(widget))
            );
        }
    }

    #[test]
    fn hiding_a_widget_round_trips_through_the_stored_text() {
        let mut layout = WidgetLayout::default();
        layout.set_visible(Widget::Attention, false);
        layout.set_visible(Widget::Attention, false);
        let stored = layout.to_stored();
        assert_eq!(stored, r#"{"hidden":["attention"]}"#);
        let restored = WidgetLayout::parse(Some(&stored));
        assert!(!restored.is_visible(Widget::Attention));
        assert!(restored.is_visible(Widget::Metrics));
        let mut shown = restored;
        shown.set_visible(Widget::Attention, true);
        assert_eq!(shown, WidgetLayout::default());
    }
}
