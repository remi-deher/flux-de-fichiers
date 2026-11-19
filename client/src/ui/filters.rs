//! Critères de filtrage des écrans : pilotage en 1 clic par `StatStrip` + feuille de période sur Activité.

use crate::route::Route;
use chrono::{DateTime, Duration, Utc};
use forgearr_shared::jobs::{Job, JobStatus};
use forgearr_shared::plugins::CatalogEntry;
use forgearr_shared::plugins::vocab::CatalogSection;
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

impl JobStateFilter {
    pub fn accepts(self, job: &Job) -> bool {
        match self {
            Self::Any => true,
            Self::Active => matches!(job.status, JobStatus::Queued | JobStatus::Running),
            Self::Completed => matches!(job.status, JobStatus::Succeeded | JobStatus::Canceled),
            Self::Failed => job.status == JobStatus::Failed,
        }
    }
}

/// Présence et activité d'un worker.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Presence {
    #[default]
    Any,
    Online,
    Offline,
    Working,
}

impl Presence {
    pub fn accepts(self, online: bool, busy: bool) -> bool {
        match self {
            Self::Any => true,
            Self::Online => online,
            Self::Offline => !online,
            Self::Working => busy,
        }
    }
}

/// Catégorie du catalogue d'extensions.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PluginKindFilter {
    #[default]
    Any,
    Plugins,
    Flows,
    Scripts,
    Resources,
    Packs,
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
            Self::Scripts => entry.entry_type.section() == Some(CatalogSection::Scripts),
            Self::Resources => entry.entry_type.section() == Some(CatalogSection::Other),
            Self::Packs => entry.entry_type.section() == Some(CatalogSection::Packs),
        }
    }
}

/// Filtre 1-clic des bibliothèques via leur `StatStrip`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LibraryFilter {
    #[default]
    All,
    Ready,
    MissingFlow,
    MissingWorker,
}

impl LibraryFilter {
    pub fn accepts(self, has_flow: bool, has_worker: bool) -> bool {
        match self {
            Self::All => true,
            Self::Ready => has_flow && has_worker,
            Self::MissingFlow => !has_flow,
            Self::MissingWorker => !has_worker,
        }
    }
}

/// Filtre 1-clic des flux via leur `StatStrip`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FlowFilter {
    #[default]
    All,
    Used,
    Unused,
}

impl FlowFilter {
    pub fn accepts(self, bound_libraries: usize) -> bool {
        match self {
            Self::All => true,
            Self::Used => bound_libraries > 0,
            Self::Unused => bound_libraries == 0,
        }
    }
}

/// Critères actifs, partagés entre les bandeaux `StatStrip`, la barre du haut et les écrans.
#[derive(Clone, Copy)]
pub struct Filters {
    pub period: RwSignal<Period>,
    pub job_state: RwSignal<JobStateFilter>,
    pub presence: RwSignal<Presence>,
    pub plugin_kind: RwSignal<PluginKindFilter>,
    pub library: RwSignal<LibraryFilter>,
    pub flow: RwSignal<FlowFilter>,
    /// Pré-filtre à appliquer juste après une redirection (ex. depuis la vue d'ensemble).
    pub pending_job_state: RwSignal<Option<JobStateFilter>>,
}

impl Filters {
    pub fn install() -> Self {
        let filters = Self {
            period: RwSignal::new(Period::default()),
            job_state: RwSignal::new(JobStateFilter::default()),
            presence: RwSignal::new(Presence::default()),
            plugin_kind: RwSignal::new(PluginKindFilter::default()),
            library: RwSignal::new(LibraryFilter::default()),
            flow: RwSignal::new(FlowFilter::default()),
            pending_job_state: RwSignal::new(None),
        };
        provide_context(filters);
        filters
    }

    /// Remet tous les critères à zéro.
    pub fn reset(&self) {
        self.period.set(Period::default());
        self.job_state.set(JobStateFilter::default());
        self.presence.set(Presence::default());
        self.plugin_kind.set(PluginKindFilter::default());
        self.library.set(LibraryFilter::default());
        self.flow.set(FlowFilter::default());
    }

    /// Réinitialise les filtres au changement d'écran puis applique un éventuel pré-filtre différé.
    pub fn apply_route_change(&self) {
        self.reset();
        if let Some(pending) = self.pending_job_state.get_untracked() {
            self.pending_job_state.set(None);
            self.job_state.set(pending);
        }
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

/// Critère des bibliothèques en cours (toutes hors contexte).
pub fn library_filter() -> LibraryFilter {
    use_context::<Filters>().map_or_else(LibraryFilter::default, |filters| filters.library.get())
}

/// Critère des flux en cours (tous hors contexte).
pub fn flow_filter() -> FlowFilter {
    use_context::<Filters>().map_or_else(FlowFilter::default, |filters| filters.flow.get())
}

/// Seul l'écran Activité expose le menu de période temporelle dans la barre du haut.
fn has_filters(route: &Route) -> bool {
    route.section() == Route::Jobs
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

/// Bouton « Période » et son panneau pour l'écran Activité.
#[component]
pub fn FilterSheet(route: Signal<Route>) -> impl IntoView {
    let filters = use_context::<Filters>().unwrap_or_else(Filters::install);
    view! {
        {move || has_filters(&route.get()).then(|| {
            view! {
                <details class="popover">
                    <summary class="btn btn-secondary btn-sm">"Période"</summary>
                    <div class="popover__panel">
                        <ChoiceSelect label="Créée" value=filters.period />
                        <button type="button" class="btn btn-secondary btn-sm" on:click=move |_| filters.period.set(Period::Any)>"Réinitialiser"</button>
                    </div>
                </details>
            }
        })}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use forgearr_shared::plugins::vocab::CatalogItemType;

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
            sha256: None,
            updated_at: None,
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
        let script = catalog_entry(CatalogItemType::Script);
        let pack = catalog_entry(CatalogItemType::Pack);
        let preset = catalog_entry(CatalogItemType::Preset);
        let subtitle = catalog_entry(CatalogItemType::ResourceSubtitle);
        let app = catalog_entry(CatalogItemType::App);
        let block = catalog_entry(CatalogItemType::Block);

        assert!(PluginKindFilter::Any.accepts(&plugin));
        assert!(PluginKindFilter::Any.accepts(&flow));
        assert!(PluginKindFilter::Any.accepts(&script));
        assert!(PluginKindFilter::Any.accepts(&subtitle));
        assert!(!PluginKindFilter::Any.accepts(&app));
        assert!(!PluginKindFilter::Any.accepts(&block));

        assert!(PluginKindFilter::Plugins.accepts(&plugin));
        assert!(!PluginKindFilter::Plugins.accepts(&flow));
        assert!(PluginKindFilter::Flows.accepts(&flow));
        assert!(PluginKindFilter::Scripts.accepts(&script));
        assert!(!PluginKindFilter::Scripts.accepts(&flow));
        assert!(PluginKindFilter::Resources.accepts(&preset));
        assert!(PluginKindFilter::Resources.accepts(&subtitle));
        assert!(!PluginKindFilter::Resources.accepts(&script));
        assert!(PluginKindFilter::Packs.accepts(&pack));
        assert!(!PluginKindFilter::Packs.accepts(&app));
        assert!(PluginKindFilter::Any.accepts(&preset));
        assert!(!PluginKindFilter::Packs.accepts(&preset));
    }

    #[test]
    fn presence_and_resource_filters_accept_matching_states() {
        assert!(Presence::Any.accepts(true, false) && Presence::Any.accepts(false, false));
        assert!(Presence::Online.accepts(true, false) && !Presence::Online.accepts(false, true));
        assert!(Presence::Offline.accepts(false, false) && !Presence::Offline.accepts(true, false));
        assert!(Presence::Working.accepts(true, true) && !Presence::Working.accepts(true, false));

        assert!(LibraryFilter::All.accepts(false, false));
        assert!(LibraryFilter::Ready.accepts(true, true));
        assert!(!LibraryFilter::Ready.accepts(false, true));
        assert!(!LibraryFilter::Ready.accepts(true, false));
        assert!(LibraryFilter::MissingFlow.accepts(false, true));
        assert!(!LibraryFilter::MissingFlow.accepts(true, false));
        assert!(LibraryFilter::MissingWorker.accepts(true, false));
        assert!(!LibraryFilter::MissingWorker.accepts(false, true));

        assert!(FlowFilter::All.accepts(0));
        assert!(FlowFilter::All.accepts(2));
        assert!(FlowFilter::Used.accepts(1));
        assert!(!FlowFilter::Used.accepts(0));
        assert!(FlowFilter::Unused.accepts(0));
        assert!(!FlowFilter::Unused.accepts(1));
    }

    #[test]
    fn only_jobs_keeps_period_filter_sheet_and_route_change_applies_pending_filter() {
        assert!(has_filters(&Route::Jobs));
        assert!(!has_filters(&Route::Workers));
        assert!(!has_filters(&Route::Plugins));
        assert!(!has_filters(&Route::Flows));
        assert!(!has_filters(&Route::Overview));

        let filters = Filters {
            period: RwSignal::new(Period::Day),
            job_state: RwSignal::new(JobStateFilter::Completed),
            presence: RwSignal::new(Presence::Online),
            plugin_kind: RwSignal::new(PluginKindFilter::Plugins),
            library: RwSignal::new(LibraryFilter::Ready),
            flow: RwSignal::new(FlowFilter::Used),
            pending_job_state: RwSignal::new(Some(JobStateFilter::Failed)),
        };
        filters.apply_route_change();
        assert_eq!(filters.period.get_untracked(), Period::Any);
        assert_eq!(filters.presence.get_untracked(), Presence::Any);
        assert_eq!(filters.library.get_untracked(), LibraryFilter::All);
        assert_eq!(filters.flow.get_untracked(), FlowFilter::All);
        assert_eq!(filters.job_state.get_untracked(), JobStateFilter::Failed);
        assert_eq!(filters.pending_job_state.get_untracked(), None);
    }
}
