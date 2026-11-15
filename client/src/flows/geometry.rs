//! Géométrie du graphe affiché : placement libre 2D, centrage topologique, ports et courbes SVG.

use flux_shared::flows::{FlowGraph, layout};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

pub const CARD_WIDTH: f64 = 248.0;
pub const CARD_HEIGHT: f64 = 92.0;
pub const COL_STRIDE: f64 = 296.0;
pub const ROW_STRIDE: f64 = 148.0;
pub const ORIGIN_X: f64 = 72.0;
pub const ORIGIN_Y: f64 = 64.0;
pub const BOARD_PAD: f64 = 120.0;
pub const ZOOM_MIN: f64 = 0.25;
pub const ZOOM_MAX: f64 = 1.75;
pub const ZOOM_STEP: f64 = 0.15;

const LINK_CURVE_MIN: f64 = 42.0;
const MIN_BOARD_WIDTH: f64 = 520.0;
const MIN_BOARD_HEIGHT: f64 = 380.0;

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PlacedNode {
    pub id: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl PlacedNode {
    /// Point d'entrée centré sur le bord supérieur de la carte.
    pub fn in_port(&self) -> Point {
        Point {
            x: self.x + self.width / 2.0,
            y: self.y,
        }
    }

    /// Sortie `index` parmi `total`, répartie le long du bord inférieur.
    pub fn out_port(&self, index: usize, total: usize) -> Point {
        let count = total.max(1);
        let share = (index.min(count - 1) + 1) as f64 / (count + 1) as f64;
        Point {
            x: self.x + self.width * share,
            y: self.y + self.height,
        }
    }

    /// Coordonnée d'ancrage d'un port nommé dans la liste des sorties du bloc.
    pub fn out_port_for(&self, port: Option<&str>, ports: &[Option<String>]) -> Point {
        let index = ports
            .iter()
            .position(|candidate| candidate.as_deref() == port)
            .unwrap_or(0);
        self.out_port(index, ports.len().max(1))
    }
}

/// Calcule une disposition topologique centrée par niveau.
pub fn default_positions(graph: &FlowGraph) -> HashMap<String, Point> {
    let grid = layout(graph);
    let mut layer_counts: HashMap<usize, usize> = HashMap::new();
    for pos in grid.values() {
        *layer_counts.entry(pos.layer).or_insert(0) += 1;
    }
    let max_in_rank = layer_counts.values().copied().max().unwrap_or(1);
    let total_width = max_in_rank as f64 * COL_STRIDE;

    graph
        .nodes
        .iter()
        .filter_map(|node| {
            let pos = grid.get(&node.id)?;
            let count_in_rank = layer_counts.get(&pos.layer).copied().unwrap_or(1);
            let rank_width = count_in_rank as f64 * COL_STRIDE;
            let offset_x = (total_width - rank_width) / 2.0;
            Some((
                node.id.clone(),
                Point {
                    x: ORIGIN_X + offset_x + pos.slot as f64 * COL_STRIDE,
                    y: ORIGIN_Y + pos.layer as f64 * ROW_STRIDE,
                },
            ))
        })
        .collect()
}

fn extra_position(extra: &serde_json::Map<String, Value>) -> Option<Point> {
    let pos = extra.get("position")?.as_object()?;
    let x = pos.get("x")?.as_f64()?;
    let y = pos.get("y")?.as_f64()?;
    (x.is_finite() && y.is_finite() && x >= 0.0 && y >= 0.0).then_some(Point { x, y })
}

/// Place chaque bloc en priorisant les positions personnalisées, puis `extra.position`, puis le layout.
pub fn place_with_positions(graph: &FlowGraph, custom: &HashMap<String, Point>) -> Vec<PlacedNode> {
    let defaults = default_positions(graph);
    graph
        .nodes
        .iter()
        .map(|node| {
            let origin = custom
                .get(&node.id)
                .copied()
                .or_else(|| extra_position(&node.extra))
                .or_else(|| defaults.get(&node.id).copied())
                .unwrap_or(Point {
                    x: ORIGIN_X,
                    y: ORIGIN_Y,
                });
            PlacedNode {
                id: node.id.clone(),
                x: origin.x,
                y: origin.y,
                width: CARD_WIDTH,
                height: CARD_HEIGHT,
            }
        })
        .collect()
}

/// Disposition automatique pure depuis le graphe sans coordonnées utilisateur.
#[cfg(test)]
pub fn place(graph: &FlowGraph) -> Vec<PlacedNode> {
    place_with_positions(graph, &HashMap::new())
}

/// Dimensions englobantes `(largeur, hauteur)` du plan de travail.
pub fn board_size(placed: &[PlacedNode]) -> (f64, f64) {
    let max_x = placed
        .iter()
        .map(|node| node.x + node.width + BOARD_PAD)
        .fold(MIN_BOARD_WIDTH, f64::max);
    let max_y = placed
        .iter()
        .map(|node| node.y + node.height + BOARD_PAD)
        .fold(MIN_BOARD_HEIGHT, f64::max);
    (max_x, max_y)
}

/// Courbe de Bézier verticale entre une sortie et une entrée.
pub fn link_path(from: Point, to: Point) -> String {
    let curve = ((to.y - from.y).abs() * 0.5).max(LINK_CURVE_MIN);
    format!(
        "M {:.1} {:.1} C {:.1} {:.1}, {:.1} {:.1}, {:.1} {:.1}",
        from.x,
        from.y,
        from.x,
        from.y + curve,
        to.x,
        to.y - curve,
        to.x,
        to.y
    )
}

/// Milieu géométrique d'une liaison, pour ancrer la pastille de port.
pub fn link_midpoint(from: Point, to: Point) -> Point {
    Point {
        x: (from.x + to.x) / 2.0,
        y: (from.y + to.y) / 2.0,
    }
}

/// Borne le facteur de zoom dans l'intervalle supporté par l'atelier.
pub fn clamp_zoom(zoom: f64) -> f64 {
    if !zoom.is_finite() {
        return 1.0;
    }
    zoom.clamp(ZOOM_MIN, ZOOM_MAX)
}

#[cfg(test)]
mod tests {
    use super::*;
    use flux_shared::flows::FlowBlockKind;
    use serde_json::json;

    #[test]
    fn default_positions_centers_ranks_horizontally() {
        let mut graph = FlowGraph::default();
        let input = graph.append(FlowBlockKind::FileInput, "Entrée", json!({}));
        let cond = graph.append(FlowBlockKind::Condition, "Si", json!({}));
        let yes = graph.append(FlowBlockKind::RemuxMkv, "Oui", json!({}));
        let no = graph.append(FlowBlockKind::Webhook, "Non", json!({}));
        graph.disconnect(&yes, &None);
        let _ = graph.connect(&cond, Some("true".into()), &yes);
        let _ = graph.connect(&cond, Some("false".into()), &no);

        let positions = default_positions(&graph);
        let mid_branch = (positions[&yes].x + positions[&no].x) / 2.0;
        assert!((positions[&input].x - mid_branch).abs() < 0.01);
        assert!((positions[&cond].x - mid_branch).abs() < 0.01);
        assert!(positions[&cond].y > positions[&input].y);
        assert_eq!(positions[&yes].y, positions[&no].y);
    }

    #[test]
    fn ports_and_custom_positions_resolve_cleanly() {
        let mut graph = FlowGraph::default();
        let n1 = graph.append(FlowBlockKind::FileInput, "Entrée", json!({}));
        let mut custom = HashMap::new();
        custom.insert(n1.clone(), Point { x: 400.0, y: 220.0 });
        let placed = place_with_positions(&graph, &custom);
        assert_eq!(placed.len(), 1);
        assert_eq!(placed[0].x, 400.0);
        assert_eq!(placed[0].y, 220.0);

        let in_pt = placed[0].in_port();
        assert_eq!(in_pt.x, 400.0 + CARD_WIDTH / 2.0);
        assert_eq!(in_pt.y, 220.0);

        let ports = vec![Some("true".to_owned()), Some("false".to_owned())];
        let p_true = placed[0].out_port_for(Some("true"), &ports);
        let p_false = placed[0].out_port_for(Some("false"), &ports);
        assert!(p_true.x < p_false.x);
        assert_eq!(p_true.y, 220.0 + CARD_HEIGHT);
    }

    #[test]
    fn board_size_curves_and_zoom_are_bounded() {
        let graph = FlowGraph::default();
        let placed = place(&graph);
        let (w, h) = board_size(&placed);
        assert!(w >= MIN_BOARD_WIDTH && h >= MIN_BOARD_HEIGHT);

        let from = Point { x: 100.0, y: 100.0 };
        let to = Point { x: 200.0, y: 300.0 };
        assert!(link_path(from, to).starts_with("M 100.0 100.0 C"));
        assert_eq!(link_midpoint(from, to), Point { x: 150.0, y: 200.0 });

        assert_eq!(clamp_zoom(0.05), ZOOM_MIN);
        assert_eq!(clamp_zoom(3.0), ZOOM_MAX);
        assert_eq!(clamp_zoom(f64::NAN), 1.0);
    }
}
