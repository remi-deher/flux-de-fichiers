//! Sélecteur d'arborescence de dossiers et fichiers médias sur un worker.

use super::icon::{Icon, IconName};
use crate::api::{self, ApiError};
use crate::route::encode_segment;
use forgearr_shared::format::format_bytes;
use forgearr_shared::libraries::{DirectoryEntry, DirectoryListing, MediaFileEntry, RootShortcut};
use leptos::prelude::*;
use std::collections::{BTreeMap, BTreeSet};
use uuid::Uuid;

const TREE_ICON_SIZE: u32 = 15;
const CHEVRON_ICON_SIZE: u32 = 13;
const ACTION_ICON_SIZE: u32 = 14;

/// Type d'entrée dans la vue aplatie de l'arborescence.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TreeRowKind {
    Directory(DirectoryEntry),
    MediaFile(MediaFileEntry),
}

/// Ligne affichée dans l'arborescence aplatie, avec sa profondeur d'indentation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TreeRow {
    pub depth: usize,
    pub kind: TreeRowKind,
}

fn append_listing(
    path: &str,
    depth: usize,
    listings: &BTreeMap<String, DirectoryListing>,
    expanded: &BTreeSet<String>,
    visited: &mut BTreeSet<String>,
    rows: &mut Vec<TreeRow>,
) {
    if !visited.insert(path.to_owned()) {
        return;
    }
    let Some(listing) = listings.get(path) else {
        return;
    };
    for dir in &listing.directories {
        rows.push(TreeRow {
            depth,
            kind: TreeRowKind::Directory(dir.clone()),
        });
        if expanded.contains(&dir.path) {
            append_listing(&dir.path, depth + 1, listings, expanded, visited, rows);
        }
    }
    for file in &listing.files {
        rows.push(TreeRow {
            depth,
            kind: TreeRowKind::MediaFile(file.clone()),
        });
    }
}

/// Aplatit l'arborescence à partir de `root_path` selon les dossiers dépliés.
pub fn flatten_tree(
    root_path: &str,
    listings: &BTreeMap<String, DirectoryListing>,
    expanded: &BTreeSet<String>,
) -> Vec<TreeRow> {
    let mut rows = Vec::new();
    let mut visited = BTreeSet::new();
    append_listing(root_path, 0, listings, expanded, &mut visited, &mut rows);
    rows
}

/// Découpe un chemin absolu en segments cliquables `(libellé, chemin complet)`.
pub fn breadcrumbs_for_path(path: &str) -> Vec<(String, String)> {
    let trimmed = path.trim();
    if !trimmed.starts_with('/') {
        return Vec::new();
    }
    let mut crumbs = vec![("/".to_owned(), "/".to_owned())];
    let mut current = String::new();
    for segment in trimmed.split('/').filter(|part| !part.is_empty()) {
        current.push('/');
        current.push_str(segment);
        crumbs.push((segment.to_owned(), current.clone()));
    }
    crumbs
}

fn browse_url(worker_id: Uuid, path: Option<&str>) -> String {
    match path.map(str::trim).filter(|value| !value.is_empty()) {
        Some(path) => format!(
            "/api/v1/libraries/browse?workerId={worker_id}&path={}",
            encode_segment(path)
        ),
        None => format!("/api/v1/libraries/browse?workerId={worker_id}"),
    }
}

fn describe_browse_error(err: &ApiError) -> String {
    match err {
        ApiError::Rejected { status: 502, .. } => {
            "Worker hors ligne ou injoignable. Saisissez le chemin manuellement ci-dessus.".into()
        }
        ApiError::Rejected { status: 404, .. } => "Dossier introuvable sur ce worker.".into(),
        ApiError::Rejected { status: 400, .. } => {
            "Chemin invalide ou inaccessible sur ce worker.".into()
        }
        other => other.message(),
    }
}

fn access_badge_label(writable: bool) -> &'static str {
    if writable { "L/É" } else { "Lecture seule" }
}

/// Explorateur d'arborescence (dossiers et fichiers médias) pour choisir la racine d'une bibliothèque.
#[component]
pub fn FolderTreePicker(
    #[prop(into)] worker_id: Signal<Option<Uuid>>,
    selected_path: RwSignal<String>,
    on_select: Callback<(String, bool)>,
    #[prop(optional)] navigate_tick: Option<Signal<u32>>,
) -> impl IntoView {
    let root_path = RwSignal::new(String::new());
    let listings = RwSignal::new(BTreeMap::<String, DirectoryListing>::new());
    let latest_roots = RwSignal::new(Vec::<RootShortcut>::new());
    let expanded = RwSignal::new(BTreeSet::<String>::new());
    let loading_root = RwSignal::new(false);
    let loading_paths = RwSignal::new(BTreeSet::<String>::new());
    let error_msg = RwSignal::new(None::<String>);
    let fetch_seq = RwSignal::new(0_u64);

    let open_root = move |wid: Uuid, target: Option<String>, select_on_load: bool| {
        let seq = fetch_seq.get_untracked().wrapping_add(1);
        fetch_seq.set(seq);
        loading_root.set(true);
        error_msg.set(None);
        let url = browse_url(wid, target.as_deref());
        leptos::task::spawn_local(async move {
            let result = api::get::<DirectoryListing>(&url).await;
            if fetch_seq.get_untracked() != seq {
                return;
            }
            loading_root.set(false);
            match result {
                Ok(listing) => {
                    let loaded_path = listing.path.clone();
                    let loaded_writable = listing.writable;
                    latest_roots.set(listing.roots.clone());
                    root_path.set(loaded_path.clone());
                    expanded.update(BTreeSet::clear);
                    loading_paths.update(BTreeSet::clear);
                    listings.update(|map| {
                        map.clear();
                        map.insert(loaded_path.clone(), listing);
                    });
                    if select_on_load {
                        on_select.run((loaded_path, loaded_writable));
                    }
                }
                Err(err) => error_msg.set(Some(describe_browse_error(&err))),
            }
        });
    };

    let fetch_subdir = move |wid: Uuid, dir_path: String| {
        if listings.get_untracked().contains_key(&dir_path)
            || loading_paths.get_untracked().contains(&dir_path)
        {
            return;
        }
        let seq = fetch_seq.get_untracked();
        loading_paths.update(|set| {
            set.insert(dir_path.clone());
        });
        let url = browse_url(wid, Some(&dir_path));
        leptos::task::spawn_local(async move {
            let result = api::get::<DirectoryListing>(&url).await;
            if fetch_seq.get_untracked() != seq {
                return;
            }
            loading_paths.update(|set| {
                set.remove(&dir_path);
            });
            match result {
                Ok(listing) => {
                    listings.update(|map| {
                        map.insert(dir_path, listing);
                    });
                }
                Err(err) => {
                    expanded.update(|set| {
                        set.remove(&dir_path);
                    });
                    error_msg.set(Some(describe_browse_error(&err)));
                }
            }
        });
    };

    let toggle_expand = move |wid: Uuid, dir_path: String| {
        let is_open = expanded.get_untracked().contains(&dir_path);
        if is_open {
            expanded.update(|set| {
                set.remove(&dir_path);
            });
        } else {
            expanded.update(|set| {
                set.insert(dir_path.clone());
            });
            fetch_subdir(wid, dir_path);
        }
    };

    Effect::new(move |_| {
        let current_worker = worker_id.get();
        if let Some(tick) = navigate_tick {
            tick.track();
        }
        listings.update(BTreeMap::clear);
        latest_roots.update(Vec::clear);
        expanded.update(BTreeSet::clear);
        loading_paths.update(BTreeSet::clear);
        error_msg.set(None);
        let Some(wid) = current_worker else {
            root_path.set(String::new());
            loading_root.set(false);
            return;
        };
        let typed = selected_path.get_untracked();
        let initial = typed
            .trim()
            .starts_with('/')
            .then(|| typed.trim().to_owned());
        open_root(wid, initial, false);
    });

    let rows = Memo::new(move |_| {
        let root = root_path.get();
        if root.is_empty() {
            return Vec::new();
        }
        let map = listings.get();
        let exp = expanded.get();
        flatten_tree(&root, &map, &exp)
    });

    view! {
        <div class="folder-picker" aria-label="Explorateur de dossiers et médias">
            {move || {
                let Some(wid) = worker_id.get() else {
                    return view! {
                        <p class="folder-picker__empty">
                            "Sélectionnez un worker pour parcourir ses dossiers et fichiers médias."
                        </p>
                    }.into_any();
                };

                let roots_list = latest_roots.get();
                let current_root = root_path.get();
                let current_listing_writable = listings
                    .get()
                    .get(&current_root)
                    .map(|item| item.writable)
                    .unwrap_or(true);
                let crumbs = breadcrumbs_for_path(&current_root);
                let is_loading = loading_root.get();
                let current_error = error_msg.get();
                let tree_rows = rows.get();
                let current_selected = selected_path.get();
                let exp_set = expanded.get();
                let loading_set = loading_paths.get();

                view! {
                    {(!roots_list.is_empty()).then(|| view! {
                        <div class="folder-picker__roots" role="group" aria-label="Raccourcis de volumes">
                            {roots_list.into_iter().map(|root| {
                                let target = root.path.clone();
                                let is_active = current_root == root.path;
                                let ro_tag = (!root.writable).then_some("RO");
                                view! {
                                    <button
                                        type="button"
                                        class="folder-picker__root-btn"
                                        class:is-active=is_active
                                        on:click=move |_| open_root(wid, Some(target.clone()), true)
                                    >
                                        <span>{root.label}</span>
                                        {ro_tag.map(|tag| view! { <span class="folder-picker__ro-pill">{tag}</span> })}
                                    </button>
                                }
                            }).collect_view()}
                        </div>
                    })}

                    <div class="folder-picker__toolbar">
                        <nav class="folder-picker__breadcrumbs" aria-label="Fil d'Ariane">
                            {if crumbs.is_empty() {
                                view! { <span class="text-dim text-xs">"Aucun dossier chargé"</span> }.into_any()
                            } else {
                                crumbs.into_iter().enumerate().map(|(idx, (label, crumb_path))| {
                                    let is_last = crumb_path == current_root;
                                    view! {
                                        {(idx > 0).then(|| view! { <span class="folder-picker__crumb-sep" aria-hidden="true">"/"</span> })}
                                        <button
                                            type="button"
                                            class="folder-picker__crumb"
                                            class:is-current=is_last
                                            on:click=move |_| open_root(wid, Some(crumb_path.clone()), true)
                                        >
                                            {label}
                                        </button>
                                    }
                                }).collect_view().into_any()
                            }}
                        </nav>
                        <div class="folder-picker__toolbar-actions">
                            {(!current_root.is_empty() && current_selected != current_root).then(|| {
                                let choose_path = current_root.clone();
                                view! {
                                    <button
                                        type="button"
                                        class="btn btn-secondary btn-sm"
                                        on:click=move |_| on_select.run((choose_path.clone(), current_listing_writable))
                                    >
                                        "Choisir ce dossier"
                                    </button>
                                }
                            })}
                            <button
                                type="button"
                                class="btn btn-secondary btn-sm"
                                title="Actualiser le dossier"
                                disabled=is_loading
                                on:click=move |_| {
                                    let target = (!current_root.is_empty()).then(|| current_root.clone());
                                    open_root(wid, target, false);
                                }
                            >
                                <Icon name=IconName::Refresh size=ACTION_ICON_SIZE />
                            </button>
                        </div>
                    </div>

                    {current_error.map(|msg| view! {
                        <div class="folder-picker__error" role="status">
                            <Icon name=IconName::Alert size=ACTION_ICON_SIZE />
                            <span class="folder-picker__error-text">{msg}</span>
                            <button
                                type="button"
                                class="btn btn-secondary btn-sm"
                                on:click=move |_| open_root(wid, None, false)
                            >
                                "Racines"
                            </button>
                        </div>
                    })}

                    <div class="folder-picker__tree" role="tree" aria-label="Dossiers et médias">
                        {if is_loading && tree_rows.is_empty() {
                            view! { <p class="folder-picker__empty">"Chargement de l'arborescence…"</p> }.into_any()
                        } else if tree_rows.is_empty() {
                            view! {
                                <p class="folder-picker__empty">
                                    "Aucun sous-dossier ni fichier média dans ce dossier."
                                </p>
                            }.into_any()
                        } else {
                            tree_rows.into_iter().map(|row| {
                                let depth_style = format!("--tree-depth: {};", row.depth);
                                match row.kind {
                                    TreeRowKind::Directory(dir) => {
                                        let dir_path = dir.path.clone();
                                        let toggle_path = dir.path.clone();
                                        let open_path = dir.path.clone();
                                        let select_path = dir.path.clone();
                                        let is_selected = current_selected == dir.path;
                                        let is_expanded = exp_set.contains(&dir.path);
                                        let is_child_loading = loading_set.contains(&dir.path);
                                        let can_expand = dir.has_subdirs || dir.media_count > 0;
                                        let writable = dir.writable;
                                        let media_badge = (dir.media_count > 0).then(|| {
                                            format!("{} média(s)", dir.media_count)
                                        });
                                        view! {
                                            <div
                                                class="folder-picker__row"
                                                class:is-selected=is_selected
                                                class:is-expanded=is_expanded
                                                style=depth_style
                                                role="treeitem"
                                                aria-expanded=is_expanded.to_string()
                                                aria-selected=is_selected.to_string()
                                            >
                                                <button
                                                    type="button"
                                                    class="folder-picker__chevron"
                                                    class:is-hidden=!can_expand
                                                    aria-label=if is_expanded { "Replier le dossier" } else { "Déplier le dossier" }
                                                    on:click=move |ev| {
                                                        ev.stop_propagation();
                                                        toggle_expand(wid, toggle_path.clone());
                                                    }
                                                >
                                                    <Icon name=IconName::ChevronRight size=CHEVRON_ICON_SIZE />
                                                </button>
                                                <button
                                                    type="button"
                                                    class="folder-picker__item-btn"
                                                    on:click=move |_| {
                                                        on_select.run((select_path.clone(), writable));
                                                        if can_expand
                                                            && !expanded
                                                                .get_untracked()
                                                                .contains(&dir_path)
                                                        {
                                                            expanded.update(|set| {
                                                                set.insert(dir_path.clone());
                                                            });
                                                            fetch_subdir(wid, dir_path.clone());
                                                        }
                                                    }
                                                    on:dblclick=move |_| open_root(wid, Some(open_path.clone()), true)
                                                >
                                                    <span class="folder-picker__icon">
                                                        <Icon name=IconName::Folder size=TREE_ICON_SIZE />
                                                    </span>
                                                    <span class="folder-picker__name">{dir.name}</span>
                                                    <span class="folder-picker__meta">
                                                        {is_child_loading.then(|| view! {
                                                            <span class="folder-picker__badge">"…"</span>
                                                        })}
                                                        {media_badge.map(|label| view! {
                                                            <span class="folder-picker__badge folder-picker__badge--media">{label}</span>
                                                        })}
                                                        <span
                                                            class="folder-picker__badge"
                                                            class:folder-picker__badge--ro=!writable
                                                        >
                                                            {access_badge_label(writable)}
                                                        </span>
                                                    </span>
                                                </button>
                                            </div>
                                        }.into_any()
                                    }
                                    TreeRowKind::MediaFile(file) => {
                                        let size = format_bytes(file.bytes);
                                        view! {
                                            <div class="folder-picker__row folder-picker__file" style=depth_style>
                                                <span class="folder-picker__chevron is-hidden"></span>
                                                <span class="folder-picker__icon folder-picker__icon--file">
                                                    <Icon name=IconName::Layers size=TREE_ICON_SIZE />
                                                </span>
                                                <span class="folder-picker__name">{file.name}</span>
                                                <span class="folder-picker__meta">
                                                    <span class="folder-picker__ext">{file.extension}</span>
                                                    <span class="folder-picker__size">{size}</span>
                                                </span>
                                            </div>
                                        }.into_any()
                                    }
                                }
                            }).collect_view().into_any()
                        }}
                    </div>
                }.into_any()
            }}
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_listing(
        worker_id: Uuid,
        path: &str,
        dirs: Vec<(&str, &str)>,
        files: Vec<(&str, &str, u64)>,
    ) -> DirectoryListing {
        DirectoryListing {
            worker_id,
            path: path.to_owned(),
            parent_path: None,
            writable: true,
            roots: Vec::new(),
            directories: dirs
                .into_iter()
                .map(|(name, sub_path)| DirectoryEntry {
                    name: name.to_owned(),
                    path: sub_path.to_owned(),
                    writable: true,
                    has_subdirs: false,
                    media_count: 0,
                })
                .collect(),
            files: files
                .into_iter()
                .map(|(name, ext, bytes)| MediaFileEntry {
                    name: name.to_owned(),
                    path: format!("{path}/{name}"),
                    bytes,
                    extension: ext.to_owned(),
                })
                .collect(),
        }
    }

    #[test]
    fn breadcrumbs_split_absolute_paths_cleanly() {
        assert_eq!(
            breadcrumbs_for_path("/mnt/medias/films"),
            vec![
                ("/".into(), "/".into()),
                ("mnt".into(), "/mnt".into()),
                ("medias".into(), "/mnt/medias".into()),
                ("films".into(), "/mnt/medias/films".into()),
            ]
        );
        assert_eq!(breadcrumbs_for_path("/"), vec![("/".into(), "/".into())]);
        assert!(breadcrumbs_for_path("relative").is_empty());
    }

    #[test]
    fn flatten_tree_interleaves_expanded_subdirectories_and_media_files() {
        let wid = Uuid::nil();
        let mut listings = BTreeMap::new();
        listings.insert(
            "/mnt".to_owned(),
            sample_listing(
                wid,
                "/mnt",
                vec![("Films", "/mnt/Films"), ("Séries", "/mnt/Séries")],
                vec![("sample.mkv", "mkv", 1024)],
            ),
        );
        listings.insert(
            "/mnt/Films".to_owned(),
            sample_listing(
                wid,
                "/mnt/Films",
                vec![("4K", "/mnt/Films/4K")],
                vec![("movie.mp4", "mp4", 2048)],
            ),
        );
        let mut expanded = BTreeSet::new();
        expanded.insert("/mnt/Films".to_owned());

        let rows = flatten_tree("/mnt", &listings, &expanded);
        assert_eq!(rows.len(), 5);
        assert_eq!(rows[0].depth, 0);
        assert!(matches!(&rows[0].kind, TreeRowKind::Directory(d) if d.name == "Films"));
        assert_eq!(rows[1].depth, 1);
        assert!(matches!(&rows[1].kind, TreeRowKind::Directory(d) if d.name == "4K"));
        assert_eq!(rows[2].depth, 1);
        assert!(matches!(&rows[2].kind, TreeRowKind::MediaFile(f) if f.name == "movie.mp4"));
        assert_eq!(rows[3].depth, 0);
        assert!(matches!(&rows[3].kind, TreeRowKind::Directory(d) if d.name == "Séries"));
        assert_eq!(rows[4].depth, 0);
        assert!(matches!(&rows[4].kind, TreeRowKind::MediaFile(f) if f.name == "sample.mkv"));
    }
}
