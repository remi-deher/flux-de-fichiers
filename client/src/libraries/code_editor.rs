//! Zone de code de l'éditeur de script (ADR 0030, lot 2) : numéros de ligne, tabulation qui indente et
//! coloration. Un `<textarea>` transparent est superposé à un `<pre>` coloré ; les deux occupent la même case
//! d'une grille, que le conteneur fait défiler d'un bloc. Les jetons viennent du central.

use crate::api;
use crate::plugins::highlighted_lines;
use forgearr_shared::highlight::{CodeToken, HighlightRequest, HighlightedCode, TokenKind};
use leptos::prelude::*;
use std::time::Duration;
use wasm_bindgen::JsCast;
use web_sys::HtmlTextAreaElement;

const HIGHLIGHT_PATH: &str = "/api/v1/highlight";
/// Pause de frappe avant de demander la coloration au central.
const HIGHLIGHT_DELAY: Duration = Duration::from_millis(350);
/// Retrait inséré par la touche Tab.
const INDENT: &str = "  ";

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

/// Jetons à afficher : ceux du central s'ils décrivent exactement `code`, sinon le texte brut, pour que la
/// couche colorée ne se décale jamais du texte saisi.
fn visible_lines(code: &str, highlighted: &Highlighted) -> Vec<Vec<CodeToken>> {
    match highlighted {
        Some((computed_for, lines)) if computed_for == code => lines.clone(),
        _ => plain_lines(code),
    }
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

/// Éditeur de code : `code` est lu et écrit ; `extension` choisit la grammaire de coloration.
#[component]
pub fn CodeEditor(
    code: RwSignal<String>,
    #[prop(into)] extension: Signal<&'static str>,
    read_only: bool,
) -> impl IntoView {
    let highlighted = RwSignal::new(None::<(String, Vec<Vec<CodeToken>>)>);
    schedule_highlight(code, extension, highlighted);
    // Échap libère la touche Tab une fois, pour pouvoir quitter la zone au clavier.
    let release_tab = StoredValue::new(false);

    let on_keydown = move |ev: leptos::ev::KeyboardEvent| {
        let key = ev.key();
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
                <textarea id="script-code"
                    class="code-editor__layer code-editor__input"
                    required=true disabled=read_only spellcheck="false" wrap="off"
                    autocapitalize="off" autocomplete="off"
                    prop:value=move || code.get()
                    on:input=move |e| code.set(event_target_value(&e))
                    on:keydown=on_keydown />
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
        let stale = visible_lines("ls -l", &known);
        assert_eq!(stale, plain_lines("ls -l"));
        assert_eq!(visible_lines("ls", &None), plain_lines("ls"));
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
