//! Résultats de la recherche globale : tâches, flux, bibliothèques, workers et plugins,
//! tirés des listes de l'API (contrat `flux_shared::search`) et classés par pertinence.

use super::search::Search;
use crate::route::{Route, use_router};
use flux_shared::flows::FlowRecord;
use flux_shared::jobs::Job;
use flux_shared::libraries::Library;
use flux_shared::plugins::CatalogEntry;
use flux_shared::search::{SearchHit, SearchKind, rank};
use flux_shared::session::Me;
use flux_shared::workers::Worker;
use flux_shared::{InstanceKind, Permission};
use leptos::prelude::*;
use serde::de::DeserializeOwned;

/// Écran où mène un résultat ; un flux ouvre directement son éditeur.
pub fn route_for(hit: &SearchHit) -> Route {
    match hit.kind {
        SearchKind::Job => Route::Jobs,
        SearchKind::Flow => Route::FlowEditor(hit.id.clone()),
        SearchKind::Library => Route::Libraries,
        SearchKind::Worker => Route::Workers,
        SearchKind::Plugin => Route::Plugins,
    }
}

/// Texte de filtre à appliquer à l'écran d'arrivée (l'éditeur d'un flux n'en a pas).
pub fn focus_text(hit: &SearchHit) -> Option<String> {
    (!matches!(route_for(hit), Route::FlowEditor(_))).then(|| hit.title.clone())
}

/// Liste de l'API transformée en résultats, lue à chaque ouverture de la recherche
/// et seulement si l'utilisateur y a droit sur cette installation.
fn hits_of<T: DeserializeOwned + 'static>(
    enabled: bool,
    opened: RwSignal<u32>,
    path: &'static str,
    to_hit: fn(&T) -> SearchHit,
) -> LocalResource<Vec<SearchHit>> {
    LocalResource::new(move || async move {
        if !enabled || opened.get() == 0 {
            return Vec::new();
        }
        crate::api::get::<Vec<T>>(path)
            .await
            .map(|items| items.iter().map(to_hit).collect())
            .unwrap_or_default()
    })
}

/// Menu des résultats, sous le champ de la barre du haut.
#[component]
pub fn SearchResults(me: Me, opened: RwSignal<u32>, focused: RwSignal<bool>) -> impl IntoView {
    let router = use_router();
    let search = use_context::<Search>().unwrap_or_else(Search::install);
    let central = me.instance_kind == InstanceKind::Central;
    let sources = [
        hits_of::<Job>(
            me.allows(Permission::ViewJobs),
            opened,
            "/api/v1/jobs",
            SearchHit::from_job,
        ),
        hits_of::<FlowRecord>(
            central && me.allows(Permission::ViewFlows),
            opened,
            "/api/v1/flows",
            SearchHit::from_flow,
        ),
        hits_of::<Library>(
            central && me.allows(Permission::ViewLibraries),
            opened,
            "/api/v1/libraries",
            SearchHit::from_library,
        ),
        hits_of::<Worker>(
            central && me.allows(Permission::ViewWorkers),
            opened,
            "/api/v1/workers",
            SearchHit::from_worker,
        ),
        hits_of::<CatalogEntry>(
            me.allows(Permission::ViewPlugins),
            opened,
            "/api/v1/plugins",
            SearchHit::from_plugin,
        ),
    ];
    let found = Memo::new(move |_| {
        let all = sources
            .iter()
            .flat_map(|source| source.get().unwrap_or_default())
            .collect();
        rank(all, &search.text.get())
    });
    let open_hit = move |hit: SearchHit| {
        search.pending.set(focus_text(&hit));
        focused.set(false);
        router.go(route_for(&hit));
    };
    // Entrée ouvre le meilleur résultat.
    let _enter = window_event_listener(leptos::ev::keydown, move |event| {
        if event.key() == "Enter"
            && focused.get_untracked()
            && let Some(hit) = found.get_untracked().into_iter().next()
        {
            open_hit(hit);
        }
    });
    view! {
        <div class="search-results" role="listbox" aria-label="Résultats de la recherche"
            hidden=move || !focused.get() || found.get().is_empty()
            on:mousedown=|event| event.prevent_default()>
            {move || found.get().into_iter().map(|hit| {
                let target = hit.clone();
                view! {
                    <button type="button" role="option" class="search-result"
                        on:click=move |_| open_hit(target.clone())>
                        <span class="search-result__kind">{hit.kind.label()}</span>
                        <span class="search-result__title">{hit.title}</span>
                        <span class="search-result__detail">{hit.detail}</span>
                    </button>
                }
            }).collect_view()}
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hit(kind: SearchKind) -> SearchHit {
        SearchHit {
            kind,
            id: "abc".into(),
            title: "Films".into(),
            detail: String::new(),
        }
    }

    #[test]
    fn a_flow_opens_its_editor_and_the_others_filter_their_screen() {
        assert_eq!(
            route_for(&hit(SearchKind::Flow)),
            Route::FlowEditor("abc".into())
        );
        assert_eq!(focus_text(&hit(SearchKind::Flow)), None);
        for (kind, route) in [
            (SearchKind::Job, Route::Jobs),
            (SearchKind::Library, Route::Libraries),
            (SearchKind::Worker, Route::Workers),
            (SearchKind::Plugin, Route::Plugins),
        ] {
            assert_eq!(route_for(&hit(kind)), route);
            assert_eq!(focus_text(&hit(kind)).as_deref(), Some("Films"));
        }
    }
}
