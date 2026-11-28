//! Rattachement d'un flux à des bibliothèques, depuis l'éditeur : une bibliothèque porte un seul flux.

use super::flow_choice::flow_options;
use crate::api;
use crate::route::{Link, Route};
use crate::ui::{Toasts, vocabulary};
use forgearr_shared::libraries::{Library, LibraryWrite};
use leptos::prelude::*;
use std::collections::HashSet;
use uuid::Uuid;

const LIBRARIES_PATH: &str = "/api/v1/libraries";

/// Changement à écrire sur une bibliothèque.
#[derive(Clone, Debug, PartialEq)]
pub struct BindingChange {
    pub library: Library,
    /// Flux à porter après le changement ; `None` : la bibliothèque n'en a plus.
    pub flow_id: Option<String>,
}

/// Corps d'écriture d'une bibliothèque qui garde tous ses réglages et ne change que son flux.
pub fn write_for_flow(library: &Library, flow_id: Option<&str>) -> LibraryWrite {
    LibraryWrite {
        name: library.name.clone(),
        flow_id: flow_id.map(str::to_owned),
        workers: library.workers.clone(),
        transfer_workers: library.transfer_workers.clone(),
        scan_interval_minutes: library.scan_interval_minutes,
        include_extensions: library.include_extensions.clone(),
        exclude_patterns: library.exclude_patterns.clone(),
        min_file_bytes: library.min_file_bytes,
        output_policy: library.output_policy,
        default_priority: library.default_priority,
    }
}

/// Bibliothèques qui portent déjà ce flux.
pub fn bound_to(libraries: &[Library], flow_id: &str) -> HashSet<Uuid> {
    libraries
        .iter()
        .filter(|library| library.flow_id.as_deref() == Some(flow_id))
        .map(|library| library.id)
        .collect()
}

/// Changements qui amènent les bibliothèques à l'état choisi : les cochées portent ce flux, les
/// décochées qui le portaient n'en ont plus. Les autres ne bougent pas.
pub fn plan_changes(
    libraries: &[Library],
    flow_id: &str,
    selected: &HashSet<Uuid>,
) -> Vec<BindingChange> {
    libraries
        .iter()
        .filter_map(|library| {
            let bound = library.flow_id.as_deref() == Some(flow_id);
            match (selected.contains(&library.id), bound) {
                (true, false) => Some(BindingChange {
                    library: library.clone(),
                    flow_id: Some(flow_id.to_owned()),
                }),
                (false, true) => Some(BindingChange {
                    library: library.clone(),
                    flow_id: None,
                }),
                _ => None,
            }
        })
        .collect()
}

/// Flux qu'une bibliothèque porte et que cocher cette case remplacerait.
pub fn replaced_flow<'a>(library: &'a Library, flow_id: &str) -> Option<&'a str> {
    library
        .flow_id
        .as_deref()
        .filter(|current| *current != flow_id)
}

pub fn updated_label(count: usize) -> String {
    match count {
        1 => "1 bibliothèque mise à jour.".to_owned(),
        count => format!("{count} bibliothèques mises à jour."),
    }
}

/// Cases à cocher des bibliothèques ; `on_changed` demande de relire la liste après l'écriture.
#[component]
pub fn LibraryBinding(
    flow_id: String,
    libraries: Signal<Vec<Library>>,
    can_bind: bool,
    on_changed: Callback<()>,
) -> impl IntoView {
    let flow_id = StoredValue::new(flow_id);
    let names = flow_options("", None);
    let selected = RwSignal::new(HashSet::<Uuid>::new());
    Effect::new(move |_| {
        selected.set(bound_to(&libraries.get(), &flow_id.get_value()));
    });
    let changes =
        Memo::new(move |_| plan_changes(&libraries.get(), &flow_id.get_value(), &selected.get()));

    let apply = Action::new_local(move |changes: &Vec<BindingChange>| {
        let changes = changes.clone();
        async move {
            let mut done = 0_usize;
            for change in changes {
                let body = write_for_flow(&change.library, change.flow_id.as_deref());
                let path = format!("{LIBRARIES_PATH}/{}", change.library.id);
                api::put::<LibraryWrite, Library>(&path, &body)
                    .await
                    .map_err(|error| error.message())?;
                done += 1;
            }
            Ok::<usize, String>(done)
        }
    });
    Effect::new(move |_| match apply.value().get() {
        Some(Ok(count)) => {
            Toasts::notify_success(updated_label(count));
            on_changed.run(());
        }
        Some(Err(message)) => {
            Toasts::notify_error(message);
            on_changed.run(());
        }
        None => {}
    });

    let flow_name = move |id: &str| {
        names
            .get()
            .into_iter()
            .find(|(candidate, _)| candidate == id)
            .map_or_else(|| id.to_owned(), |(_, name)| name)
    };

    view! {
        <div class="flow-inspector__section">
            <h3 class="flow-inspector__section-title">{vocabulary::LIBRARIES}</h3>
            <p class="form-help">
                "Cochez les bibliothèques dont les fichiers passeront par ce flux. Une bibliothèque n'a qu'un flux."
            </p>
            {move || libraries.with(Vec::is_empty).then(|| view! {
                <p class="form-help">"Aucune bibliothèque pour l'instant."</p>
            })}
            {move || libraries.get().into_iter().map(|library| {
                let id = library.id;
                let input_id = format!("flow-library-{id}");
                let label_for = input_id.clone();
                let replaced = replaced_flow(&library, &flow_id.get_value()).map(str::to_owned);
                view! {
                    <div class="form-group">
                        <label class="flow-settings__toggle" for=label_for>
                            <input
                                id=input_id
                                type="checkbox"
                                disabled=!can_bind
                                prop:checked=move || selected.with(|set| set.contains(&id))
                                on:change=move |event| {
                                    let checked = event_target_checked(&event);
                                    selected.update(|set| {
                                        if checked {
                                            set.insert(id);
                                        } else {
                                            set.remove(&id);
                                        }
                                    });
                                }
                            />
                            <span>{library.name.clone()}</span>
                        </label>
                        {replaced.map(|current| view! {
                            <p class="form-help">
                                {move || if selected.with(|set| set.contains(&id)) {
                                    format!("Remplace son flux actuel : {}.", flow_name(&current))
                                } else {
                                    format!("Flux actuel : {}.", flow_name(&current))
                                }}
                            </p>
                        })}
                    </div>
                }
            }).collect_view()}
            <div class="inspection-drawer__actions">
                <button type="button" class="btn btn-primary btn-sm"
                    disabled=move || !can_bind || changes.with(Vec::is_empty) || apply.pending().get()
                    on:click=move |_| { apply.dispatch(changes.get_untracked()); }>
                    "Rattacher"
                </button>
                <Link to=Route::Libraries class="btn btn-secondary btn-sm">
                    "Gérer les bibliothèques"
                </Link>
            </div>
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::DateTime;
    use forgearr_shared::libraries::LibraryOutputPolicy;
    use std::sync::atomic::{AtomicU64, Ordering};

    fn unique_id() -> Uuid {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        Uuid::from_u128(u128::from(NEXT.fetch_add(1, Ordering::Relaxed)))
    }

    fn library(flow_id: Option<&str>) -> Library {
        Library {
            id: unique_id(),
            name: "Films".into(),
            flow_id: flow_id.map(str::to_owned),
            workers: Vec::new(),
            transfer_workers: vec![unique_id()],
            scan_interval_minutes: 30,
            last_scanned_at: None,
            include_extensions: vec!["mkv".into()],
            exclude_patterns: vec!["*.tmp".into()],
            min_file_bytes: 1_500_000,
            output_policy: LibraryOutputPolicy::default(),
            default_priority: 3,
            created_at: DateTime::UNIX_EPOCH,
            updated_at: DateTime::UNIX_EPOCH,
        }
    }

    #[test]
    fn writing_changes_only_the_flow() {
        let source = library(Some("flow.a"));
        let body = write_for_flow(&source, Some("flow.b"));
        assert_eq!(body.flow_id.as_deref(), Some("flow.b"));
        assert_eq!(body.name, source.name);
        assert_eq!(body.transfer_workers, source.transfer_workers);
        assert_eq!(body.scan_interval_minutes, 30);
        assert_eq!(body.include_extensions, source.include_extensions);
        assert_eq!(body.exclude_patterns, source.exclude_patterns);
        assert_eq!(body.min_file_bytes, 1_500_000);
        assert_eq!(body.default_priority, 3);
        assert_eq!(write_for_flow(&source, None).flow_id, None);
    }

    #[test]
    fn bound_to_lists_the_libraries_that_carry_the_flow() {
        let mine = library(Some("flow.a"));
        let other = library(Some("flow.b"));
        let free = library(None);
        let ids = bound_to(&[mine.clone(), other, free], "flow.a");
        assert_eq!(ids, HashSet::from([mine.id]));
    }

    #[test]
    fn plan_binds_checked_libraries_and_unbinds_unchecked_ones() {
        let keep = library(Some("flow.a"));
        let drop = library(Some("flow.a"));
        let take_free = library(None);
        let take_other = library(Some("flow.b"));
        let untouched = library(Some("flow.b"));
        let selected = HashSet::from([keep.id, take_free.id, take_other.id]);
        let libraries = [
            keep,
            drop.clone(),
            take_free.clone(),
            take_other.clone(),
            untouched,
        ];
        let changes = plan_changes(&libraries, "flow.a", &selected);
        let summary: Vec<(Uuid, Option<&str>)> = changes
            .iter()
            .map(|change| (change.library.id, change.flow_id.as_deref()))
            .collect();
        assert_eq!(
            summary,
            vec![
                (drop.id, None),
                (take_free.id, Some("flow.a")),
                (take_other.id, Some("flow.a")),
            ]
        );
    }

    #[test]
    fn no_change_when_the_selection_matches() {
        let mine = library(Some("flow.a"));
        let selected = bound_to(std::slice::from_ref(&mine), "flow.a");
        assert!(plan_changes(&[mine, library(None)], "flow.a", &selected).is_empty());
    }

    #[test]
    fn a_replaced_flow_is_another_flow_only() {
        assert_eq!(
            replaced_flow(&library(Some("flow.b")), "flow.a"),
            Some("flow.b")
        );
        assert_eq!(replaced_flow(&library(Some("flow.a")), "flow.a"), None);
        assert_eq!(replaced_flow(&library(None), "flow.a"), None);
    }

    #[test]
    fn the_confirmation_agrees_in_number() {
        assert_eq!(updated_label(1), "1 bibliothèque mise à jour.");
        assert_eq!(updated_label(3), "3 bibliothèques mises à jour.");
    }
}
