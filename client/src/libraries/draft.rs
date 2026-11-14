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
}
