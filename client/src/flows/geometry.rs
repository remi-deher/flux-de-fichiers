//! Géométrie du graphe affiché : placement libre 2D, centrage topologique, ports et courbes SVG.

use flux_shared::flows::{FlowGraph, FlowNode, layout};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

pub const CARD_WIDTH: f64 = 248.0;
pub const CARD_HEIGHT: f64 = 92.0;
pub const COL_STRIDE: f64 = 296.0;
pub const ROW_STRIDE: f64 = 148.0;
pub const ORIGIN_X: f64 = 72.0;
pub const ORIGIN_Y: f64 = 64.0;
#[cfg(test)]
pub const BOARD_PAD: f64 = 120.0;
pub const ZOOM_MIN: f64 = 0.25;
pub const ZOOM_MAX: f64 = 1.75;
pub const ZOOM_STEP: f64 = 0.15;
pub const GRID_STEP: f64 = 24.0;

const LINK_CURVE_MIN: f64 = 42.0;
const MIN_BOARD_WIDTH: f64 = 520.0;
const MIN_BOARD_HEIGHT: f64 = 380.0;
const FIT_PADDING: f64 = 48.0;
const FIT_MAX_ZOOM: f64 = 1.0;
const MINIMAP_PAD: f64 = 80.0;

/// Caméra 2D du canevas infini : décalage en pixels écran et facteur de zoom.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Viewport {
    pub pan_x: f64,
    pub pan_y: f64,
    pub zoom: f64,
}

impl Default for Viewport {
    fn default() -> Self {
        Self {
            pan_x: 0.0,
            pan_y: 0.0,
            zoom: 1.0,
        }
    }
}

/// Rectangle englobant en coordonnées graphe.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bounds {
    pub min_x: f64,
    pub min_y: f64,
    pub width: f64,
    pub height: f64,
}

impl Bounds {
    /// Construit un rectangle normalisé entre deux sommets quelconques (4 quadrants).
    pub fn from_points(a: Point, b: Point) -> Self {
        Self {
            min_x: a.x.min(b.x),
            min_y: a.y.min(b.y),
            width: (a.x - b.x).abs(),
            height: (a.y - b.y).abs(),
        }
    }
}

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
    /// Boîte englobante de la carte sur le plan.
    pub fn bounds(&self) -> Bounds {
        Bounds {
            min_x: self.x,
            min_y: self.y,
            width: self.width,
            height: self.height,
        }
    }

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
    (x.is_finite() && y.is_finite()).then_some(Point { x, y })
}

/// Résout l'origine 2D d'un bloc : position personnalisée, puis `extra.position`, puis le layout automatique.
pub fn resolve_node_origin(
    node: Option<&FlowNode>,
    custom: Option<Point>,
    default: Option<Point>,
) -> Point {
    custom
        .or_else(|| node.and_then(|n| extra_position(&n.extra)))
        .or(default)
        .unwrap_or(Point {
            x: ORIGIN_X,
            y: ORIGIN_Y,
        })
}

/// Place chaque bloc en priorisant les positions personnalisées, puis `extra.position`, puis le layout.
pub fn place_with_positions(graph: &FlowGraph, custom: &HashMap<String, Point>) -> Vec<PlacedNode> {
    let defaults = default_positions(graph);
    graph
        .nodes
        .iter()
        .map(|node| {
            let origin = resolve_node_origin(
                Some(node),
                custom.get(&node.id).copied(),
                defaults.get(&node.id).copied(),
            );
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
#[cfg(test)]
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

/// Convertit un point local dans le conteneur canevas vers les coordonnées du graphe.
pub fn screen_to_board(vp: Viewport, local: Point) -> Point {
    let zoom = vp.zoom.max(f64::EPSILON);
    Point {
        x: (local.x - vp.pan_x) / zoom,
        y: (local.y - vp.pan_y) / zoom,
    }
}

/// Convertit un point du graphe vers les coordonnées locales dans le conteneur canevas.
pub fn board_to_screen(vp: Viewport, board: Point) -> Point {
    Point {
        x: board.x * vp.zoom + vp.pan_x,
        y: board.y * vp.zoom + vp.pan_y,
    }
}

/// Applique `next_zoom` en maintenant immobile le point du graphe situé sous `cursor_local`.
pub fn zoom_around(vp: Viewport, cursor_local: Point, next_zoom: f64) -> Viewport {
    let zoom = clamp_zoom(next_zoom);
    let anchor = screen_to_board(vp, cursor_local);
    Viewport {
        pan_x: cursor_local.x - anchor.x * zoom,
        pan_y: cursor_local.y - anchor.y * zoom,
        zoom,
    }
}

/// Centre la caméra courante sur `board_target` dans un canevas de taille `(viewport_w, viewport_h)`.
pub fn center_viewport_on(
    vp: Viewport,
    board_target: Point,
    viewport_w: f64,
    viewport_h: f64,
) -> Viewport {
    Viewport {
        pan_x: viewport_w / 2.0 - board_target.x * vp.zoom,
        pan_y: viewport_h / 2.0 - board_target.y * vp.zoom,
        zoom: vp.zoom,
    }
}

/// Formate la transformation CSS appliquée à la scène HTML du graphe.
pub fn viewport_css_transform(vp: Viewport) -> String {
    format!(
        "transform:translate({:.2}px,{:.2}px) scale({:.4});transform-origin:0 0;",
        vp.pan_x, vp.pan_y, vp.zoom
    )
}

/// Formate l'attribut `transform` SVG appliqué au calque de liaisons pour rester aligné avec la scène.
pub fn viewport_svg_transform(vp: Viewport) -> String {
    format!(
        "translate({:.2} {:.2}) scale({:.4})",
        vp.pan_x, vp.pan_y, vp.zoom
    )
}

/// Calcule le `Viewport` (zoom <= 1.0 et pan) centrant tous les blocs dans la vue.
pub fn fit_viewport(placed: &[PlacedNode], viewport_w: f64, viewport_h: f64) -> Option<Viewport> {
    if placed.is_empty() || viewport_w <= 0.0 || viewport_h <= 0.0 {
        return None;
    }
    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    for node in placed {
        min_x = min_x.min(node.x);
        min_y = min_y.min(node.y);
        max_x = max_x.max(node.x + node.width);
        max_y = max_y.max(node.y + node.height);
    }
    let content_w = (max_x - min_x).max(1.0);
    let content_h = (max_y - min_y).max(1.0);
    let avail_w = (viewport_w - 2.0 * FIT_PADDING).max(1.0);
    let avail_h = (viewport_h - 2.0 * FIT_PADDING).max(1.0);
    let zoom = clamp_zoom(
        (avail_w / content_w)
            .min(avail_h / content_h)
            .min(FIT_MAX_ZOOM),
    );
    let center_x = (min_x + max_x) / 2.0;
    let center_y = (min_y + max_y) / 2.0;
    Some(Viewport {
        pan_x: viewport_w / 2.0 - center_x * zoom,
        pan_y: viewport_h / 2.0 - center_y * zoom,
        zoom,
    })
}

/// Rectangle actuellement visible à l'écran exprimé en coordonnées du graphe.
pub fn visible_board_rect(vp: Viewport, viewport_w: f64, viewport_h: f64) -> Bounds {
    let top_left = screen_to_board(vp, Point { x: 0.0, y: 0.0 });
    let zoom = vp.zoom.max(f64::EPSILON);
    Bounds {
        min_x: top_left.x,
        min_y: top_left.y,
        width: (viewport_w / zoom).max(1.0),
        height: (viewport_h / zoom).max(1.0),
    }
}

/// Boîte englobante dynamique de la minimap (tous les blocs + la zone visible courante).
pub fn minimap_bounds(
    placed: &[PlacedNode],
    vp: Viewport,
    viewport_w: f64,
    viewport_h: f64,
) -> Bounds {
    let vis = visible_board_rect(vp, viewport_w.max(1.0), viewport_h.max(1.0));
    let mut min_x = vis.min_x;
    let mut min_y = vis.min_y;
    let mut max_x = vis.min_x + vis.width;
    let mut max_y = vis.min_y + vis.height;
    for node in placed {
        min_x = min_x.min(node.x - MINIMAP_PAD);
        min_y = min_y.min(node.y - MINIMAP_PAD);
        max_x = max_x.max(node.x + node.width + MINIMAP_PAD);
        max_y = max_y.max(node.y + node.height + MINIMAP_PAD);
    }
    Bounds {
        min_x,
        min_y,
        width: (max_x - min_x).max(MIN_BOARD_WIDTH),
        height: (max_y - min_y).max(MIN_BOARD_HEIGHT),
    }
}

/// Indique si deux rectangles englobants s'intersectent.
pub fn rects_intersect(a: Bounds, b: Bounds) -> bool {
    a.min_x <= b.min_x + b.width
        && a.min_x + a.width >= b.min_x
        && a.min_y <= b.min_y + b.height
        && a.min_y + a.height >= b.min_y
}

/// Aimante une coordonnée graphe sur le pas de grille le plus proche.
pub fn snap_point(pt: Point, step: f64) -> Point {
    if !step.is_finite() || step <= 0.0 || !pt.x.is_finite() || !pt.y.is_finite() {
        return pt;
    }
    Point {
        x: (pt.x / step).round() * step,
        y: (pt.y / step).round() * step,
    }
}

/// Calcule le coin haut-gauche d'une carte centrée sur `board_pt`, aimanté à la grille si `snap` vaut `true`.
pub fn centered_card_origin(board_pt: Point, snap: bool) -> Point {
    let origin = Point {
        x: board_pt.x - CARD_WIDTH / 2.0,
        y: board_pt.y - CARD_HEIGHT / 2.0,
    };
    if snap {
        snap_point(origin, GRID_STEP)
    } else {
        origin
    }
}

/// Indique si le pointeur a franchi le seuil de glissement depuis `(start_x, start_y)`.
pub fn drag_moved_beyond_threshold(
    start_x: f64,
    start_y: f64,
    cur_x: f64,
    cur_y: f64,
    threshold: f64,
) -> bool {
    (cur_x - start_x).hypot(cur_y - start_y) >= threshold
}

#[cfg(test)]
mod tests {
    use super::super::test_support::test_catalog;
    use super::*;
    use flux_shared::flows::FlowBlockKind;
    use serde_json::json;

    #[test]
    fn default_positions_centers_ranks_horizontally() {
        let catalog = test_catalog();
        let mut graph = FlowGraph::default();
        let input = graph.append(
            FlowBlockKind::parse("block.core.file-input"),
            "Entrée",
            json!({}),
            &catalog,
        );
        let cond = graph.append(
            FlowBlockKind::parse("block.core.condition"),
            "Si",
            json!({}),
            &catalog,
        );
        let yes = graph.append(
            FlowBlockKind::parse("block.video.remux-mkv"),
            "Oui",
            json!({}),
            &catalog,
        );
        let no = graph.append(
            FlowBlockKind::parse("block.core.webhook"),
            "Non",
            json!({}),
            &catalog,
        );
        graph.disconnect(&yes, &None);
        let _ = graph.connect(&cond, Some("true".into()), &yes, &catalog);
        let _ = graph.connect(&cond, Some("false".into()), &no, &catalog);

        let positions = default_positions(&graph);
        let mid_branch = (positions[&yes].x + positions[&no].x) / 2.0;
        assert!((positions[&input].x - mid_branch).abs() < 0.01);
        assert!((positions[&cond].x - mid_branch).abs() < 0.01);
        assert!(positions[&cond].y > positions[&input].y);
        assert_eq!(positions[&yes].y, positions[&no].y);
    }

    #[test]
    fn ports_and_custom_positions_resolve_cleanly() {
        let catalog = test_catalog();
        let mut graph = FlowGraph::default();
        let n1 = graph.append(
            FlowBlockKind::parse("block.core.file-input"),
            "Entrée",
            json!({}),
            &catalog,
        );
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

    #[test]
    fn fit_viewport_centers_and_scales_graph() {
        assert_eq!(fit_viewport(&[], 800.0, 600.0), None);
        let small = vec![PlacedNode {
            id: "n1".into(),
            x: 100.0,
            y: 100.0,
            width: CARD_WIDTH,
            height: CARD_HEIGHT,
        }];
        let Some(vp_small) = fit_viewport(&small, 1000.0, 800.0) else {
            panic!("fit_viewport aurait dû calculer un cadrage");
        };
        assert_eq!(vp_small.zoom, 1.0);
        let center_screen = board_to_screen(
            vp_small,
            Point {
                x: 100.0 + CARD_WIDTH / 2.0,
                y: 100.0 + CARD_HEIGHT / 2.0,
            },
        );
        assert!((center_screen.x - 500.0).abs() < 0.01);
        assert!((center_screen.y - 400.0).abs() < 0.01);

        let wide = vec![
            PlacedNode {
                id: "n1".into(),
                x: -400.0,
                y: -250.0,
                width: CARD_WIDTH,
                height: CARD_HEIGHT,
            },
            PlacedNode {
                id: "n2".into(),
                x: 2000.0,
                y: 1200.0,
                width: CARD_WIDTH,
                height: CARD_HEIGHT,
            },
        ];
        let Some(vp_wide) = fit_viewport(&wide, 800.0, 600.0) else {
            panic!("fit_viewport aurait dû dézoomer un grand graphe");
        };
        assert!(vp_wide.zoom < 1.0 && vp_wide.zoom >= ZOOM_MIN);
    }

    #[test]
    fn viewport_conversions_and_zoom_around_preserve_cursor_anchor() {
        let vp = Viewport {
            pan_x: 120.0,
            pan_y: -45.0,
            zoom: 1.25,
        };
        let cursor = Point { x: 420.0, y: 310.0 };
        let board_before = screen_to_board(vp, cursor);
        let back = board_to_screen(vp, board_before);
        assert!((back.x - cursor.x).abs() < 1e-6);
        assert!((back.y - cursor.y).abs() < 1e-6);

        let zoomed = zoom_around(vp, cursor, 0.75);
        let board_after = screen_to_board(zoomed, cursor);
        assert!((board_after.x - board_before.x).abs() < 1e-6);
        assert!((board_after.y - board_before.y).abs() < 1e-6);

        let neg = [PlacedNode {
            id: "n".into(),
            x: -320.0,
            y: -180.0,
            width: CARD_WIDTH,
            height: CARD_HEIGHT,
        }];
        let bounds = minimap_bounds(&neg, Viewport::default(), 800.0, 600.0);
        assert!(bounds.min_x <= -320.0 - MINIMAP_PAD);
        assert!(bounds.min_y <= -180.0 - MINIMAP_PAD);
    }

    #[test]
    fn bounds_from_points_intersection_and_snap_work_across_quadrants() {
        let marquee = Bounds::from_points(
            Point { x: 120.0, y: 80.0 },
            Point {
                x: -150.0,
                y: -120.0,
            },
        );
        assert_eq!(
            marquee,
            Bounds {
                min_x: -150.0,
                min_y: -120.0,
                width: 270.0,
                height: 200.0,
            }
        );

        let inside = PlacedNode {
            id: "in".into(),
            x: -200.0,
            y: -150.0,
            width: CARD_WIDTH,
            height: CARD_HEIGHT,
        };
        let outside = PlacedNode {
            id: "out".into(),
            x: 300.0,
            y: 200.0,
            width: CARD_WIDTH,
            height: CARD_HEIGHT,
        };
        assert!(rects_intersect(marquee, inside.bounds()));
        assert!(!rects_intersect(marquee, outside.bounds()));

        assert_eq!(
            snap_point(Point { x: 35.0, y: -38.0 }, GRID_STEP),
            Point { x: 24.0, y: -48.0 }
        );
    }

    #[test]
    fn resolve_node_origin_prioritizes_custom_then_extra_then_default() {
        let catalog = test_catalog();
        let mut graph = FlowGraph::default();
        let id = graph.append(
            FlowBlockKind::parse("block.core.file-input"),
            "Entrée",
            json!({}),
            &catalog,
        );
        if let Some(node) = graph.node_mut(&id) {
            node.extra
                .insert("position".into(), json!({ "x": 312.0, "y": 168.0 }));
        }
        let default = Point { x: 72.0, y: 64.0 };
        let node = graph.node(&id);

        assert_eq!(
            resolve_node_origin(node, None, Some(default)),
            Point { x: 312.0, y: 168.0 }
        );
        assert_eq!(
            resolve_node_origin(node, Some(Point { x: 480.0, y: 240.0 }), Some(default)),
            Point { x: 480.0, y: 240.0 }
        );
        assert_eq!(resolve_node_origin(None, None, Some(default)), default);
        assert_eq!(
            resolve_node_origin(None, None, None),
            Point {
                x: ORIGIN_X,
                y: ORIGIN_Y
            }
        );
    }

    #[test]
    fn centered_card_origin_and_drag_threshold_behave_accurately() {
        let target = Point { x: 300.0, y: 200.0 };
        let free = centered_card_origin(target, false);
        assert_eq!(
            free,
            Point {
                x: 300.0 - CARD_WIDTH / 2.0,
                y: 200.0 - CARD_HEIGHT / 2.0
            }
        );

        let snapped = centered_card_origin(target, true);
        assert_eq!(snapped.x % GRID_STEP, 0.0);
        assert_eq!(snapped.y % GRID_STEP, 0.0);

        assert!(!drag_moved_beyond_threshold(10.0, 20.0, 12.0, 22.0, 4.0));
        assert!(drag_moved_beyond_threshold(10.0, 20.0, 14.0, 20.0, 4.0));
        assert!(drag_moved_beyond_threshold(10.0, 20.0, 13.0, 23.0, 4.0));
    }

    #[test]
    fn viewport_transform_strings_share_exact_coordinates_and_zoom() {
        let vp = Viewport {
            pan_x: -120.5,
            pan_y: 84.25,
            zoom: 1.3,
        };
        assert_eq!(
            viewport_css_transform(vp),
            "transform:translate(-120.50px,84.25px) scale(1.3000);transform-origin:0 0;"
        );
        assert_eq!(
            viewport_svg_transform(vp),
            "translate(-120.50 84.25) scale(1.3000)"
        );
    }
}
