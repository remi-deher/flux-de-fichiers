//! Feuille de filtres de la barre du haut : critères propres à l'écran, en plus du texte recherché.

use crate::route::Route;
use chrono::{DateTime, Duration, Utc};
use flux_shared::jobs::{Job, JobStatus};
use flux_shared::plugins::CatalogEntry;
use flux_shared::plugins::vocab::CatalogSection;
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

/// État d'avancement d'une tâche dans Activité.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum JobStateFilter {
    #[default]
    Any,
    Active,
    Completed,
    Failed,
}

impl FilterChoice for JobStateFilter {
    const ALL: &'static [Self] = &[Self::Any, Self::Active, Self::Completed, Self::Failed];

    fn label(self) -> &'static str {
        match self {
            Self::Any => "Toutes",
            Self::Active => "En file & en cours",
            Self::Completed => "Terminées",
            Self::Failed => "Échouées",
        }
    }
}

impl JobStateFilter {
    pub fn accepts(self, job: &Job) -> bool {
        match self {
            Self::Any => true,
            Self::Active => matches!(job.status, JobStatus::Queued | JobStatus::Running),
            Self::Completed => job.status == JobStatus::Succeeded,
            Self::Failed => job.status == JobStatus::Failed,
        }
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

/// Catégorie du catalogue de plugins.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PluginKindFilter {
    #[default]
    Any,
    Plugins,
    Flows,
    Packs,
    Other,
}

impl FilterChoice for PluginKindFilter {
    const ALL: &'static [Self] = &[
        Self::Any,
        Self::Plugins,
        Self::Flows,
        Self::Packs,
        Self::Other,
    ];

    fn label(self) -> &'static str {
        match self {
            Self::Any => "Toutes",
            Self::Plugins => "Plugins",
            Self::Flows => "Flows",
            Self::Packs => "Packs",
            Self::Other => "Autre",
        }
    }
}

impl PluginKindFilter {
    pub fn accepts(self, entry: &CatalogEntry) -> bool {
        match self {
            Self::Any => entry
                .entry_type
                .section()
                .is_some_and(|section| section != CatalogSection::Applications),
            Self::Plugins => entry.entry_type.section() == Some(CatalogSection::Plugins),
            Self::Flows => entry.entry_type.section() == Some(CatalogSection::Flows),
            Self::Packs => entry.entry_type.section() == Some(CatalogSection::Packs),
            Self::Other => entry.entry_type.section() == Some(CatalogSection::Other),
        }
    }
}

/// Critères actifs, partagés entre la barre du haut et les écrans.
#[derive(Clone, Copy)]
pub struct Filters {
    pub period: RwSignal<Period>,
    pub job_state: RwSignal<JobStateFilter>,
    pub presence: RwSignal<Presence>,
    pub plugin_kind: RwSignal<PluginKindFilter>,
}

impl Filters {
    pub fn install() -> Self {
        let filters = Self {
            period: RwSignal::new(Period::default()),
            job_state: RwSignal::new(JobStateFilter::default()),
            presence: RwSignal::new(Presence::default()),
            plugin_kind: RwSignal::new(PluginKindFilter::default()),
        };
        provide_context(filters);
        filters
    }

    /// Remet tous les critères à zéro (changement d'écran).
    pub fn reset(&self) {
        self.period.set(Period::default());
        self.job_state.set(JobStateFilter::default());
        self.presence.set(Presence::default());
        self.plugin_kind.set(PluginKindFilter::default());
    }
}

/// Critère de période en cours (aucun hors contexte).
pub fn period() -> Period {
    use_context::<Filters>().map_or_else(Period::default, |filters| filters.period.get())
}

/// Critère d'état des tâches en cours (toutes hors contexte).
pub fn job_state() -> JobStateFilter {
    use_context::<Filters>().map_or_else(JobStateFilter::default, |filters| filters.job_state.get())
}

/// Critère de présence en cours (aucun hors contexte).
pub fn presence() -> Presence {
    use_context::<Filters>().map_or_else(Presence::default, |filters| filters.presence.get())
}

/// Critère de catégorie de plugin en cours (toutes hors contexte).
pub fn plugin_kind() -> PluginKindFilter {
    use_context::<Filters>().map_or_else(PluginKindFilter::default, |filters| {
        filters.plugin_kind.get()
    })
}

/// Un écran sans critère propre n'affiche pas la feuille.
fn has_filters(route: &Route) -> bool {
    matches!(
        route.section(),
        Route::Jobs | Route::Workers | Route::Plugins
    )
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
                        {(section == Route::Jobs).then(|| view! {
                            <ChoiceSelect label="État" value=filters.job_state />
                            <ChoiceSelect label="Créée" value=filters.period />
                        })}
                        {(section == Route::Workers).then(|| view! { <ChoiceSelect label="Présence" value=filters.presence /> })}
                        {(section == Route::Plugins).then(|| view! { <ChoiceSelect label="Catégorie" value=filters.plugin_kind /> })}
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
    use flux_shared::plugins::vocab::CatalogItemType;

    fn job_with_status(status: JobStatus) -> Result<Job, serde_json::Error> {
        let mut job: Job = serde_json::from_value(serde_json::json!({
            "id": "00000000-0000-0000-0000-000000000001",
            "source_name": "a.mkv",
            "source_sha256": null,
            "source_bytes": null,
            "output_sha256": null,
            "output_bytes": null,
            "warning": false,
            "target_worker_id": null,
            "worker_id": null,
            "status": "queued",
            "attempts": 0,
            "leased_until": null,
            "output_name": null,
            "error_code": null,
            "flow_id": null,
            "flow_version": null,
            "created_at": "2026-11-14T10:00:00Z",
            "updated_at": "2026-11-14T10:00:00Z"
        }))?;
        job.status = status;
        Ok(job)
    }

    fn catalog_entry(kind: CatalogItemType) -> CatalogEntry {
        CatalogEntry {
            id: "id".into(),
            entry_type: kind,
            version: "1.0.0".into(),
            name: "Nom".into(),
            manifest_path: "manifest.json".into(),
            capability: None,
            current_choice: None,
            distribution: None,
            category: None,
            tags: Vec::new(),
            owner: None,
        }
    }

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
    fn job_state_filter_partitions_statuses() -> Result<(), serde_json::Error> {
        let queued = job_with_status(JobStatus::Queued)?;
        let running = job_with_status(JobStatus::Running)?;
        let done = job_with_status(JobStatus::Succeeded)?;
        let failed = job_with_status(JobStatus::Failed)?;
        assert!(JobStateFilter::Any.accepts(&queued) && JobStateFilter::Any.accepts(&failed));
        assert!(
            JobStateFilter::Active.accepts(&queued) && JobStateFilter::Active.accepts(&running)
        );
        assert!(!JobStateFilter::Active.accepts(&done));
        assert!(
            JobStateFilter::Completed.accepts(&done) && !JobStateFilter::Completed.accepts(&failed)
        );
        assert!(JobStateFilter::Failed.accepts(&failed) && !JobStateFilter::Failed.accepts(&done));
        Ok(())
    }

    #[test]
    fn plugin_kind_filter_selects_catalog_sections() {
        let plugin = catalog_entry(CatalogItemType::Plugin);
        let flow = catalog_entry(CatalogItemType::Flow);
        let pack = catalog_entry(CatalogItemType::Pack);
        let preset = catalog_entry(CatalogItemType::Preset);
        let app = catalog_entry(CatalogItemType::App);
        let block = catalog_entry(CatalogItemType::Block);

        assert!(PluginKindFilter::Any.accepts(&plugin));
        assert!(PluginKindFilter::Any.accepts(&flow));
        assert!(!PluginKindFilter::Any.accepts(&app));
        assert!(!PluginKindFilter::Any.accepts(&block));

        assert!(PluginKindFilter::Plugins.accepts(&plugin));
        assert!(!PluginKindFilter::Plugins.accepts(&flow));
        assert!(PluginKindFilter::Flows.accepts(&flow));
        assert!(PluginKindFilter::Packs.accepts(&pack));
        assert!(PluginKindFilter::Other.accepts(&preset));
    }

    #[test]
    fn presence_splits_online_and_offline_workers() {
        assert!(Presence::Any.accepts(true) && Presence::Any.accepts(false));
        assert!(Presence::Online.accepts(true) && !Presence::Online.accepts(false));
        assert!(Presence::Offline.accepts(false) && !Presence::Offline.accepts(true));
    }

    #[test]
    fn jobs_workers_and_plugins_have_filters() {
        assert!(has_filters(&Route::Jobs));
        assert!(has_filters(&Route::Workers));
        assert!(has_filters(&Route::Plugins));
        assert!(!has_filters(&Route::Flows));
        assert!(!has_filters(&Route::Overview));
    }
}
