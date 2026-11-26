//! Champ « Types de fichiers » : pastilles retirables et suggestions d'extensions pendant la frappe.
//!
//! La valeur reste le texte du flux (`*.mkv, *.mp4`) : seul l'affichage change.

use leptos::prelude::*;

/// Séparateur des motifs dans la valeur enregistrée.
const SEPARATOR: &str = ",";
/// Valeur enregistrée quand aucun type n'est choisi : tous les fichiers.
pub const ALL_FILES: &str = "*";
const ALL_FILES_LABEL: &str = "Tous les fichiers";
const MAX_SUGGESTIONS: usize = 8;
const EXTENSION_PREFIX: &str = "*.";

/// Extensions proposées : motif, puis mots-clés qui le retrouvent (hors motif lui-même).
const SUGGESTIONS: &[(&str, &str)] = &[
    ("*.mkv", "matroska"),
    ("*.mp4", "mpeg4 h264"),
    ("*.m4v", "apple"),
    ("*.mov", "quicktime"),
    ("*.avi", ""),
    ("*.m2ts", "bluray"),
    ("*.ts", "mpegts"),
    ("*.hevc", "h265"),
    ("*.h264", "avc"),
    ("*.webm", "vp9 av1"),
    ("*.flac", "audio"),
    ("*.m4a", "audio aac"),
    ("*.mp3", "audio"),
    ("*.srt", "sous-titres"),
];

/// Motifs d'une valeur enregistrée, sans blancs ni doublons ; `*` si elle est vide.
pub fn split_file_types(value: &str) -> Vec<String> {
    let mut types: Vec<String> = Vec::new();
    for part in value.split(SEPARATOR) {
        let part = part.trim();
        if !part.is_empty() && !types.iter().any(|known| known == part) {
            types.push(part.to_owned());
        }
    }
    if types.is_empty() {
        types.push(ALL_FILES.to_owned());
    }
    types
}

/// Valeur enregistrée pour ces motifs ; `*` s'il n'y en a aucun.
pub fn join_file_types(types: &[String]) -> String {
    if types.is_empty() {
        return ALL_FILES.to_owned();
    }
    types.join(&format!("{SEPARATOR} "))
}

/// Motif saisi à la main : `mp4` et `.mp4` deviennent `*.mp4` ; un motif avec joker reste tel quel.
pub fn normalize_file_type(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    if trimmed.contains('*') || trimmed.contains('?') {
        return Some(trimmed.to_owned());
    }
    Some(format!(
        "{EXTENSION_PREFIX}{}",
        trimmed.trim_start_matches('.')
    ))
}

/// Ajoute le motif absent, retire le motif présent ; `*` laisse la place au premier type choisi.
pub fn toggle_file_type(types: &[String], item: &str) -> Vec<String> {
    if types.iter().any(|known| known == item) {
        let remaining: Vec<String> = types.iter().filter(|k| *k != item).cloned().collect();
        return if remaining.is_empty() {
            vec![ALL_FILES.to_owned()]
        } else {
            remaining
        };
    }
    let mut next: Vec<String> = types
        .iter()
        .filter(|known| item == ALL_FILES || *known != ALL_FILES)
        .cloned()
        .collect();
    if item == ALL_FILES {
        next.clear();
    }
    next.push(item.to_owned());
    next
}

/// Suggestions pour la saisie `query` : motif ou mot-clé contenant le texte, sans casse.
pub fn suggest_file_types(query: &str) -> Vec<&'static str> {
    let needle = query.trim().trim_start_matches(['*', '.']).to_lowercase();
    SUGGESTIONS
        .iter()
        .filter(|(pattern, keywords)| {
            needle.is_empty()
                || pattern
                    .trim_start_matches(EXTENSION_PREFIX)
                    .contains(&needle)
                || keywords.contains(&needle)
        })
        .map(|(pattern, _)| *pattern)
        .take(MAX_SUGGESTIONS)
        .collect()
}

/// Texte d'une pastille.
pub fn chip_label(item: &str) -> &str {
    if item == ALL_FILES {
        ALL_FILES_LABEL
    } else {
        item
    }
}

/// Champ à pastilles ; `on_change` reçoit la valeur à enregistrer (`*.mkv, *.mp4`).
#[component]
pub fn FileTypesInput(
    input_id: &'static str,
    #[prop(into)] value: Signal<String>,
    on_change: Callback<String>,
    #[prop(optional)] disabled: bool,
) -> impl IntoView {
    let query = RwSignal::new(String::new());
    let focused = RwSignal::new(false);
    let selected = Memo::new(move |_| split_file_types(&value.get()));
    let apply = Callback::new(move |next: Vec<String>| on_change.run(join_file_types(&next)));
    let toggle = Callback::new(move |item: String| {
        apply.run(toggle_file_type(&selected.get_untracked(), &item));
    });
    let commit_query = move || {
        if let Some(item) = normalize_file_type(&query.get_untracked()) {
            if !selected.get_untracked().contains(&item) {
                toggle.run(item);
            }
            query.set(String::new());
        }
    };

    view! {
        <div class="file-types" class:is-disabled=disabled>
            <div class="file-types__field">
                {move || selected.get().into_iter().map(|item| {
                    let label = chip_label(&item).to_owned();
                    let removed = item.clone();
                    view! {
                        <button type="button" class="file-types__chip" disabled=disabled
                            title="Retirer ce type"
                            aria-label=format!("Retirer {label}")
                            on:click=move |_| toggle.run(removed.clone())>
                            {label.clone()}
                        </button>
                    }
                }).collect_view()}
                <input id=input_id type="text" class="file-types__input" autocomplete="off"
                    placeholder="Ajouter un type…" disabled=disabled
                    prop:value=move || query.get()
                    on:input=move |event| query.set(event_target_value(&event))
                    on:focus=move |_| focused.set(true)
                    on:blur=move |_| focused.set(false)
                    on:keydown=move |event| match event.key().as_str() {
                        "Enter" | "," => {
                            event.prevent_default();
                            commit_query();
                        }
                        "Backspace" if query.get_untracked().is_empty() => {
                            let mut types = selected.get_untracked();
                            if types.pop().is_some() {
                                apply.run(types);
                            }
                        }
                        _ => {}
                    } />
            </div>
            {move || (focused.get() && !disabled).then(|| {
                let current = selected.get();
                view! {
                    <div class="file-types__options" role="listbox">
                        {suggest_file_types(&query.get()).into_iter().map(|pattern| {
                            let picked = current.iter().any(|known| known == pattern);
                            view! {
                                <button type="button" class="file-types__option"
                                    class:is-selected=picked
                                    aria-selected=picked.to_string()
                                    on:mousedown=|event| event.prevent_default()
                                    on:click=move |_| {
                                        toggle.run(pattern.to_owned());
                                        query.set(String::new());
                                    }>
                                    {pattern}
                                </button>
                            }
                        }).collect_view()}
                    </div>
                }
            })}
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn owned(items: &[&str]) -> Vec<String> {
        items.iter().map(|item| (*item).to_owned()).collect()
    }

    #[test]
    fn split_trims_dedupes_and_defaults_to_all_files() {
        assert_eq!(
            split_file_types("*.mkv, *.mp4 ,*.mkv"),
            owned(&["*.mkv", "*.mp4"])
        );
        assert_eq!(split_file_types(""), owned(&[ALL_FILES]));
        assert_eq!(split_file_types(" , "), owned(&[ALL_FILES]));
    }

    #[test]
    fn join_uses_the_stored_separator_and_defaults_to_all_files() {
        assert_eq!(join_file_types(&owned(&["*.mkv", "*.mp4"])), "*.mkv, *.mp4");
        assert_eq!(join_file_types(&[]), ALL_FILES);
    }

    #[test]
    fn normalize_completes_bare_extensions_and_keeps_wildcards() {
        assert_eq!(normalize_file_type("mp4").as_deref(), Some("*.mp4"));
        assert_eq!(normalize_file_type(" .MKV ").as_deref(), Some("*.MKV"));
        assert_eq!(
            normalize_file_type("film*.mkv").as_deref(),
            Some("film*.mkv")
        );
        assert_eq!(normalize_file_type("  "), None);
    }

    #[test]
    fn toggle_adds_then_removes_a_type() {
        let added = toggle_file_type(&owned(&["*.mkv"]), "*.mp4");
        assert_eq!(added, owned(&["*.mkv", "*.mp4"]));
        assert_eq!(toggle_file_type(&added, "*.mp4"), owned(&["*.mkv"]));
    }

    #[test]
    fn toggle_all_files_replaces_and_is_replaced() {
        assert_eq!(
            toggle_file_type(&owned(&[ALL_FILES]), "*.mkv"),
            owned(&["*.mkv"])
        );
        assert_eq!(
            toggle_file_type(&owned(&["*.mkv", "*.mp4"]), ALL_FILES),
            owned(&[ALL_FILES])
        );
    }

    #[test]
    fn removing_the_last_type_falls_back_to_all_files() {
        assert_eq!(
            toggle_file_type(&owned(&["*.mkv"]), "*.mkv"),
            owned(&[ALL_FILES])
        );
    }

    #[test]
    fn suggestions_match_patterns_and_keywords_without_case() {
        assert_eq!(suggest_file_types("he"), vec!["*.hevc"]);
        assert_eq!(suggest_file_types("MATROSKA"), vec!["*.mkv"]);
        assert_eq!(suggest_file_types("*.m4"), vec!["*.m4v", "*.m4a"]);
        assert!(suggest_file_types("zzz").is_empty());
    }

    #[test]
    fn empty_query_lists_the_first_suggestions() {
        let all = suggest_file_types("");
        assert_eq!(all.len(), MAX_SUGGESTIONS);
        assert_eq!(all.first(), Some(&"*.mkv"));
    }

    #[test]
    fn all_files_chip_has_a_readable_label() {
        assert_eq!(chip_label(ALL_FILES), "Tous les fichiers");
        assert_eq!(chip_label("*.mkv"), "*.mkv");
    }
}
