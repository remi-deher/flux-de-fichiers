//! Workers « par transfert » d'une bibliothèque : ils traitent ses tâches sans dossier déclaré et
//! reçoivent les sources d'un worker à accès direct (ADR 0021).

use super::draft::LibraryDraft;
use forgearr_shared::workers::Worker;
use leptos::prelude::*;
use uuid::Uuid;

/// Texte d'avertissement quand les workers par transfert sont incompatibles avec la stratégie de sortie.
pub const POLICY_CONFLICT_HELP: &str = "Les workers par transfert exigent le dossier de sortie dédié : changez la stratégie de sortie ou décochez-les.";

/// Résumé du mode d'un worker dans la bibliothèque en cours d'édition.
pub fn mode_label(draft: &LibraryDraft, worker_id: Uuid) -> &'static str {
    if draft.has_direct_folder(worker_id) {
        "Accès direct"
    } else if draft.transfer_workers.contains(&worker_id) {
        "Par transfert"
    } else {
        "Aucun accès"
    }
}

#[component]
pub fn TransferWorkersSection(
    draft: RwSignal<LibraryDraft>,
    workers: Vec<Worker>,
) -> impl IntoView {
    let workers = StoredValue::new(workers);
    view! {
        <div class="inspection-panel__section">
            <p class="inspection-panel__title">"Workers sans accès direct"</p>
            <p class="form-help">
                "Ils traitent les tâches de cette bibliothèque sans avoir son dossier : les fichiers leur sont transférés par un worker à accès direct. Un worker non listé ici et sans dossier n'y a aucun accès."
            </p>
            <ul class="library-worker-list">
                {move || {
                    let current = draft.get();
                    let rows = workers.with_value(|list| list.clone());
                    if rows.is_empty() {
                        return view! { <li class="text-dim text-sm">"Aucun worker enregistré."</li> }.into_any();
                    }
                    rows.into_iter().map(|worker| {
                        let id = worker.id;
                        let direct = current.has_direct_folder(id);
                        let checked = current.transfer_workers.contains(&id);
                        let mode = mode_label(&current, id);
                        view! {
                            <li class="library-worker-row">
                                <label class="cluster">
                                    <input type="checkbox"
                                        disabled=direct
                                        prop:checked=checked
                                        on:change=move |event| draft.update(|value| {
                                            value.set_transfer_worker(id, event_target_checked(&event));
                                        }) />
                                    <span class="text-strong">{worker.name}</span>
                                </label>
                                <span class="library-worker-path">{mode}</span>
                            </li>
                        }
                    }).collect_view().into_any()
                }}
            </ul>
            {move || draft.get().transfer_conflicts_with_policy().then(|| view! {
                <p class="form-help library-access-warning">{POLICY_CONFLICT_HELP}</p>
            })}
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use forgearr_shared::libraries::{LibraryAccess, LibraryOutputPolicy};

    #[test]
    fn mode_label_distinguishes_the_three_modes() {
        let direct = Uuid::from_u128(1);
        let transfer = Uuid::from_u128(2);
        let outsider = Uuid::from_u128(3);
        let mut draft = LibraryDraft::default();
        assert!(
            draft
                .set_or_add_worker(direct, "/mnt/a", LibraryAccess::Read)
                .is_ok()
        );
        draft.set_transfer_worker(transfer, true);
        assert_eq!(mode_label(&draft, direct), "Accès direct");
        assert_eq!(mode_label(&draft, transfer), "Par transfert");
        assert_eq!(mode_label(&draft, outsider), "Aucun accès");
        draft.output_policy = LibraryOutputPolicy::ReplaceInPlace;
        assert!(draft.transfer_conflicts_with_policy());
    }
}
