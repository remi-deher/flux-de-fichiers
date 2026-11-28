//! Zone de code de l'éditeur de script (ADR 0030, lot 2) : numéros de ligne, tabulation qui indente et
//! coloration. Un `<textarea>` transparent est superposé à un `<pre>` coloré ; les deux occupent la même case
//! d'une grille, que le conteneur fait défiler d'un bloc. Les jetons viennent du central.

use super::language_channel::LanguageChannel;
use super::language_logic::{self, char_before};
use super::suggestions::{self, MIN_PREFIX_CHARS, Suggestion};
use crate::api;
use crate::plugins::highlighted_lines;
use forgearr_shared::highlight::{CodeToken, HighlightRequest, HighlightedCode, TokenKind};
use forgearr_shared::local::ScriptInterpreter;
use leptos::prelude::*;
use std::time::Duration;
use wasm_bindgen::JsCast;
use web_sys::HtmlTextAreaElement;

const HIGHLIGHT_PATH: &str = "/api/v1/highlight";
/// Pause de frappe avant de demander la coloration au central.
const HIGHLIGHT_DELAY: Duration = Duration::from_millis(350);
/// Retrait inséré par la touche Tab.
const INDENT: &str = "  ";

/// Liste de suggestions ouverte sous le curseur.
#[derive(Clone, Debug)]
struct SuggestionPopup {
    items: Vec<Suggestion>,
    selected: usize,
    /// Début du mot tapé et position du curseur (unités UTF-16) : la plage remplacée à l'acceptation.
    start: usize,
    caret: usize,
    line: usize,
    column: usize,
}

/// Code et jetons calculés pour ce code précis.
type Highlighted = Option<(String, Vec<Vec<CodeToken>>)>;

/// Nombre de lignes affichées pour `code` : au moins une, une de plus après chaque saut de ligne.
fn line_count(code: &str) -> usize {
    code.matches('\n').count() + 1
}

/// Le code sans coloration : une ligne de jetons neutres par ligne de texte.
fn plain_lines(code: &str) -> Vec<Vec<CodeToken>> {
    code.lines()
        .map(|line| {
            vec![CodeToken {
                text: line.to_owned(),
                kind: TokenKind::Plain,
            }]
        })
        .collect()
}

/// Jetons à afficher : ceux du central s'ils décrivent exactement `code` ; sinon les lignes inchangées depuis
/// le dernier calcul gardent leur couleur (début et fin du texte) et seules les lignes touchées passent en
/// texte brut. La couche colorée garde ainsi le même nombre de lignes que le texte saisi et ne se décale jamais.
fn visible_lines(code: &str, highlighted: &Highlighted) -> Vec<Vec<CodeToken>> {
    let Some((computed_for, tokens)) = highlighted else {
        return plain_lines(code);
    };
    if computed_for == code {
        return tokens.clone();
    }
    let old_lines: Vec<&str> = computed_for.lines().collect();
    let new_lines: Vec<&str> = code.lines().collect();
    if old_lines.len() != tokens.len() {
        return plain_lines(code);
    }
    let head = old_lines
        .iter()
        .zip(&new_lines)
        .take_while(|(old, new)| old == new)
        .count();
    let tail = old_lines[head..]
        .iter()
        .rev()
        .zip(new_lines[head..].iter().rev())
        .take_while(|(old, new)| old == new)
        .count();
    let mut lines = plain_lines(code);
    lines[..head].clone_from_slice(&tokens[..head]);
    let (old_end, new_end) = (old_lines.len() - tail, new_lines.len() - tail);
    lines[new_end..].clone_from_slice(&tokens[old_end..]);
    lines
}

/// Remplace la sélection `[start, end)` (en unités UTF-16, comme le navigateur) par un retrait ; renvoie le
/// nouveau texte et la position du curseur après le retrait.
fn insert_indent(code: &str, start: usize, end: usize) -> (String, usize) {
    let units: Vec<u16> = code.encode_utf16().collect();
    let (start, end) = (start.min(units.len()), end.min(units.len()));
    let (start, end) = (start.min(end), start.max(end));
    let indent: Vec<u16> = INDENT.encode_utf16().collect();
    let caret = start + indent.len();
    let mut next = units[..start].to_vec();
    next.extend_from_slice(&indent);
    next.extend_from_slice(&units[end..]);
    (String::from_utf16_lossy(&next), caret)
}

/// Demande la coloration après une pause de frappe ; une frappe plus récente annule l'appel en attente.
fn schedule_highlight(
    code: RwSignal<String>,
    extension: Signal<&'static str>,
    highlighted: RwSignal<Highlighted>,
) {
    let generation = StoredValue::new(0_u64);
    Effect::new(move |_| {
        let text = code.get();
        let extension = extension.get();
        generation.update_value(|count| *count += 1);
        let ticket = generation.get_value();
        set_timeout(
            move || {
                if generation.try_get_value() != Some(ticket) {
                    return;
                }
                leptos::task::spawn_local(async move {
                    let request = HighlightRequest {
                        code: text.clone(),
                        extension: extension.to_owned(),
                    };
                    let reply =
                        api::post::<_, HighlightedCode>(HIGHLIGHT_PATH, Some(&request)).await;
                    if let Ok(done) = reply
                        && generation.try_get_value() == Some(ticket)
                    {
                        let _ = highlighted.try_set(Some((text, done.lines)));
                    }
                });
            },
            HIGHLIGHT_DELAY,
        );
    });
}

/// Éditeur de code : `code` est lu et écrit ; `extension` choisit la grammaire de coloration et `interpreter`
/// le serveur de langage qui complète la liste locale.
#[component]
pub fn CodeEditor(
    code: RwSignal<String>,
    #[prop(into)] extension: Signal<&'static str>,
    #[prop(into)] interpreter: Signal<ScriptInterpreter>,
    /// Noms des variables déclarées du script, proposés dans les suggestions.
    #[prop(into)]
    declared: Signal<Vec<String>>,
    read_only: bool,
) -> impl IntoView {
    let highlighted = RwSignal::new(None::<(String, Vec<Vec<CodeToken>>)>);
    schedule_highlight(code, extension, highlighted);
    // Échap libère la touche Tab une fois, pour pouvoir quitter la zone au clavier.
    let release_tab = StoredValue::new(false);
    let area_ref = NodeRef::<leptos::html::Textarea>::new();
    let popup = RwSignal::new(None::<SuggestionPopup>);
    // Le serveur de langage complète la liste locale ; sans lui, la liste locale répond seule.
    let channel = StoredValue::new_local(LanguageChannel::new(
        interpreter.get_untracked(),
        &code.get_untracked(),
    ));
    Effect::new(move |_| {
        let interpreter = interpreter.get();
        if !read_only {
            channel.with_value(|channel| channel.set_interpreter(interpreter));
        }
    });
    on_cleanup(move || {
        channel.try_with_value(LanguageChannel::close);
    });
    // Une réponse du serveur n'est prise en compte que si aucune frappe plus récente ne l'a rendue caduque.
    let request = StoredValue::new(0_u64);

    // Recalcule la liste d'après le mot qui précède le curseur.
    let refresh = move |area: &HtmlTextAreaElement| {
        let text = area.value();
        let caret = area.selection_start().ok().flatten().unwrap_or(0) as usize;
        let (start, prefix) = suggestions::word_before(&text, caret);
        let local = suggestions::suggest(
            &prefix,
            &text,
            extension.get_untracked(),
            &declared.get_untracked(),
        );
        let (line, column) = suggestions::caret_position(&text, caret);
        let show = move |items: Vec<Suggestion>| {
            if items.is_empty() {
                popup.set(None);
                return;
            }
            popup.set(Some(SuggestionPopup {
                items,
                selected: 0,
                start,
                caret,
                line,
                column,
            }));
        };
        request.update_value(|count| *count += 1);
        let ticket = request.get_value();
        let triggered = char_before(&text, caret)
            .is_some_and(|typed| language_logic::is_trigger(extension.get_untracked(), typed));
        let asks_server = !read_only && (triggered || prefix.chars().count() >= MIN_PREFIX_CHARS);
        if asks_server {
            let fallback = local.clone();
            let answer = move |items: Vec<forgearr_shared::language::Suggestion>| {
                if request.try_get_value() != Some(ticket) {
                    return;
                }
                let server = language_logic::from_server(&items, &prefix);
                show(language_logic::merge(server, fallback));
            };
            channel.with_value(|channel| channel.complete(&text, caret, answer));
        }
        show(local);
    };

    // Remplace le mot tapé par la suggestion `index`.
    let accept = move |index: usize| {
        let Some(open) = popup.get_untracked() else {
            return;
        };
        let (Some(item), Some(area)) = (open.items.get(index), area_ref.get_untracked()) else {
            return;
        };
        let (next, caret) = suggestions::replace_range(
            &code.get_untracked(),
            open.start,
            open.caret,
            item.inserted(),
        );
        area.set_value(&next);
        let caret = u32::try_from(caret).unwrap_or(u32::MAX);
        let _ = area.set_selection_range(caret, caret);
        code.set(next);
        popup.set(None);
    };

    let on_keydown = move |ev: leptos::ev::KeyboardEvent| {
        let key = ev.key();
        if let Some(open) = popup.get_untracked() {
            let last = open.items.len().saturating_sub(1);
            match key.as_str() {
                "ArrowDown" | "ArrowUp" => {
                    ev.prevent_default();
                    let selected = if key == "ArrowDown" {
                        (open.selected + 1).min(last)
                    } else {
                        open.selected.saturating_sub(1)
                    };
                    popup.set(Some(SuggestionPopup { selected, ..open }));
                    return;
                }
                "Enter" | "Tab" => {
                    ev.prevent_default();
                    accept(open.selected);
                    return;
                }
                "Escape" => {
                    popup.set(None);
                    return;
                }
                "ArrowLeft" | "ArrowRight" | "Home" | "End" => popup.set(None),
                _ => {}
            }
        }
        if key == "Escape" {
            release_tab.set_value(true);
            return;
        }
        let plain_tab = key == "Tab" && !ev.shift_key() && !ev.ctrl_key() && !ev.alt_key();
        if !plain_tab || read_only {
            return;
        }
        if release_tab.get_value() {
            release_tab.set_value(false);
            return;
        }
        ev.prevent_default();
        let Some(area) = ev
            .target()
            .and_then(|target| target.dyn_into::<HtmlTextAreaElement>().ok())
        else {
            return;
        };
        let start = area.selection_start().ok().flatten().unwrap_or(0) as usize;
        let end = area.selection_end().ok().flatten().unwrap_or(0) as usize;
        let (next, caret) = insert_indent(&code.get_untracked(), start, end);
        area.set_value(&next);
        let caret = u32::try_from(caret).unwrap_or(u32::MAX);
        let _ = area.set_selection_range(caret, caret);
        code.set(next);
    };

    view! {
        <div class="code-editor">
            <ol class="code-editor__gutter" aria-hidden="true">
                {move || (1..=line_count(&code.get()))
                    .map(|number| view! { <li>{number}</li> })
                    .collect_view()}
            </ol>
            <div class="code-editor__stack">
                <pre class="code-editor__layer code-editor__highlight" aria-hidden="true">
                    {move || {
                        let current = code.get();
                        let lines = visible_lines(&current, &highlighted.get());
                        highlighted_lines(lines)
                    }}
                    "\n"
                </pre>
                <textarea id="script-code" node_ref=area_ref
                    class="code-editor__layer code-editor__input"
                    required=true disabled=read_only spellcheck="false" wrap="off"
                    autocapitalize="off" autocomplete="off"
                    prop:value=move || code.get()
                    on:input=move |e| {
                        code.set(event_target_value(&e));
                        if let Some(area) = area_ref.get_untracked() {
                            refresh(&area);
                        }
                    }
                    on:blur=move |_| popup.set(None)
                    on:click=move |_| popup.set(None)
                    on:keydown=on_keydown />
                {move || popup.get().map(|open| {
                    let selected = open.selected;
                    view! {
                        <ul class="code-editor__suggestions" role="listbox"
                            style=format!("--caret-line: {}; --caret-col: {};", open.line, open.column)>
                            {open.items.into_iter().enumerate().map(|(index, item)| view! {
                                <li class="code-editor__suggestion" role="option"
                                    class:is-selected=index == selected
                                    aria-selected=(index == selected).to_string()
                                    on:mousedown=move |ev| {
                                        ev.prevent_default();
                                        accept(index);
                                    }>
                                    <span>{item.label}</span>
                                    <span class="code-editor__suggestion-kind">{item.kind.label()}</span>
                                </li>
                            }).collect_view()}
                        </ul>
                    }
                })}
            </div>
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn there_is_one_more_line_than_line_breaks() {
        assert_eq!(line_count(""), 1);
        assert_eq!(line_count("a"), 1);
        assert_eq!(line_count("a\nb\n"), 3);
    }

    #[test]
    fn plain_lines_split_the_code_without_line_breaks() {
        let lines = plain_lines("ls\n\necho");
        let texts: Vec<&str> = lines
            .iter()
            .map(|line| line.first().map_or("", |token| token.text.as_str()))
            .collect();
        assert_eq!(texts, ["ls", "", "echo"]);
    }

    #[test]
    fn tokens_are_shown_only_for_the_code_they_were_computed_for() {
        let tokens = vec![vec![CodeToken {
            text: "ls".to_owned(),
            kind: TokenKind::Keyword,
        }]];
        let known = Some(("ls".to_owned(), tokens.clone()));
        assert_eq!(visible_lines("ls", &known), tokens);
        assert_eq!(visible_lines("ls", &None), plain_lines("ls"));
    }

    fn colored(text: &str) -> Vec<CodeToken> {
        vec![CodeToken {
            text: text.to_owned(),
            kind: TokenKind::Keyword,
        }]
    }

    #[test]
    fn untouched_lines_keep_their_color_while_the_edited_one_turns_plain() {
        let known = Some((
            "a\nb\nc".to_owned(),
            vec![colored("a"), colored("b"), colored("c")],
        ));
        let lines = visible_lines("a\nbx\nc", &known);
        assert_eq!(
            lines,
            [colored("a"), plain_lines("bx").remove(0), colored("c")]
        );
    }

    #[test]
    fn inserted_and_removed_lines_keep_the_line_count_of_the_text() {
        let known = Some(("a\nc".to_owned(), vec![colored("a"), colored("c")]));
        let inserted = visible_lines("a\nb\nc", &known);
        assert_eq!(inserted.len(), 3);
        assert_eq!(inserted[0], colored("a"));
        assert_eq!(inserted[2], colored("c"));
        let removed = visible_lines("c", &known);
        assert_eq!(removed, [colored("c")]);
    }

    #[test]
    fn tokens_that_do_not_match_their_text_fall_back_to_plain() {
        let known = Some(("a\nb".to_owned(), vec![colored("a")]));
        assert_eq!(visible_lines("a\nbx", &known), plain_lines("a\nbx"));
    }

    #[test]
    fn tab_inserts_two_spaces_at_the_caret_or_over_the_selection() {
        assert_eq!(insert_indent("ab", 1, 1), ("a  b".to_owned(), 3));
        assert_eq!(insert_indent("abcd", 1, 3), ("a  d".to_owned(), 3));
        assert_eq!(insert_indent("ab", 2, 0), ("  ".to_owned(), 2));
        assert_eq!(insert_indent("ab", 9, 9), ("ab  ".to_owned(), 4));
    }

    #[test]
    fn the_caret_counts_utf16_units_like_the_browser() {
        // « é » = 1 unité ; « 😀 » = 2 unités.
        assert_eq!(insert_indent("é😀x", 3, 3), ("é😀  x".to_owned(), 5));
    }
}
