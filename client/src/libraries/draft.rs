//! Brouillon d'une bibliothèque en cours d'édition, sans DOM.

use flux_shared::libraries::{
    LIBRARY_NAME_MAX, Library, LibraryAccess, LibraryWorkerPath, LibraryWrite,
};
use uuid::Uuid;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct LibraryDraft {
    /// `None` : bibliothèque nouvelle.
    pub id: Option<Uuid>,
    pub name: String,
    pub flow_id: Option<String>,
    pub workers: Vec<LibraryWorkerPath>,
}

impl LibraryDraft {
    pub fn editing(library: &Library) -> Self {
        Self {
            id: Some(library.id),
            name: library.name.clone(),
            flow_id: library.flow_id.clone(),
            workers: library.workers.clone(),
        }
    }

    /// Ajoute le chemin d'un worker ; refuse un worker déjà présent ou un chemin non absolu.
    pub fn add_worker(
        &mut self,
        worker_id: Uuid,
        path: &str,
        access: LibraryAccess,
    ) -> Result<(), &'static str> {
        let path = path.trim();
        if !path.starts_with('/') {
            return Err("Le chemin doit être absolu (commencer par /).");
        }
        if self
            .workers
            .iter()
            .any(|entry| entry.worker_id == worker_id)
        {
            return Err("Ce worker a déjà un chemin pour cette bibliothèque.");
        }
        self.workers.push(LibraryWorkerPath {
            worker_id,
            local_path: path.to_owned(),
            access,
        });
        Ok(())
    }

    pub fn remove_worker(&mut self, worker_id: Uuid) {
        self.workers.retain(|entry| entry.worker_id != worker_id);
    }

    /// Corps envoyé au central, ou la première erreur de saisie.
    pub fn to_write(&self) -> Result<LibraryWrite, &'static str> {
        let name = self.name.trim();
        if name.is_empty() || name.chars().count() > LIBRARY_NAME_MAX {
            return Err("Le nom doit compter de 1 à 80 caractères.");
        }
        Ok(LibraryWrite {
            name: name.to_owned(),
            flow_id: self.flow_id.clone().filter(|id| !id.is_empty()),
            workers: self.workers.clone(),
        })
    }
}

/// Prête à traiter : un flux et au moins un worker.
pub fn is_ready(library: &Library) -> bool {
    library.flow_id.is_some() && !library.workers.is_empty()
}

/// Compteurs affichés dans le bandeau `StatStrip` des bibliothèques.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LibraryCounts {
    pub total: usize,
    pub ready: usize,
    pub missing_flow: usize,
    pub missing_worker: usize,
}

/// Synthèse pour le bandeau d'en-tête des bibliothèques.
pub fn library_counts(libraries: &[Library]) -> LibraryCounts {
    let mut counts = LibraryCounts {
        total: libraries.len(),
        ..LibraryCounts::default()
    };
    for library in libraries {
        let has_flow = library.flow_id.is_some();
        let has_worker = !library.workers.is_empty();
        if has_flow && has_worker {
            counts.ready += 1;
        }
        if !has_flow {
            counts.missing_flow += 1;
        }
        if !has_worker {
            counts.missing_worker += 1;
        }
    }
    counts
}

pub fn access_label(access: LibraryAccess) -> &'static str {
    match access {
        LibraryAccess::Read => "Lecture",
        LibraryAccess::ReadWrite => "Lecture et écriture",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adding_a_worker_checks_the_path_and_duplicates() {
        let mut draft = LibraryDraft::default();
        let worker = Uuid::nil();
        assert!(
            draft
                .add_worker(worker, "media", LibraryAccess::Read)
                .is_err()
        );
        assert_eq!(
            draft.add_worker(worker, " /mnt/films ", LibraryAccess::Read),
            Ok(())
        );
        assert_eq!(draft.workers[0].local_path, "/mnt/films");
        assert!(
            draft
                .add_worker(worker, "/autre", LibraryAccess::ReadWrite)
                .is_err()
        );
        draft.remove_worker(worker);
        assert!(draft.workers.is_empty());
    }

    #[test]
    fn the_written_body_trims_the_name_and_drops_an_empty_flow() {
        let draft = LibraryDraft {
            name: "  Films ".into(),
            flow_id: Some(String::new()),
            ..LibraryDraft::default()
        };
        let write = draft.to_write();
        assert_eq!(write.as_ref().map(|body| body.name.as_str()), Ok("Films"));
        assert_eq!(write.map(|body| body.flow_id), Ok(None));
        assert!(LibraryDraft::default().to_write().is_err());
        let long = LibraryDraft {
            name: "x".repeat(LIBRARY_NAME_MAX + 1),
            ..LibraryDraft::default()
        };
        assert!(long.to_write().is_err());
    }

    #[test]
    fn library_counts_tracks_ready_and_flow_bound_libraries() {
        let now = chrono::Utc::now();
        let ready = Library {
            id: Uuid::nil(),
            name: "Films".into(),
            flow_id: Some("remux".into()),
            workers: vec![LibraryWorkerPath {
                worker_id: Uuid::nil(),
                local_path: "/media/films".into(),
                access: LibraryAccess::ReadWrite,
            }],
            created_at: now,
            updated_at: now,
        };
        let flow_only = Library {
            id: Uuid::nil(),
            name: "Séries".into(),
            flow_id: Some("remux".into()),
            workers: Vec::new(),
            created_at: now,
            updated_at: now,
        };
        let empty = Library {
            id: Uuid::nil(),
            name: "Archives".into(),
            flow_id: None,
            workers: Vec::new(),
            created_at: now,
            updated_at: now,
        };
        assert_eq!(library_counts(&[]), LibraryCounts::default());
        assert_eq!(
            library_counts(&[ready, flow_only, empty]),
            LibraryCounts {
                total: 3,
                ready: 1,
                missing_flow: 1,
                missing_worker: 2,
            }
        );
    }
}
