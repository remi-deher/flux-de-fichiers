//! Menu contextuel Quick-Add sur le canevas : suggestions intelligentes, recherche et clavier.

use super::geometry::Point;
use super::presentation::NodeDomain;
use super::state::Editor;
use crate::ui::search::matches;
use crate::ui::{Icon, IconName, Toasts};
use flux_shared::flows::{OwnedBlockSpec, find_owned};
use leptos::prelude::*;

const POPOVER_WIDTH: f64 = 280.0;
const POPOVER_HEIGHT: f64 = 340.0;
const POPOVER_MARGIN: f64 = 12.0;
const ITEM_ICON_SIZE: u32 = 14;
const SEARCH_ICON_SIZE: u32 = 14;

/// Résultats partitionnés du menu Quick-Add.
#[derive(Clone, Debug, PartialEq)]
pub struct QuickAddResults {
    pub suggested: Vec<OwnedBlockSpec>,
    pub others: Vec<OwnedBlockSpec>,
}

impl QuickAddResults {
    pub fn ordered(&self) -> Vec<OwnedBlockSpec> {
        let mut all = Vec::with_capacity(self.suggested.len() + self.others.len());
        all.extend(self.suggested.iter().cloned());
        all.extend(self.others.iter().cloned());
        all
    }
}

/// Filtre et classe le catalogue pour Quick-Add (suggestions en tête si requête vide).
pub fn quick_add_candidates(
    catalog: &[OwnedBlockSpec],
    query: &str,
    requires_input: bool,
    source: Option<&OwnedBlockSpec>,
) -> QuickAddResults {
    let trimmed = query.trim();
    let eligible: Vec<OwnedBlockSpec> = catalog
        .iter()
        .filter(|spec| !requires_input || spec.has_input)
        .filter(|spec| {
            trimmed.is_empty()
                || matches(
                    &[&spec.label, &spec.category, &spec.kind, &spec.description],
                    trimmed,
                )
        })
        .cloned()
        .collect();

    if !trimmed.is_empty() {
        return QuickAddResults {
            suggested: Vec::new(),
            others: eligible,
        };
    }

    let preferred = source.map_or(&[][..], |spec| spec.suggested_next.as_slice());
    let mut suggested = Vec::new();
    for kind in preferred {
        if let Some(spec) = eligible.iter().find(|s| s.matches_kind(kind)) {
            suggested.push(spec.clone());
        }
    }
    let others = eligible
        .into_iter()
        .filter(|spec| !suggested.iter().any(|s| s.kind == spec.kind))
        .collect();
    QuickAddResults { suggested, others }
}

/// Maintient le pop-over entièrement visible à l'intérieur du canevas `(area_w, area_h)`.
pub fn clamp_popover_origin(anchor: Point, area_w: f64, area_h: f64) -> Point {
    let max_x = (area_w - POPOVER_WIDTH - POPOVER_MARGIN).max(POPOVER_MARGIN);
    let max_y = (area_h - POPOVER_HEIGHT - POPOVER_MARGIN).max(POPOVER_MARGIN);
    Point {
        x: anchor.x.clamp(POPOVER_MARGIN, max_x),
        y: anchor.y.clamp(POPOVER_MARGIN, max_y),
    }
}

fn render_quick_add_item(
    editor: Editor,
    spec: OwnedBlockSpec,
    index: usize,
    active_idx: RwSignal<usize>,
) -> impl IntoView {
    let domain = NodeDomain::from_spec(Some(&spec), &spec.category);
    let accent_style = format!("--component-accent: {}", domain.accent_var());
    let category = spec.category.clone();
    let label = spec.label.clone();
    let click_spec = spec.clone();

    view! {
        <button
            type="button"
            class="flow-quick-add__item"
            class:is-active=move || active_idx.get() == index
            style=accent_style
            on:mouseenter=move |_| active_idx.set(index)
            on:click=move |event| {
                event.stop_propagation();
                if let Err(err) = editor.complete_quick_add(&click_spec) {
                    Toasts::notify_error(err.message());
                }
            }
        >
            <span class="flow-quick-add__icon" aria-hidden="true">
                <Icon name=domain.icon() size=ITEM_ICON_SIZE />
            </span>
            <span class="flow-quick-add__text">
                <span class="flow-quick-add__label">{label}</span>
                <span class="flow-quick-add__cat">{category}</span>
            </span>
        </button>
    }
}

#[component]
pub fn QuickAddPopover(editor: Editor, board_ref: NodeRef<leptos::html::Div>) -> impl IntoView {
    let query = RwSignal::new(String::new());
    let active_idx = RwSignal::new(0_usize);
    let input_ref = NodeRef::<leptos::html::Input>::new();

    Effect::new(move |_| {
        if editor.quick_add.get().is_some() {
            query.set(String::new());
            active_idx.set(0);
            request_animation_frame(move || {
                if let Some(input) = input_ref.get() {
                    let _ = input.focus();
                }
            });
        }
    });

    move || {
        let state = editor.quick_add.get()?;
        let requires_input = state.branch.is_some() || editor.has_entry_node();
        let source_kind = state.branch.as_ref().and_then(|branch| {
            editor
                .graph
                .with(|graph| graph.node(&branch.source_id).map(|n| n.kind.clone()))
        });
        let results = Memo::new(move |_| {
            editor.catalog.with(|catalog| {
                let source_spec = source_kind
                    .as_ref()
                    .and_then(|kind| find_owned(catalog, kind));
                quick_add_candidates(catalog, &query.get(), requires_input, source_spec)
            })
        });

        let (area_w, area_h) = board_ref.get().map_or((960.0, 640.0), |el| {
            (f64::from(el.client_width()), f64::from(el.client_height()))
        });
        let origin = clamp_popover_origin(state.anchor, area_w, area_h);
        let style = format!("left:{:.0}px;top:{:.0}px;", origin.x, origin.y);

        Some(view! {
            <div
                class="flow-quick-add"
                style=style
                role="dialog"
                aria-label="Ajout rapide de composant"
                on:mousedown=move |event| event.stop_propagation()
                on:click=move |event| event.stop_propagation()
                on:wheel=move |event| event.stop_propagation()
                on:keydown=move |event| {
                    let ordered = results.with_untracked(QuickAddResults::ordered);
                    let total = ordered.len();
                    match event.key().as_str() {
                        "ArrowDown" if total > 0 => {
                            event.prevent_default();
                            active_idx.update(|idx| *idx = (*idx + 1) % total);
                        }
                        "ArrowUp" if total > 0 => {
                            event.prevent_default();
                            active_idx.update(|idx| *idx = (*idx + total - 1) % total);
                        }
                        "Enter" => {
                            if let Some(spec) = ordered.get(active_idx.get_untracked()) {
                                event.prevent_default();
                                if let Err(err) = editor.complete_quick_add(spec) {
                                    Toasts::notify_error(err.message());
                                }
                            }
                        }
                        "Escape" => {
                            event.prevent_default();
                            editor.quick_add.set(None);
                        }
                        _ => {}
                    }
                }
            >
                <div class="flow-quick-add__search">
                    <Icon name=IconName::Search size=SEARCH_ICON_SIZE class="flow-quick-add__search-icon" />
                    <input
                        node_ref=input_ref
                        type="search"
                        class="form-input flow-quick-add__input"
                        placeholder="Ajouter une étape…"
                        aria-label="Rechercher une étape à ajouter"
                        prop:value=move || query.get()
                        on:input=move |event| {
                            query.set(event_target_value(&event));
                            active_idx.set(0);
                        }
                    />
                </div>
                <div class="flow-quick-add__list">
                    {move || {
                        let res = results.get();
                        let sug_len = res.suggested.len();
                        if res.suggested.is_empty() && res.others.is_empty() {
                            let empty_msg = if editor.catalog.with(|c| c.is_empty()) {
                                "Aucun bloc disponible — synchronisez un dépôt dans Extensions."
                            } else {
                                "Aucun composant trouvé."
                            };
                            return view! {
                                <p class="flow-quick-add__empty">{empty_msg}</p>
                            }
                            .into_any();
                        }
                        view! {
                            <>
                                {(!res.suggested.is_empty()).then(|| view! {
                                    <div class="flow-quick-add__group-label">"Suggérés"</div>
                                })}
                                {res.suggested
                                    .into_iter()
                                    .enumerate()
                                    .map(|(i, spec)| render_quick_add_item(editor, spec, i, active_idx))
                                    .collect_view()}
                                {(!res.others.is_empty() && sug_len > 0).then(|| view! {
                                    <div class="flow-quick-add__group-label">"Tous les composants"</div>
                                })}
                                {res.others
                                    .into_iter()
                                    .enumerate()
                                    .map(|(i, spec)| {
                                        render_quick_add_item(editor, spec, sug_len + i, active_idx)
                                    })
                                    .collect_view()}
                            </>
                        }
                        .into_any()
                    }}
                </div>
            </div>
        })
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_support::test_catalog;
    use super::*;
    use flux_shared::flows::FlowBlockKind;

    #[test]
    fn quick_add_suggests_contextual_blocks_and_filters_inputless_on_branch() {
        let catalog = test_catalog();
        let entry = find_owned(&catalog, &FlowBlockKind::parse("block.core.file-input"));
        let res = quick_add_candidates(&catalog, "", true, entry);
        assert!(!res.suggested.is_empty());
        assert_eq!(res.suggested[0].kind, "block.core.condition");
        assert!(res.ordered().iter().all(|s| s.has_input));

        let filtered = quick_add_candidates(&catalog, "sonarr", true, None);
        assert!(filtered.suggested.is_empty());
        assert_eq!(filtered.others.len(), 2);
    }

    #[test]
    fn popover_origin_is_clamped_inside_canvas_bounds() {
        let clamped = clamp_popover_origin(Point { x: 950.0, y: 620.0 }, 1000.0, 650.0);
        assert!(clamped.x + POPOVER_WIDTH <= 1000.0 - POPOVER_MARGIN + 0.01);
        assert!(clamped.y + POPOVER_HEIGHT <= 650.0 - POPOVER_MARGIN + 0.01);

        let min_clamped = clamp_popover_origin(Point { x: -20.0, y: 0.0 }, 1000.0, 650.0);
        assert_eq!(
            min_clamped,
            Point {
                x: POPOVER_MARGIN,
                y: POPOVER_MARGIN
            }
        );
    }
}
