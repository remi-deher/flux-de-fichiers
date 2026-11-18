//! Canevas au clavier (WCAG 2.1.1) : déplacement du plan, zoom et déplacement des blocs
//! sélectionnés. La sélection, la liaison (sortie puis entrée) et la suppression passent déjà par
//! des boutons et par les raccourcis de l'éditeur.

use super::geometry::{GRID_STEP, Point, ZOOM_STEP, zoom_around};
use super::state::Editor;
use leptos::prelude::*;

/// Zoom par défaut, rétabli par la touche `0`.
pub const DEFAULT_ZOOM: f64 = 1.0;
/// Dimensions supposées du plan tant qu'il n'est pas monté.
const FALLBACK_BOARD_SIZE: (f64, f64) = (960.0, 640.0);
/// Pas de déplacement du plan au clavier, en pixels d'écran.
const PAN_STEP: f64 = 48.0;
/// Multiplicateur appliqué avec Maj (plan et blocs).
const FAST_FACTOR: f64 = 4.0;

/// Action du plan pour une touche pressée quand il a le focus.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BoardKey {
    Pan { dx: f64, dy: f64 },
    Zoom(f64),
    ResetZoom,
}

/// Direction d'une flèche : (x, y) unitaires.
fn arrow_direction(key: &str) -> Option<(f64, f64)> {
    match key {
        "ArrowLeft" => Some((-1.0, 0.0)),
        "ArrowRight" => Some((1.0, 0.0)),
        "ArrowUp" => Some((0.0, -1.0)),
        "ArrowDown" => Some((0.0, 1.0)),
        _ => None,
    }
}

fn speed(fast: bool) -> f64 {
    if fast { FAST_FACTOR } else { 1.0 }
}

/// Flèches : déplacer la vue (le contenu suit la flèche inverse, comme un défilement) ;
/// `+`/`=` et `-` : zoomer ; `0` : zoom à 100 %.
pub fn board_key(key: &str, fast: bool) -> Option<BoardKey> {
    if let Some((x, y)) = arrow_direction(key) {
        let step = PAN_STEP * speed(fast);
        return Some(BoardKey::Pan {
            dx: -x * step,
            dy: -y * step,
        });
    }
    match key {
        "+" | "=" => Some(BoardKey::Zoom(ZOOM_STEP)),
        "-" | "_" => Some(BoardKey::Zoom(-ZOOM_STEP)),
        "0" => Some(BoardKey::ResetZoom),
        _ => None,
    }
}

/// Décalage d'un bloc sélectionné pour une flèche : un pas de grille, quatre avec Maj.
pub fn nudge(key: &str, fast: bool) -> Option<Point> {
    let (x, y) = arrow_direction(key)?;
    let step = GRID_STEP * speed(fast);
    Some(Point {
        x: x * step,
        y: y * step,
    })
}

/// Largeur et hauteur visibles du plan.
pub fn board_dimensions(board_ref: NodeRef<leptos::html::Div>) -> (f64, f64) {
    board_ref.get().map_or(FALLBACK_BOARD_SIZE, |el| {
        (
            f64::from(el.client_width()).max(1.0),
            f64::from(el.client_height()).max(1.0),
        )
    })
}

/// Zoome autour du centre du plan : vers `target_zoom` s'il est donné, sinon de `delta`.
pub fn zoom_at_center(
    editor: Editor,
    board_ref: NodeRef<leptos::html::Div>,
    target_zoom: Option<f64>,
    delta: f64,
) {
    let (width, height) = board_dimensions(board_ref);
    let center = Point {
        x: width / 2.0,
        y: height / 2.0,
    };
    editor.viewport.update(|vp| {
        let next = target_zoom.unwrap_or(vp.zoom + delta);
        *vp = zoom_around(*vp, center, next);
    });
}

/// Applique une action clavier du plan.
pub fn apply_board_key(editor: Editor, board_ref: NodeRef<leptos::html::Div>, action: BoardKey) {
    match action {
        BoardKey::Pan { dx, dy } => editor.viewport.update(|vp| {
            vp.pan_x += dx;
            vp.pan_y += dy;
        }),
        BoardKey::Zoom(delta) => zoom_at_center(editor, board_ref, None, delta),
        BoardKey::ResetZoom => zoom_at_center(editor, board_ref, Some(DEFAULT_ZOOM), 0.0),
    }
}

/// Déplace `id` et les blocs sélectionnés avec lui de `delta`, puis enregistre la disposition.
pub fn nudge_selection(editor: Editor, id: &str, delta: Point) {
    if !editor.is_selected(id) {
        editor.select_node(Some(id.to_owned()), false);
    }
    let updates: Vec<(String, Point)> = editor
        .selected_ids
        .get_untracked()
        .into_iter()
        .map(|selected| {
            let origin = editor.node_point(&selected);
            (
                selected,
                Point {
                    x: origin.x + delta.x,
                    y: origin.y + delta.y,
                },
            )
        })
        .collect();
    editor.move_nodes_live(&updates, false);
    editor.commit_positions();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arrows_pan_the_view_and_shift_goes_faster() {
        assert_eq!(
            board_key("ArrowRight", false),
            Some(BoardKey::Pan {
                dx: -PAN_STEP,
                dy: 0.0
            })
        );
        assert_eq!(
            board_key("ArrowUp", true),
            Some(BoardKey::Pan {
                dx: 0.0,
                dy: PAN_STEP * FAST_FACTOR
            })
        );
    }

    #[test]
    fn plus_minus_and_zero_drive_the_zoom() {
        assert_eq!(board_key("+", false), Some(BoardKey::Zoom(ZOOM_STEP)));
        assert_eq!(board_key("=", false), Some(BoardKey::Zoom(ZOOM_STEP)));
        assert_eq!(board_key("-", false), Some(BoardKey::Zoom(-ZOOM_STEP)));
        assert_eq!(board_key("0", false), Some(BoardKey::ResetZoom));
        assert_eq!(board_key("a", false), None);
    }

    #[test]
    fn arrows_nudge_a_block_by_one_grid_step_or_four_with_shift() {
        assert_eq!(
            nudge("ArrowLeft", false),
            Some(Point {
                x: -GRID_STEP,
                y: 0.0
            })
        );
        assert_eq!(
            nudge("ArrowDown", true),
            Some(Point {
                x: 0.0,
                y: GRID_STEP * FAST_FACTOR
            })
        );
        assert_eq!(nudge("Enter", false), None);
    }
}
