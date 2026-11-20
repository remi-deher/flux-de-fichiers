//! Bibliothèques d'un worker et son mode d'accès pour chacune (ADR 0021).

use crate::ui::{Tag, TagTone};
use forgearr_shared::libraries::{Library, has_unshared_remote_sources};
use leptos::prelude::*;
use uuid::Uuid;

/// Mode d'accès d'un worker à une bibliothèque ; l'absence de mode est « aucun accès ».
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LibraryMode {
    /// Le worker a un dossier déclaré dans la bibliothèque.
    Direct,
    /// Le worker traite les tâches de la bibliothèque et reçoit les sources par transfert.
    Transfer,
}

impl LibraryMode {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Direct => "Accès direct",
            Self::Transfer => "Par transfert",
        }
    }

    const fn tone(self) -> TagTone {
        match self {
            Self::Direct => TagTone::Success,
            Self::Transfer => TagTone::Primary,
        }
    }
}

/// Une bibliothèque vue depuis un worker.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkerLibrary {
    pub name: String,
    pub mode: LibraryMode,
    pub unshared_sources: bool,
}

/// Bibliothèques où le worker a un mode, avec ce mode. Le worker intégré a l'accès direct aux
/// dossiers de la centrale (identifiant nul), comme dans `claim`.
pub fn worker_libraries(
    libraries: &[Library],
    worker_id: Uuid,
    integrated: bool,
) -> Vec<WorkerLibrary> {
    libraries
        .iter()
        .filter_map(|library| {
            let direct = library.workers.iter().any(|folder| {
                folder.worker_id == worker_id || (integrated && folder.worker_id.is_nil())
            });
            let mode = if direct {
                LibraryMode::Direct
            } else if library.transfer_workers.contains(&worker_id) {
                LibraryMode::Transfer
            } else {
                return None;
            };
            Some(WorkerLibrary {
                name: library.name.clone(),
                mode,
                unshared_sources: mode == LibraryMode::Transfer
                    && has_unshared_remote_sources(&library.workers),
            })
        })
        .collect()
}

#[component]
pub fn WorkerLibrariesSection(libraries: Vec<WorkerLibrary>) -> impl IntoView {
    view! {
        <div class="stack">
            <h3 class="flow-inspector__section-title">"Bibliothèques"</h3>
            {if libraries.is_empty() {
                view! {
                    <p class="form-help">"Aucune bibliothèque n'utilise ce worker : il n'y a aucun accès."</p>
                }.into_any()
            } else {
                view! {
                    <ul class="library-worker-list">
                        {libraries.into_iter().map(|library| view! {
                            <li class="library-worker-row">
                                <span class="text-strong">{library.name}</span>
                                <Tag label=library.mode.label() tone=library.mode.tone() />
                                {library.unshared_sources.then(|| view! {
                                    <Tag label="Sources non partagées" tone=TagTone::Warning />
                                })}
                            </li>
                        }).collect_view()}
                    </ul>
                }.into_any()
            }}
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use forgearr_shared::libraries::{LibraryAccess, LibraryOutputPolicy, LibraryWorkerPath};

    fn library(name: &str, direct: &[Uuid], transfer: &[Uuid]) -> Library {
        Library {
            id: Uuid::nil(),
            name: name.into(),
            flow_id: None,
            workers: direct
                .iter()
                .map(|worker_id| LibraryWorkerPath {
                    source_key: None,
                    worker_id: *worker_id,
                    local_path: "/mnt/a".into(),
                    access: LibraryAccess::Read,
                })
                .collect(),
            transfer_workers: transfer.to_vec(),
            scan_interval_minutes: 0,
            last_scanned_at: None,
            include_extensions: Vec::new(),
            exclude_patterns: Vec::new(),
            min_file_bytes: 0,
            output_policy: LibraryOutputPolicy::OutputDir,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn a_worker_sees_its_libraries_with_the_right_mode() {
        let me = Uuid::from_u128(1);
        let other = Uuid::from_u128(2);
        let libraries = [
            library("Films", &[me], &[]),
            library("Séries", &[other], &[me]),
            library("Musique", &[other], &[]),
        ];
        assert_eq!(
            worker_libraries(&libraries, me, false),
            vec![
                WorkerLibrary {
                    name: "Films".into(),
                    mode: LibraryMode::Direct,
                    unshared_sources: false,
                },
                WorkerLibrary {
                    name: "Séries".into(),
                    mode: LibraryMode::Transfer,
                    unshared_sources: true,
                },
            ]
        );
    }

    #[test]
    fn only_the_integrated_worker_has_direct_access_to_central_folders() {
        let integrated = Uuid::from_u128(1);
        let remote = Uuid::from_u128(2);
        let libraries = [library("Centrale", &[Uuid::nil()], &[remote])];
        assert_eq!(
            worker_libraries(&libraries, integrated, true)[0].mode,
            LibraryMode::Direct
        );
        assert_eq!(
            worker_libraries(&libraries, remote, false)[0].mode,
            LibraryMode::Transfer
        );
        assert!(worker_libraries(&libraries, Uuid::from_u128(3), false).is_empty());
    }
}
