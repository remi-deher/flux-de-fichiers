//! Recherche globale de la barre du haut : un résultat typé par objet, classé par pertinence.
//!
//! Le client charge les listes déjà exposées par l'API (tâches, flux, bibliothèques, workers,
//! plugins), en fait des `SearchHit` puis les filtre avec `rank`. Aucune route dédiée.

use crate::flows::FlowRecord;
use crate::jobs::Job;
use crate::libraries::Library;
use crate::plugins::CatalogEntry;
use crate::workers::Worker;
use serde::{Deserialize, Serialize};

/// Nombre maximal de résultats affichés.
pub const MAX_SEARCH_HITS: usize = 12;

/// Nature de l'objet trouvé : décide de l'écran où mène le résultat.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SearchKind {
    Job,
    Flow,
    Library,
    Worker,
    Plugin,
}

impl SearchKind {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Job => "Tâche",
            Self::Flow => "Flux",
            Self::Library => "Bibliothèque",
            Self::Worker => "Worker",
            Self::Plugin => "Plugin",
        }
    }
}

/// Objet trouvé : son titre est ce que l'écran d'arrivée filtre.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchHit {
    pub kind: SearchKind,
    pub id: String,
    pub title: String,
    pub detail: String,
}

impl SearchHit {
    fn new(kind: SearchKind, id: String, title: &str, detail: String) -> Self {
        Self {
            kind,
            id,
            title: title.to_owned(),
            detail,
        }
    }

    pub fn from_job(job: &Job) -> Self {
        Self::new(
            SearchKind::Job,
            job.id.to_string(),
            &job.source_name,
            job.status.to_string(),
        )
    }

    pub fn from_flow(flow: &FlowRecord) -> Self {
        Self::new(
            SearchKind::Flow,
            flow.id.clone(),
            &flow.name,
            flow.description.clone().unwrap_or_default(),
        )
    }

    pub fn from_library(library: &Library) -> Self {
        Self::new(
            SearchKind::Library,
            library.id.to_string(),
            &library.name,
            library.flow_id.clone().unwrap_or_default(),
        )
    }

    pub fn from_worker(worker: &Worker) -> Self {
        Self::new(
            SearchKind::Worker,
            worker.id.to_string(),
            &worker.name,
            String::new(),
        )
    }

    pub fn from_plugin(entry: &CatalogEntry) -> Self {
        Self::new(
            SearchKind::Plugin,
            entry.id.clone(),
            &entry.name,
            entry.tags.join(" "),
        )
    }

    /// Rang de pertinence : 0 titre commençant par le texte, 1 titre le contenant,
    /// 2 identifiant ou détail le contenant ; `None` si rien ne correspond.
    fn rank(&self, term: &str) -> Option<u8> {
        let title = self.title.to_lowercase();
        if title.starts_with(term) {
            Some(0)
        } else if title.contains(term) {
            Some(1)
        } else if self.id.to_lowercase().contains(term) || self.detail.to_lowercase().contains(term)
        {
            Some(2)
        } else {
            None
        }
    }
}

/// Résultats pour ce texte, du plus pertinent au moins pertinent, au plus `MAX_SEARCH_HITS`.
/// Un texte vide ne donne rien : sans saisie, il n'y a pas de menu.
pub fn rank(hits: Vec<SearchHit>, query: &str) -> Vec<SearchHit> {
    let term = query.trim().to_lowercase();
    if term.is_empty() {
        return Vec::new();
    }
    let mut ranked: Vec<(u8, SearchHit)> = hits
        .into_iter()
        .filter_map(|hit| hit.rank(&term).map(|rank| (rank, hit)))
        .collect();
    ranked.sort_by_key(|(rank, _)| *rank);
    ranked
        .into_iter()
        .take(MAX_SEARCH_HITS)
        .map(|(_, hit)| hit)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hit(kind: SearchKind, id: &str, title: &str, detail: &str) -> SearchHit {
        SearchHit::new(kind, id.into(), title, detail.into())
    }

    #[test]
    fn an_empty_query_gives_no_result() {
        let hits = vec![hit(SearchKind::Flow, "f", "Films", "")];
        assert!(rank(hits, "  ").is_empty());
    }

    #[test]
    fn title_prefix_comes_before_title_substring_before_id_or_detail() {
        let hits = vec![
            hit(SearchKind::Plugin, "video", "Encodeur", "film"),
            hit(SearchKind::Job, "j", "Mon film.mkv", "failed"),
            hit(SearchKind::Flow, "f", "FILM 4K", ""),
        ];
        let titles: Vec<String> = rank(hits, "film")
            .into_iter()
            .map(|found| found.title)
            .collect();
        assert_eq!(titles, ["FILM 4K", "Mon film.mkv", "Encodeur"]);
    }

    #[test]
    fn unmatched_hits_are_dropped_and_the_list_is_capped() {
        let mut hits = vec![hit(SearchKind::Worker, "w", "nas", "")];
        hits.extend((0..MAX_SEARCH_HITS + 5).map(|index| {
            hit(
                SearchKind::Job,
                &index.to_string(),
                &format!("film {index}"),
                "",
            )
        }));
        let found = rank(hits, "film");
        assert_eq!(found.len(), MAX_SEARCH_HITS);
        assert!(found.iter().all(|found| found.kind == SearchKind::Job));
    }

    #[test]
    fn hits_serialize_with_a_snake_case_kind() {
        let json =
            serde_json::to_string(&hit(SearchKind::Library, "l", "Séries", "")).unwrap_or_default();
        assert!(json.contains("\"kind\":\"library\""));
    }

    #[test]
    fn every_kind_has_a_label() {
        for kind in [
            SearchKind::Job,
            SearchKind::Flow,
            SearchKind::Library,
            SearchKind::Worker,
            SearchKind::Plugin,
        ] {
            assert!(!kind.label().is_empty());
        }
    }
}
