//! Géométrie du graphe affiché : positions des cartes, ancrages des liaisons, tracés SVG.

use flux_shared::flows::GraphPosition;

pub const CARD_WIDTH: f64 = 220.0;
pub const CARD_HEIGHT: f64 = 76.0;
const GAP_X: f64 = 40.0;
const GAP_Y: f64 = 64.0;
const PADDING: f64 = 24.0;
/// Courbure verticale d'une liaison.
const CURVE: f64 = 28.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

/// Coin haut-gauche d'une carte.
pub fn card_origin(position: GraphPosition) -> Point {
    Point {
        x: PADDING + position.slot as f64 * (CARD_WIDTH + GAP_X),
        y: PADDING + position.layer as f64 * (CARD_HEIGHT + GAP_Y),
    }
}

/// Entrée d'une carte : milieu du bord haut.
pub fn input_anchor(position: GraphPosition) -> Point {
    let origin = card_origin(position);
    Point {
        x: origin.x + CARD_WIDTH / 2.0,
        y: origin.y,
    }
}

/// Sortie `index` parmi `count` : répartie sur le bord bas.
pub fn output_anchor(position: GraphPosition, index: usize, count: usize) -> Point {
    let origin = card_origin(position);
    let share = (index + 1) as f64 / (count.max(1) + 1) as f64;
    Point {
        x: origin.x + CARD_WIDTH * share,
        y: origin.y + CARD_HEIGHT,
    }
}

/// Courbe de Bézier verticale entre une sortie et une entrée.
pub fn link_path(from: Point, to: Point) -> String {
    format!(
        "M {} {} C {} {}, {} {}, {} {}",
        from.x,
        from.y,
        from.x,
        from.y + CURVE,
        to.x,
        to.y - CURVE,
        to.x,
        to.y
    )
}

/// Taille du plan qui contient toutes les cartes.
pub fn board_size<'a>(positions: impl Iterator<Item = &'a GraphPosition>) -> Point {
    let (slots, layers) = positions.fold((1, 1), |(slots, layers), position| {
        (slots.max(position.slot + 1), layers.max(position.layer + 1))
    });
    Point {
        x: 2.0 * PADDING + slots as f64 * CARD_WIDTH + (slots - 1) as f64 * GAP_X,
        y: 2.0 * PADDING + layers as f64 * CARD_HEIGHT + (layers - 1) as f64 * GAP_Y,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ORIGIN: GraphPosition = GraphPosition { layer: 0, slot: 0 };

    #[test]
    fn cards_are_laid_out_by_slot_and_layer() {
        assert_eq!(
            card_origin(ORIGIN),
            Point {
                x: PADDING,
                y: PADDING
            }
        );
        let below = card_origin(GraphPosition { layer: 1, slot: 1 });
        assert_eq!(below.x, PADDING + CARD_WIDTH + GAP_X);
        assert_eq!(below.y, PADDING + CARD_HEIGHT + GAP_Y);
    }

    #[test]
    fn outputs_share_the_bottom_edge_and_the_input_is_centered() {
        assert_eq!(input_anchor(ORIGIN).x, PADDING + CARD_WIDTH / 2.0);
        let first = output_anchor(ORIGIN, 0, 2);
        let second = output_anchor(ORIGIN, 1, 2);
        assert!(first.x < second.x);
        assert_eq!(first.y, PADDING + CARD_HEIGHT);
        assert_eq!(output_anchor(ORIGIN, 0, 1).x, input_anchor(ORIGIN).x);
    }

    #[test]
    fn the_board_contains_every_card() {
        let positions = [ORIGIN, GraphPosition { layer: 2, slot: 1 }];
        let size = board_size(positions.iter());
        let last = card_origin(positions[1]);
        assert_eq!(size.x, last.x + CARD_WIDTH + PADDING);
        assert_eq!(size.y, last.y + CARD_HEIGHT + PADDING);
        assert_eq!(
            board_size([].iter()),
            Point {
                x: 2.0 * PADDING + CARD_WIDTH,
                y: 2.0 * PADDING + CARD_HEIGHT
            }
        );
        assert!(
            link_path(Point { x: 0.0, y: 0.0 }, Point { x: 1.0, y: 2.0 }).starts_with("M 0 0 C")
        );
    }
}
