//! Feuille de filtres de la barre du haut : critères propres à l'écran, en plus du texte recherché.

use crate::route::Route;
use chrono::{DateTime, Duration, Utc};
use leptos::prelude::*;

/// Critère proposé dans la feuille, avec ses choix.
pub trait FilterChoice: Copy + PartialEq + Send + Sync + 'static {
    const ALL: &'static [Self];
    fn label(self) -> &'static str;
}

/// Ancienneté d'une tâche.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Period {
    #[default]
    Any,
    Day,
    Week,
    Month,
}

impl FilterChoice for Period {
    const ALL: &'static [Self] = &[Self::Any, Self::Day, Self::Week, Self::Month];

    fn label(self) -> &'static str {
        match self {
            Self::Any => "Toute la période",
            Self::Day => "Dernières 24 h",
            Self::Week => "7 derniers jours",
            Self::Month => "30 derniers jours",
        }
    }
}

impl Period {
    fn window(self) -> Option<Duration> {
        match self {
            Self::Any => None,
            Self::Day => Some(Duration::days(1)),
            Self::Week => Some(Duration::days(7)),
            Self::Month => Some(Duration::days(30)),
        }
    }

    /// Une date de création passe-t-elle ce critère à l'instant `now` ?
    pub fn accepts(self, created_at: DateTime<Utc>, now: DateTime<Utc>) -> bool {
        self.window()
            .is_none_or(|window| now - created_at <= window)
    }
}

/// Présence d'un worker.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Presence {
    #[default]
    Any,
    Online,
    Offline,
}

impl FilterChoice for Presence {
    const ALL: &'static [Self] = &[Self::Any, Self::Online, Self::Offline];

    fn label(self) -> &'static str {
        match self {
            Self::Any => "En ligne ou non",
            Self::Online => "En ligne",
            Self::Offline => "Hors ligne",
        }
    }
}

impl Presence {
    pub fn accepts(self, online: bool) -> bool {
        match self {
            Self::Any => true,
            Self::Online => online,
            Self::Offline => !online,
        }
    }
}

/// Critères actifs, partagés entre la barre du haut et les écrans.
#[derive(Clone, Copy)]
pub struct Filters {
    pub period: RwSignal<Period>,
    pub presence: RwSignal<Presence>,
}

impl Filters {
    pub fn install() -> Self {
        let filters = Self {
            period: RwSignal::new(Period::default()),
            presence: RwSignal::new(Presence::default()),
        };
        provide_context(filters);
        filters
    }

    /// Remet tous les critères à zéro (changement d'écran).
    pub fn reset(&self) {
        self.period.set(Period::default());
        self.presence.set(Presence::default());
    }
}

/// Critère de période en cours (aucun hors contexte).
pub fn period() -> Period {
    use_context::<Filters>().map_or_else(Period::default, |filters| filters.period.get())
}

/// Critère de présence en cours (aucun hors contexte).
pub fn presence() -> Presence {
    use_context::<Filters>().map_or_else(Presence::default, |filters| filters.presence.get())
}

/// Un écran sans critère propre n'affiche pas la feuille.
fn has_filters(route: &Route) -> bool {
    matches!(route.section(), Route::Jobs | Route::Workers)
}

#[component]
fn ChoiceSelect<T: FilterChoice>(label: &'static str, value: RwSignal<T>) -> impl IntoView {
    view! {
        <label class="cluster">
            <span class="form-label">{label}</span>
            <select class="form-select"
                on:change=move |event| {
                    if let Some(choice) = event_target_value(&event).parse::<usize>().ok().and_then(|index| T::ALL.get(index)) {
                        value.set(*choice);
                    }
                }>
                {T::ALL.iter().enumerate().map(|(index, choice)| view! {
                    <option value=index.to_string() selected=move || value.get() == *choice>{choice.label()}</option>
                }).collect_view()}
            </select>
        </label>
    }
}

/// Bouton « Filtres » et son panneau, selon l'écran affiché.
#[component]
pub fn FilterSheet(route: Signal<Route>) -> impl IntoView {
    let filters = use_context::<Filters>().unwrap_or_else(Filters::install);
    view! {
        {move || has_filters(&route.get()).then(|| {
            let section = route.get().section();
            view! {
                <details class="popover">
                    <summary class="btn btn-secondary btn-sm">"Filtres"</summary>
                    <div class="popover__panel">
                        {(section == Route::Jobs).then(|| view! { <ChoiceSelect label="Créée" value=filters.period /> })}
                        {(section == Route::Workers).then(|| view! { <ChoiceSelect label="Présence" value=filters.presence /> })}
                        <button type="button" class="btn btn-secondary btn-sm" on:click=move |_| filters.reset()>"Réinitialiser"</button>
                    </div>
                </details>
            }
        })}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_period_keeps_only_recent_creations() {
        let now = DateTime::<Utc>::UNIX_EPOCH + Duration::days(60);
        let ago = |days| now - Duration::days(days);
        assert!(Period::Any.accepts(ago(59), now));
        assert!(Period::Day.accepts(ago(0), now));
        assert!(!Period::Day.accepts(ago(2), now));
        assert!(Period::Week.accepts(ago(7), now));
        assert!(!Period::Week.accepts(ago(8), now));
        assert!(Period::Month.accepts(ago(30), now));
        assert!(!Period::Month.accepts(ago(31), now));
    }

    #[test]
    fn presence_splits_online_and_offline_workers() {
        assert!(Presence::Any.accepts(true) && Presence::Any.accepts(false));
        assert!(Presence::Online.accepts(true) && !Presence::Online.accepts(false));
        assert!(Presence::Offline.accepts(false) && !Presence::Offline.accepts(true));
    }

    #[test]
    fn only_jobs_and_workers_have_filters() {
        assert!(has_filters(&Route::Jobs));
        assert!(has_filters(&Route::Workers));
        assert!(!has_filters(&Route::Flows));
        assert!(!has_filters(&Route::Overview));
    }
}
