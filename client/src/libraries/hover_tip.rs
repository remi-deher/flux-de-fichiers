//! Info-bulles du serveur de langage dans l'éditeur (ADR 0032, lot c2) : au survol après une pause, ou au
//! clavier (Ctrl+K puis I) à la position du curseur. Le serveur répond en Markdown, rendu en texte sûr.

use super::hover_text::{HoverBlock, blocks};
use super::language_channel::LanguageChannel;
use super::language_logic::{Cell, CellMetrics, cell_at, language_position, parse_pixels};
use super::suggestions::caret_position;
use super::typing_pause::after_pause;
use forgearr_shared::language::Position;
use leptos::prelude::*;
use std::time::Duration;
use web_sys::{HtmlTextAreaElement, KeyboardEvent, MouseEvent};

/// Pause du pointeur sur un caractère avant d'en demander l'info-bulle.
const HOVER_DELAY: Duration = Duration::from_millis(500);
/// Nombre de caractères de la règle qui mesure la police fixe : plus il est grand, plus la mesure est fine.
const RULER_CHARS: usize = 100;
const CHORD_START_KEY: &str = "k";
const CHORD_HOVER_KEY: &str = "i";
/// Touches de modification : les presser ne rompt pas l'enchaînement Ctrl+K puis I.
const MODIFIER_KEYS: [&str; 4] = ["Control", "Shift", "Alt", "Meta"];

/// Info-bulle affichée, avec la ligne et la colonne (en caractères) sous lesquelles elle se place.
#[derive(Clone, Debug, PartialEq)]
struct Tip {
    blocks: Vec<HoverBlock>,
    line: usize,
    column: usize,
}

/// État des info-bulles d'un éditeur ; les copier donne un autre accès au même état.
#[derive(Clone, Copy)]
pub struct HoverTip {
    code: RwSignal<String>,
    channel: StoredValue<LanguageChannel, LocalStorage>,
    area: NodeRef<leptos::html::Textarea>,
    ruler: NodeRef<leptos::html::Span>,
    /// Caractère sous le pointeur ; la demande part quand il ne bouge plus.
    pointer: RwSignal<Option<Cell>>,
    tip: RwSignal<Option<Tip>>,
    /// Ctrl+K a été pressé : la touche suivante peut demander l'info-bulle.
    chord: StoredValue<bool>,
    /// Numéro de la dernière demande ; une réponse d'une demande plus ancienne est ignorée.
    asked: StoredValue<u64>,
}

impl HoverTip {
    pub fn new(
        code: RwSignal<String>,
        channel: StoredValue<LanguageChannel, LocalStorage>,
        area: NodeRef<leptos::html::Textarea>,
    ) -> Self {
        let this = Self {
            code,
            channel,
            area,
            ruler: NodeRef::new(),
            pointer: RwSignal::new(None),
            tip: RwSignal::new(None),
            chord: StoredValue::new(false),
            asked: StoredValue::new(0),
        };
        after_pause(
            move || this.pointer.get(),
            HOVER_DELAY,
            move |cell, _current| {
                if let Some(cell) = cell {
                    this.ask(cell.position, cell.line, cell.column);
                }
            },
        );
        this
    }

    /// Règle invisible dont la largeur et la hauteur donnent celles d'un caractère de la police fixe.
    pub fn ruler(&self) -> impl IntoView + use<> {
        view! {
            <span class="code-editor__ruler" aria-hidden="true" node_ref=self.ruler>
                {"0".repeat(RULER_CHARS)}
            </span>
        }
    }

    /// Cache l'info-bulle et rend caduque la demande en cours.
    pub fn hide(&self) {
        self.asked.update_value(|count| *count += 1);
        self.tip.set(None);
    }

    pub fn leave(&self) {
        self.pointer.set(None);
        self.hide();
    }

    fn ask(&self, position: Position, line: usize, column: usize) {
        self.asked.update_value(|count| *count += 1);
        let ticket = self.asked.get_value();
        let (asked, tip) = (self.asked, self.tip);
        let text = self.code.get_untracked();
        self.channel.with_value(|channel| {
            channel.hover(&text, position, move |reply| {
                if asked.try_get_value() != Some(ticket) {
                    return;
                }
                let shown = reply
                    .map(|text| blocks(&text))
                    .filter(|found| !found.is_empty());
                tip.set(shown.map(|blocks| Tip {
                    blocks,
                    line,
                    column,
                }));
            })
        });
    }

    /// Dimensions d'une cellule : la règle donne le caractère, le style calculé les marges de la saisie.
    fn metrics(&self, area: &HtmlTextAreaElement) -> Option<CellMetrics> {
        let ruler = self.ruler.get_untracked()?;
        let style = web_sys::window()?.get_computed_style(area).ok()??;
        let pixels = |property: &str| parse_pixels(&style.get_property_value(property).ok()?);
        let chars = f64::from(u32::try_from(RULER_CHARS).ok()?);
        Some(CellMetrics {
            char_width: f64::from(ruler.offset_width()) / chars,
            line_height: f64::from(ruler.offset_height()),
            pad_left: pixels("padding-left")?,
            pad_top: pixels("padding-top")?,
        })
    }

    /// Le pointeur bouge sur la saisie : l'info-bulle d'un autre caractère n'a plus lieu d'être.
    pub fn moved(&self, event: &MouseEvent) {
        let Some(area) = self.area.get_untracked() else {
            return;
        };
        let cell = self.metrics(&area).and_then(|metrics| {
            let (x, y) = (f64::from(event.offset_x()), f64::from(event.offset_y()));
            cell_at(&self.code.get_untracked(), x, y, metrics)
        });
        if self.pointer.get_untracked() != cell {
            self.hide();
            self.pointer.set(cell);
        }
    }

    /// Réagit à une touche ; rend `true` si elle est consommée (Ctrl+K, puis I).
    pub fn key(&self, event: &KeyboardEvent) -> bool {
        let key = event.key();
        if MODIFIER_KEYS.contains(&key.as_str()) {
            return false;
        }
        let chained = self.chord.get_value();
        self.chord.set_value(false);
        if chained && key.eq_ignore_ascii_case(CHORD_HOVER_KEY) {
            event.prevent_default();
            self.ask_at_caret();
            return true;
        }
        self.hide();
        if event.ctrl_key() && key.eq_ignore_ascii_case(CHORD_START_KEY) {
            event.prevent_default();
            self.chord.set_value(true);
            return true;
        }
        false
    }

    fn ask_at_caret(&self) {
        let Some(area) = self.area.get_untracked() else {
            return;
        };
        let text = self.code.get_untracked();
        let caret = area.selection_start().ok().flatten().unwrap_or(0) as usize;
        let (line, column) = caret_position(&text, caret);
        self.ask(language_position(&text, caret), line, column);
    }

    /// L'info-bulle, placée sous la ligne comme la liste de suggestions.
    pub fn tooltip(&self) -> impl IntoView + use<> {
        let tip = self.tip;
        move || {
            tip.get().map(|shown| {
                let style = format!("--caret-line: {}; --caret-col: {};", shown.line, shown.column);
                view! {
                    <div class="code-editor__hover" role="tooltip" style=style>
                        {shown.blocks.into_iter().map(|block| match block {
                            HoverBlock::Text(text) => view! { <p>{text}</p> }.into_any(),
                            HoverBlock::Code(text) => view! { <pre><code>{text}</code></pre> }.into_any(),
                        }).collect_view()}
                    </div>
                }
            })
        }
    }
}
