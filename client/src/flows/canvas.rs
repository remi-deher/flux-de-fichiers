//! Plan interactif 2D du graphe : cartes librement déplaçables, zoom/pan, ports et minimap.

use super::dnd::{BranchTarget, DragPayload, can_connect, connect_checked};
use super::drag::{DragItem, is_related_inside_current_target};
use super::geometry::{
    Bounds, CARD_HEIGHT, CARD_WIDTH, GRID_STEP, ORIGIN_X, ORIGIN_Y, PlacedNode, Point, ZOOM_STEP,
    board_to_screen, center_viewport_on, centered_card_origin, drag_moved_beyond_threshold,
    fit_viewport, link_midpoint, link_path, minimap_bounds, place_with_positions, rects_intersect,
    screen_to_board, viewport_css_transform, viewport_svg_transform, visible_board_rect,
    zoom_around,
};
use super::presentation::{
    NodeDomain, PortPresentation, node_chips, node_output_ports, output_presentation,
};
use super::quick_add::QuickAddPopover;
use super::state::{Editor, PendingWire, QuickAddState};
use crate::route::{Link, Route};
use crate::ui::{Icon, IconName, Toasts};
use flux_shared::flows::{FlowBlockKind, FlowConnection, find_owned};
use leptos::prelude::*;
use std::collections::{HashMap, HashSet};

const NODE_ICON_SIZE: u32 = 15;
const ACTION_ICON_SIZE: u32 = 14;
const TOOLBAR_ICON_SIZE: u32 = 14;
const WIRE_BADGE_WIDTH: f64 = 60.0;
const WIRE_BADGE_HEIGHT: f64 = 20.0;
const WIRE_BADGE_RX: f64 = 10.0;
const WIRE_ENDPOINT_RADIUS: f64 = 3.5;
const WIRE_INSERT_RADIUS: f64 = 11.0;
const WIRE_BADGE_OFFSET_Y: f64 = 13.0;
const WIRE_INSERT_OFFSET_Y: f64 = 13.0;
const ZOOM_PERCENT_SCALE: f64 = 100.0;
const MINI_NODE_RX: f64 = 8.0;
const DEFAULT_ZOOM: f64 = 1.0;
const NODE_DRAG_THRESHOLD: f64 = 4.0;
const WIRE_DRAG_THRESHOLD: f64 = 8.0;

#[derive(Clone, Debug)]
struct DragNodeState {
    primary_id: String,
    start_x: f64,
    start_y: f64,
    origins: Vec<(String, Point)>,
    moved: bool,
}

#[derive(Clone, Debug)]
struct DropPreviewState {
    label: String,
    pos: Point,
}

#[derive(Clone, Copy, Debug)]
struct MarqueeState {
    start: Point,
    current: Point,
}

#[derive(Clone, Copy, Debug)]
struct PanBoardState {
    start_x: f64,
    start_y: f64,
    origin_pan_x: f64,
    origin_pan_y: f64,
}

fn board_local_point(board: &web_sys::HtmlDivElement, client_x: i32, client_y: i32) -> Point {
    let rect = board.get_bounding_client_rect();
    Point {
        x: f64::from(client_x) - rect.left(),
        y: f64::from(client_y) - rect.top(),
    }
}

fn board_dimensions(board_ref: NodeRef<leptos::html::Div>) -> (f64, f64) {
    board_ref.get().map_or((960.0, 640.0), |el| {
        (
            f64::from(el.client_width()).max(1.0),
            f64::from(el.client_height()).max(1.0),
        )
    })
}

struct OutputSlotData {
    port: Option<String>,
    pres: PortPresentation,
    port_idx: usize,
    existing_to: Option<String>,
}

fn wire_badge_label(port: &str) -> String {
    let pres = output_presentation(port);
    format!("{} {}", pres.symbol, pres.label)
}

#[component]
fn CanvasToolbar(editor: Editor, can_manage: bool) -> impl IntoView {
    move || {
        let can_undo_layout = editor.previous_positions.get().is_some();
        let pending = editor.pending_wire.get();
        let multi_count = editor.selected_ids.with(HashSet::len);
        let notice = editor.drop_notice.get();
        if !can_undo_layout && pending.is_none() && multi_count <= 1 && notice.is_none() {
            return None;
        }
        Some(view! {
            <div class="flow-canvas__toolbar" role="status">
                {can_undo_layout.then(|| view! {
                    <button
                        type="button"
                        class="btn btn-secondary btn-sm"
                        on:click=move |_| editor.undo_layout()
                    >
                        <Icon name=IconName::Undo size=TOOLBAR_ICON_SIZE />
                        <span>"Annuler la disposition"</span>
                    </button>
                })}
                {pending.as_ref().map(|_| view! {
                    <div class="flow-canvas__banner">
                        <span>"Liaison en cours : cliquez sur l'entrée du bloc cible."</span>
                        <button
                            type="button"
                            class="btn btn-secondary btn-sm"
                            on:click=move |_| editor.pending_wire.set(None)
                        >
                            "Annuler la liaison"
                        </button>
                    </div>
                })}
                {(multi_count > 1).then(|| view! {
                    <div class="flow-canvas__banner">
                        <span>{format!("{multi_count} blocs sélectionnés")}</span>
                        {can_manage.then(|| view! {
                            <>
                                <button
                                    type="button"
                                    class="btn btn-secondary btn-sm"
                                    title="Dupliquer la sélection (Ctrl+D)"
                                    on:click=move |_| {
                                        let _ = editor.duplicate_selected();
                                    }
                                >
                                    <Icon name=IconName::Copy size=TOOLBAR_ICON_SIZE />
                                    <span>"Dupliquer"</span>
                                </button>
                                <button
                                    type="button"
                                    class="btn btn-danger-quiet btn-sm"
                                    title="Supprimer la sélection (Suppr)"
                                    on:click=move |_| {
                                        let _ = editor.remove_selected_nodes();
                                    }
                                >
                                    <Icon name=IconName::Trash size=TOOLBAR_ICON_SIZE />
                                    <span>"Supprimer"</span>
                                </button>
                            </>
                        })}
                        <button
                            type="button"
                            class="btn btn-secondary btn-sm"
                            on:click=move |_| editor.clear_selection()
                        >
                            "Désélectionner"
                        </button>
                    </div>
                })}
                {notice.map(|message| view! {
                    <div class="flow-canvas__banner is-error">
                        <span>{message}</span>
                        <button
                            type="button"
                            class="header-icon-btn"
                            aria-label="Fermer le message"
                            on:click=move |_| editor.drop_notice.set(None)
                        >
                            <Icon name=IconName::Close size=TOOLBAR_ICON_SIZE />
                        </button>
                    </div>
                })}
            </div>
        })
    }
}

#[component]
fn WireActionPopover(
    editor: Editor,
    placed: Memo<Vec<PlacedNode>>,
    can_manage: bool,
) -> impl IntoView {
    move || {
        if !can_manage {
            return None;
        }
        let conn = editor.selected_wire.get()?;
        let vp = editor.viewport.get();
        let mid_board = placed.with(|nodes| {
            let from_node = editor.graph.with(|g| g.node(&conn.from).cloned())?;
            let from_placed = nodes.iter().find(|n| n.id == conn.from)?;
            let to_placed = nodes.iter().find(|n| n.id == conn.to)?;
            let ports = editor
                .catalog
                .with(|cat| editor.graph.with(|g| node_output_ports(g, &from_node, cat)));
            let start = from_placed.out_port_for(conn.port.as_deref(), &ports);
            Some(link_midpoint(start, to_placed.in_port()))
        })?;
        let screen = board_to_screen(vp, mid_board);
        let insert_conn = conn.clone();
        Some(view! {
            <div
                class="flow-wire__popover"
                role="toolbar"
                aria-label="Actions sur la liaison sélectionnée"
                style=format!("left:{:.1}px;top:{:.1}px;", screen.x, screen.y)
                on:mousedown=move |event| event.stop_propagation()
                on:click=move |event| event.stop_propagation()
            >
                <button
                    type="button"
                    class="btn btn-secondary btn-sm"
                    title="Insérer une étape sur cette liaison"
                    on:click=move |_| {
                        editor.open_quick_add(QuickAddState {
                            anchor: screen,
                            board_pos: None,
                            branch: Some(BranchTarget {
                                source_id: insert_conn.from.clone(),
                                port: insert_conn.port.clone(),
                                replace_to: Some(insert_conn.to.clone()),
                            }),
                        });
                    }
                >
                    <Icon name=IconName::Plus size=TOOLBAR_ICON_SIZE />
                    <span>"Insérer"</span>
                </button>
                <button
                    type="button"
                    class="btn btn-danger-quiet btn-sm"
                    title="Supprimer cette liaison (Suppr)"
                    aria-label="Supprimer cette liaison"
                    on:click=move |_| {
                        let _ = editor.disconnect_selected_wire();
                    }
                >
                    <Icon name=IconName::Trash size=TOOLBAR_ICON_SIZE />
                </button>
            </div>
        })
    }
}

#[component]
fn CanvasControlsBar(
    editor: Editor,
    placed: Memo<Vec<PlacedNode>>,
    board_ref: NodeRef<leptos::html::Div>,
    can_manage: bool,
) -> impl IntoView {
    let zoom_center = move |target_zoom: Option<f64>, delta: f64| {
        let (vw, vh) = board_dimensions(board_ref);
        let center = Point {
            x: vw / 2.0,
            y: vh / 2.0,
        };
        editor.viewport.update(|vp| {
            let next = target_zoom.unwrap_or(vp.zoom + delta);
            *vp = zoom_around(*vp, center, next);
        });
    };

    let fit_all = move |_| {
        let (vw, vh) = board_dimensions(board_ref);
        if let Some(next_vp) = placed.with_untracked(|nodes| fit_viewport(nodes, vw, vh)) {
            editor.viewport.set(next_vp);
        }
    };

    view! {
        <div class="flow-canvas-controls" role="toolbar" aria-label="Contrôles du canevas">
            <button
                type="button"
                class="flow-control-btn"
                title="Zoomer"
                aria-label="Zoomer"
                on:click=move |_| zoom_center(None, ZOOM_STEP)
            >
                <Icon name=IconName::Plus size=TOOLBAR_ICON_SIZE />
            </button>
            <button
                type="button"
                class="flow-control-btn"
                title="Dézoomer"
                aria-label="Dézoomer"
                on:click=move |_| zoom_center(None, -ZOOM_STEP)
            >
                <Icon name=IconName::Minus size=TOOLBAR_ICON_SIZE />
            </button>
            <button
                type="button"
                class="flow-control-btn flow-control-btn--pct"
                title="Réinitialiser le zoom à 100 %"
                aria-label="Réinitialiser le zoom à 100 %"
                on:click=move |_| zoom_center(Some(DEFAULT_ZOOM), 0.0)
            >
                {move || format!("{}%", (editor.viewport.get().zoom * ZOOM_PERCENT_SCALE).round())}
            </button>
            <button
                type="button"
                class="flow-control-btn"
                title="Adapter tout le flux à l'écran"
                aria-label="Adapter tout le flux à l'écran"
                on:click=fit_all
            >
                <Icon name=IconName::Maximize size=TOOLBAR_ICON_SIZE />
            </button>

            {can_manage.then(|| view! {
                <>
                    <span class="flow-controls-sep" aria-hidden="true"></span>
                    <button
                        type="button"
                        class="flow-control-btn"
                        class:is-active=move || editor.snap_to_grid.get()
                        aria-pressed=move || editor.snap_to_grid.get().to_string()
                        title="Aimantation sur la grille 24 px (Alt pour ignorer)"
                        aria-label="Activer ou désactiver l'aimantation sur la grille"
                        on:click=move |_| editor.snap_to_grid.update(|snap| *snap = !*snap)
                    >
                        <Icon name=IconName::Magnet size=TOOLBAR_ICON_SIZE />
                    </button>
                    <button
                        type="button"
                        class="flow-control-btn"
                        title="Réorganiser automatiquement les blocs"
                        aria-label="Réorganiser automatiquement les blocs"
                        on:click=move |_| editor.auto_layout()
                    >
                        <Icon name=IconName::Layout size=TOOLBAR_ICON_SIZE />
                    </button>
                    <button
                        type="button"
                        class="flow-control-btn"
                        title="Annuler (Ctrl+Z)"
                        aria-label="Annuler la dernière modification"
                        disabled=move || !editor.can_undo()
                        on:click=move |_| editor.undo()
                    >
                        <Icon name=IconName::Undo size=TOOLBAR_ICON_SIZE />
                    </button>
                    <button
                        type="button"
                        class="flow-control-btn"
                        title="Rétablir (Ctrl+Maj+Z)"
                        aria-label="Rétablir la modification"
                        disabled=move || !editor.can_redo()
                        on:click=move |_| editor.redo()
                    >
                        <Icon name=IconName::Redo size=TOOLBAR_ICON_SIZE />
                    </button>
                </>
            })}
        </div>
    }
}

#[component]
fn MinimapPanel(
    editor: Editor,
    placed: Memo<Vec<PlacedNode>>,
    board_ref: NodeRef<leptos::html::Div>,
) -> impl IntoView {
    let minimap_ref = NodeRef::<leptos::html::Div>::new();
    view! {
        <div class="flow-minimap-panel">
            <div
                node_ref=minimap_ref
                style="cursor:pointer;"
                title="Cliquer pour centrer le canevas sur cette zone"
                on:click=move |event| {
                    let Some(mini) = minimap_ref.get() else {
                        return;
                    };
                    let (vw, vh) = board_dimensions(board_ref);
                    let vp = editor.viewport.get_untracked();
                    let bounds = placed.with_untracked(|nodes| minimap_bounds(nodes, vp, vw, vh));
                    let mini_w = f64::from(mini.client_width()).max(1.0);
                    let mini_h = f64::from(mini.client_height()).max(1.0);
                    let scale = (mini_w / bounds.width).min(mini_h / bounds.height).max(f64::EPSILON);
                    let pad_x = (mini_w - bounds.width * scale) / 2.0;
                    let pad_y = (mini_h - bounds.height * scale) / 2.0;
                    let target = Point {
                        x: bounds.min_x + (f64::from(event.offset_x()) - pad_x) / scale,
                        y: bounds.min_y + (f64::from(event.offset_y()) - pad_y) / scale,
                    };
                    editor.viewport.set(center_viewport_on(vp, target, vw, vh));
                }
            >
                {move || {
                    let (vw, vh) = board_dimensions(board_ref);
                    let vp = editor.viewport.get();
                    let nodes = placed.get();
                    let bounds = minimap_bounds(&nodes, vp, vw, vh);
                    let vis = visible_board_rect(vp, vw, vh);
                    let graph = editor.graph.get();
                    let map_by_id: HashMap<&str, &PlacedNode> =
                        nodes.iter().map(|p| (p.id.as_str(), p)).collect();
                    let lines = graph
                        .connections
                        .iter()
                        .filter_map(|edge| {
                            let from = map_by_id.get(edge.from.as_str())?;
                            let to = map_by_id.get(edge.to.as_str())?;
                            let a = Point {
                                x: from.x + from.width / 2.0,
                                y: from.y + from.height,
                            };
                            let b = to.in_port();
                            Some(view! {
                                <line class="mini-wire" x1=a.x y1=a.y x2=b.x y2=b.y />
                            })
                        })
                        .collect_view();
                    let rects = nodes
                        .iter()
                        .map(|node| {
                            let is_selected = editor.is_selected(&node.id);
                            view! {
                                <rect
                                    class="mini-node"
                                    class:is-selected=is_selected
                                    x=node.x
                                    y=node.y
                                    width=node.width
                                    height=node.height
                                    rx=MINI_NODE_RX
                                />
                            }
                        })
                        .collect_view();
                    view! {
                        <svg
                            class="flow-minimap__svg"
                            style="pointer-events:none;"
                            viewBox=format!("{} {} {} {}", bounds.min_x, bounds.min_y, bounds.width, bounds.height)
                            aria-hidden="true"
                        >
                            {lines}
                            {rects}
                            <rect
                                class="mini-viewport"
                                x=vis.min_x
                                y=vis.min_y
                                width=vis.width
                                height=vis.height
                                rx=MINI_NODE_RX
                            />
                        </svg>
                    }
                }}
            </div>
        </div>
    }
}

#[component]
pub fn GraphCanvas(editor: Editor, can_manage: bool) -> impl IntoView {
    let board_ref = NodeRef::<leptos::html::Div>::new();
    let dragging_node = RwSignal::new(None::<DragNodeState>);
    let marquee = RwSignal::new(None::<MarqueeState>);
    let panning = RwSignal::new(None::<PanBoardState>);
    let drop_preview = RwSignal::new(None::<DropPreviewState>);
    let wire_opened_on_press = RwSignal::new(false);
    let ignore_click_for_node = RwSignal::new(None::<String>);

    let placed = Memo::new(move |_| {
        editor.graph.with(|graph| {
            editor
                .positions
                .with(|custom| place_with_positions(graph, custom))
        })
    });

    let _window_move = window_event_listener(leptos::ev::mousemove, move |event| {
        let cx = f64::from(event.client_x());
        let cy = f64::from(event.client_y());
        if let Some(mut drag) = dragging_node.get_untracked() {
            if !drag.moved
                && !drag_moved_beyond_threshold(
                    drag.start_x,
                    drag.start_y,
                    cx,
                    cy,
                    NODE_DRAG_THRESHOLD,
                )
            {
                return;
            }
            if !drag.moved {
                drag.moved = true;
                if !editor.is_selected(&drag.primary_id) {
                    editor.select_node(Some(drag.primary_id.clone()), false);
                }
                dragging_node.set(Some(drag.clone()));
            }
            let zoom = editor.viewport.get_untracked().zoom.max(f64::EPSILON);
            let dx = (cx - drag.start_x) / zoom;
            let dy = (cy - drag.start_y) / zoom;
            let snap = editor.snap_to_grid.get_untracked() ^ event.alt_key();
            let updates: Vec<(String, Point)> = drag
                .origins
                .iter()
                .map(|(id, origin)| {
                    (
                        id.clone(),
                        Point {
                            x: origin.x + dx,
                            y: origin.y + dy,
                        },
                    )
                })
                .collect();
            editor.move_nodes_live(&updates, snap);
            return;
        }
        if let Some(mut box_sel) = marquee.get_untracked()
            && let Some(board) = board_ref.get()
        {
            let local = board_local_point(&board, event.client_x(), event.client_y());
            let board_pt = screen_to_board(editor.viewport.get_untracked(), local);
            box_sel.current = board_pt;
            marquee.set(Some(box_sel));
            let sel_rect = Bounds::from_points(box_sel.start, box_sel.current);
            let hits: HashSet<String> = placed.with_untracked(|nodes| {
                nodes
                    .iter()
                    .filter(|node| rects_intersect(sel_rect, node.bounds()))
                    .map(|node| node.id.clone())
                    .collect()
            });
            editor.set_selection(hits);
            return;
        }
        if let Some(pan) = panning.get_untracked() {
            editor.viewport.update(|vp| {
                vp.pan_x = pan.origin_pan_x + (cx - pan.start_x);
                vp.pan_y = pan.origin_pan_y + (cy - pan.start_y);
            });
            return;
        }
        if editor.pending_wire.get_untracked().is_some() {
            let zoom = editor.viewport.get_untracked().zoom.max(f64::EPSILON);
            editor.pending_wire.update(|wire| {
                if let Some(w) = wire.as_mut() {
                    w.cursor = Point {
                        x: w.start_anchor.x + (cx - w.start_client_x) / zoom,
                        y: w.start_anchor.y + (cy - w.start_client_y) / zoom,
                    };
                }
            });
        }
    });

    let _window_up = window_event_listener(leptos::ev::mouseup, move |event| {
        if let Some(drag) = dragging_node.get_untracked() {
            dragging_node.set(None);
            if drag.moved {
                editor.commit_positions();
                ignore_click_for_node.set(Some(drag.primary_id));
            }
        }
        marquee.set(None);
        panning.set(None);
        if let Some(wire) = editor.pending_wire.get_untracked() {
            let moved = (wire.cursor.x - wire.start_anchor.x).abs() > WIRE_DRAG_THRESHOLD
                || (wire.cursor.y - wire.start_anchor.y).abs() > WIRE_DRAG_THRESHOLD;
            if moved {
                if can_manage {
                    let anchor = board_ref
                        .get()
                        .map(|b| board_local_point(&b, event.client_x(), event.client_y()))
                        .unwrap_or_else(|| {
                            board_to_screen(editor.viewport.get_untracked(), wire.cursor)
                        });
                    let existing_to = editor.graph.with_untracked(|graph| {
                        graph
                            .outgoing(&wire.from_id)
                            .find(|edge| edge.port == wire.port)
                            .map(|edge| edge.to.clone())
                    });
                    editor.open_quick_add(QuickAddState {
                        anchor,
                        board_pos: Some(editor.snap_if_enabled(Point {
                            x: wire.cursor.x - CARD_WIDTH / 2.0,
                            y: wire.cursor.y,
                        })),
                        branch: Some(BranchTarget {
                            source_id: wire.from_id,
                            port: wire.port,
                            replace_to: existing_to,
                        }),
                    });
                } else {
                    editor.pending_wire.set(None);
                }
            }
        }
    });

    let connect_to_node = move |target_id: String| {
        if !can_manage {
            return;
        }
        let Some(wire) = editor.pending_wire.get_untracked() else {
            return;
        };
        editor.pending_wire.set(None);
        let catalog = editor.catalog.get_untracked();
        let mut outcome = Ok(());
        editor.mutate_graph(|graph| {
            outcome = connect_checked(graph, &wire.from_id, wire.port, &target_id, &catalog);
        });
        match outcome {
            Ok(()) => Toasts::notify_success("Liaison créée."),
            Err(err) => Toasts::notify_error(err.message()),
        }
    };

    view! {
        <section class="flow-canvas-area" aria-label="Plan du graphe">
            <CanvasToolbar editor=editor can_manage=can_manage />
            <div
                node_ref=board_ref
                class="flow-graph-board"
                class:is-panning=move || panning.get().is_some()
                style=move || {
                    let vp = editor.viewport.get();
                    let step = GRID_STEP * vp.zoom;
                    format!(
                        "background-size:{step:.2}px {step:.2}px;background-position:{:.2}px {:.2}px;",
                        vp.pan_x, vp.pan_y
                    )
                }
                on:mousedown=move |event| {
                    ignore_click_for_node.set(None);
                    if event.button() != 0 {
                        return;
                    }
                    if dragging_node.get_untracked().is_some() {
                        return;
                    }
                    editor.selected_wire.set(None);
                    editor.pending_wire.set(None);
                    editor.branch_target.set(None);
                    editor.quick_add.set(None);
                    let vp = editor.viewport.get_untracked();
                    if event.shift_key()
                        && let Some(board) = board_ref.get()
                    {
                        event.prevent_default();
                        editor.clear_selection();
                        let local = board_local_point(&board, event.client_x(), event.client_y());
                        let board_pt = screen_to_board(vp, local);
                        marquee.set(Some(MarqueeState {
                            start: board_pt,
                            current: board_pt,
                        }));
                        return;
                    }
                    panning.set(Some(PanBoardState {
                        start_x: f64::from(event.client_x()),
                        start_y: f64::from(event.client_y()),
                        origin_pan_x: vp.pan_x,
                        origin_pan_y: vp.pan_y,
                    }));
                }
                on:dblclick=move |event| {
                    if !can_manage {
                        return;
                    }
                    let Some(board) = board_ref.get() else {
                        return;
                    };
                    let anchor = board_local_point(&board, event.client_x(), event.client_y());
                    let board_pt = screen_to_board(editor.viewport.get_untracked(), anchor);
                    let snap = editor.snap_to_grid.get_untracked() ^ event.alt_key();
                    let snapped = centered_card_origin(board_pt, snap);
                    editor.open_quick_add(QuickAddState {
                        anchor,
                        board_pos: Some(snapped),
                        branch: None,
                    });
                }
                on:wheel=move |event| {
                    event.prevent_default();
                    let Some(board) = board_ref.get() else {
                        return;
                    };
                    let cursor_local = board_local_point(&board, event.client_x(), event.client_y());
                    let delta = if event.delta_y() < 0.0 {
                        ZOOM_STEP
                    } else {
                        -ZOOM_STEP
                    };
                    editor.viewport.update(|vp| {
                        *vp = zoom_around(*vp, cursor_local, vp.zoom + delta);
                    });
                }
                on:dragover=move |event| {
                    if !can_manage {
                        return;
                    }
                    let Some(DragPayload::Palette { label, .. }) = editor.drag.get_untracked() else {
                        drop_preview.set(None);
                        return;
                    };
                    event.prevent_default();
                    if let Some(transfer) = event.data_transfer() {
                        transfer.set_drop_effect("copy");
                    }
                    let Some(board) = board_ref.get() else {
                        return;
                    };
                    let local = board_local_point(&board, event.client_x(), event.client_y());
                    let board_pt = screen_to_board(editor.viewport.get_untracked(), local);
                    let snap = editor.snap_to_grid.get_untracked() ^ event.alt_key();
                    drop_preview.set(Some(DropPreviewState {
                        label,
                        pos: centered_card_origin(board_pt, snap),
                    }));
                }
                on:dragleave=move |event| {
                    if !is_related_inside_current_target(&event) {
                        drop_preview.set(None);
                    }
                }
                on:drop=move |event| {
                    drop_preview.set(None);
                    if !can_manage {
                        return;
                    }
                    let Some(DragPayload::Palette {
                        kind,
                        label,
                        defaults,
                    }) = editor.drag.get_untracked()
                    else {
                        return;
                    };
                    event.prevent_default();
                    editor.end_drag();
                    let catalog = editor.catalog.get_untracked();
                    let kind_enum = FlowBlockKind::parse(&kind);
                    let accepts_input = find_owned(&catalog, &kind_enum).is_none_or(|spec| spec.has_input);
                    if !accepts_input && let Some(existing) = editor.existing_entry_id() {
                        editor.select_node(Some(existing), true);
                        Toasts::notify_error("Ce flux possède déjà un bloc d'entrée.");
                        return;
                    }
                    let mut created = String::new();
                    editor.mutate_graph(|graph| {
                        created = graph.append(kind_enum, &label, defaults, &catalog);
                    });
                    let snap = editor.snap_to_grid.get_untracked() ^ event.alt_key();
                    let drop_pt = board_ref.get().map(|board| {
                        let local = board_local_point(&board, event.client_x(), event.client_y());
                        let board_pt = screen_to_board(editor.viewport.get_untracked(), local);
                        centered_card_origin(board_pt, snap)
                    });
                    if let Some(pt) = drop_pt {
                        editor.set_node_position(&created, pt);
                    }
                    editor.select_node(Some(created), true);
                }
            >
                {move || {
                    editor.graph.with(|graph| graph.nodes.is_empty()).then(|| {
                        let catalog_empty = editor.catalog_loaded.get()
                            && editor.catalog.with(|c| c.is_empty());
                        view! {
                            <div class="flow-canvas__empty empty-state-panel">
                                <p class="empty-state-title">
                                    {if catalog_empty {
                                        "Aucun bloc de flux disponible."
                                    } else {
                                        "Aucun bloc dans ce flux."
                                    }}
                                </p>
                                <p class="form-help">
                                    {if catalog_empty {
                                        "Le moteur n'embarque aucun bloc : synchronisez un dépôt d'extensions pour peupler la palette."
                                    } else {
                                        "Ouvrez « + Composants » ou glissez une étape sur le canevas pour démarrer."
                                    }}
                                </p>
                                {catalog_empty.then(|| view! {
                                    <Link to=Route::Plugins class="btn btn-primary btn-sm">
                                        "Ouvrir Extensions"
                                    </Link>
                                })}
                            </div>
                        }
                    })
                }}
                {{
                    let wires_view = move || {
                        let graph = editor.graph.get();
                        let catalog = editor.catalog.get();
                        let nodes_placed = placed.get();
                        let placed_map: HashMap<String, PlacedNode> = nodes_placed
                            .into_iter()
                            .map(|p| (p.id.clone(), p))
                            .collect();
                        let selected_id = editor.selected.get();
                        let selected_wire = editor.selected_wire.get();
                        let sim_step = editor.sim_step.get();
                        let sim_rep = editor.simulation_open.get().then(|| {
                            let input = editor.sim_input.get();
                            super::simulation::simulate_flow(&graph, &catalog, &input)
                        });

                        graph
                            .connections
                            .iter()
                            .filter_map(|edge| {
                                let from_node = graph.node(&edge.from)?;
                                let from_placed = placed_map.get(&edge.from)?;
                                let to_placed = placed_map.get(&edge.to)?;
                                let ports = node_output_ports(&graph, from_node, &catalog);
                                let start = from_placed.out_port_for(edge.port.as_deref(), &ports);
                                let end = to_placed.in_port();
                                let d = link_path(start, end);
                                let mid = link_midpoint(start, end);
                                let pres =
                                    output_presentation(edge.port.as_deref().unwrap_or("default"));
                                let is_selected_wire = selected_wire.as_ref() == Some(edge);
                                let sim_active = sim_rep
                                    .as_ref()
                                    .is_some_and(|rep| rep.edge_is_active(edge, sim_step));
                                let is_highlighted = is_selected_wire
                                    || sim_active
                                    || selected_id.as_deref() == Some(edge.from.as_str())
                                    || selected_id.as_deref() == Some(edge.to.as_str());
                                let is_muted = (selected_id.is_some() || sim_rep.is_some())
                                    && !is_highlighted;
                                let click_edge = FlowConnection {
                                    from: edge.from.clone(),
                                    to: edge.to.clone(),
                                    port: edge.port.clone(),
                                };
                                let insert_edge = click_edge.clone();
                                let named_port =
                                    edge.port.as_deref().filter(|p| !p.is_empty());
                                let badge_y = if can_manage {
                                    mid.y - WIRE_BADGE_OFFSET_Y
                                } else {
                                    mid.y
                                };
                                let insert_y = if named_port.is_some() {
                                    mid.y + WIRE_INSERT_OFFSET_Y
                                } else {
                                    mid.y
                                };
                                let insert_point = Point { x: mid.x, y: insert_y };
                                let open_wire_insert = move || {
                                    let anchor = board_to_screen(
                                        editor.viewport.get_untracked(),
                                        insert_point,
                                    );
                                    editor.open_quick_add(QuickAddState {
                                        anchor,
                                        board_pos: None,
                                        branch: Some(BranchTarget {
                                            source_id: insert_edge.from.clone(),
                                            port: insert_edge.port.clone(),
                                            replace_to: Some(insert_edge.to.clone()),
                                        }),
                                    });
                                };
                                let click_insert = open_wire_insert.clone();
                                let key_insert = open_wire_insert;
                                let badge = named_port.map(|port| {
                                    let label = wire_badge_label(port);
                                    let rx = mid.x - WIRE_BADGE_WIDTH / 2.0;
                                    let ry = badge_y - WIRE_BADGE_HEIGHT / 2.0;
                                    view! {
                                        <g class="flow-wire__badge" data-tone=pres.tone.as_attr()>
                                            <rect
                                                x=rx
                                                y=ry
                                                width=WIRE_BADGE_WIDTH
                                                height=WIRE_BADGE_HEIGHT
                                                rx=WIRE_BADGE_RX
                                            />
                                            <text x=mid.x y=badge_y text-anchor="middle" dominant-baseline="central">
                                                {label}
                                            </text>
                                        </g>
                                    }
                                });
                                let insert_btn = can_manage.then(|| view! {
                                    <g
                                        class="flow-wire__insert-btn"
                                        role="button"
                                        tabindex="0"
                                        aria-label="Insérer une étape sur cette liaison"
                                        on:mousedown=move |event| event.stop_propagation()
                                        on:click=move |event| {
                                            event.stop_propagation();
                                            click_insert();
                                        }
                                        on:keydown=move |event| {
                                            let key = event.key();
                                            if key == "Enter" || key == " " {
                                                event.prevent_default();
                                                event.stop_propagation();
                                                key_insert();
                                            }
                                        }
                                    >
                                        <circle
                                            class="flow-wire__insert-circle"
                                            cx=mid.x
                                            cy=insert_y
                                            r=WIRE_INSERT_RADIUS
                                        />
                                        <text
                                            class="flow-wire__insert-plus"
                                            x=mid.x
                                            y=insert_y
                                            text-anchor="middle"
                                            dominant-baseline="central"
                                        >
                                            "+"
                                        </text>
                                    </g>
                                });
                                let tone_attr = pres.tone.as_attr();
                                Some(view! {
                                    <g
                                        class="flow-wire-group"
                                        class:is-highlighted=is_highlighted
                                        data-tone=tone_attr
                                    >
                                        <path
                                            class="flow-wire__hit"
                                            d=d.clone()
                                            on:mousedown=move |event| event.stop_propagation()
                                            on:click=move |event| {
                                                event.stop_propagation();
                                                editor.selected_wire.set(Some(click_edge.clone()));
                                            }
                                        />
                                        {(is_highlighted || sim_active).then(|| view! {
                                            <path
                                                class="flow-wire__glow"
                                                data-tone=tone_attr
                                                d=d.clone()
                                            />
                                        })}
                                        <path
                                            class="flow-wire"
                                            class:is-highlighted=is_highlighted
                                            class:is-sim-path=sim_active
                                            class:is-muted=is_muted
                                            data-tone=tone_attr
                                            d=d.clone()
                                        />
                                        {(is_highlighted || sim_active).then(|| view! {
                                            <path
                                                class="flow-wire__flow"
                                                data-tone=tone_attr
                                                d=d
                                            />
                                        })}
                                        <circle
                                            class="flow-wire__endpoint"
                                            class:is-muted=is_muted
                                            data-tone=tone_attr
                                            cx=start.x
                                            cy=start.y
                                            r=WIRE_ENDPOINT_RADIUS
                                        />
                                        <circle
                                            class="flow-wire__endpoint"
                                            class:is-muted=is_muted
                                            data-tone=tone_attr
                                            cx=end.x
                                            cy=end.y
                                            r=WIRE_ENDPOINT_RADIUS
                                        />
                                        {badge}
                                        {insert_btn}
                                    </g>
                                })
                            })
                            .collect_view()
                    };

                    let ghost_wire_view = move || {
                        let wire = editor.pending_wire.get()?;
                        let nodes_placed = placed.get();
                        let from_placed = nodes_placed.iter().find(|p| p.id == wire.from_id)?;
                        let start = editor.graph.with(|graph| {
                            let from_node = graph.node(&wire.from_id)?;
                            let catalog = editor.catalog.get();
                            let ports = node_output_ports(graph, from_node, &catalog);
                            Some(from_placed.out_port_for(wire.port.as_deref(), &ports))
                        })?;
                        Some(view! {
                            <path class="flow-wire flow-wire--ghost" d=link_path(start, wire.cursor) />
                        })
                    };

                    let cards_view = move || {
                        let graph = editor.graph.get();
                        let catalog = editor.catalog.get();
                        let sim_step = editor.sim_step.get();
                        let sim_rep = editor.simulation_open.get().then(|| {
                            let input = editor.sim_input.get();
                            super::simulation::simulate_flow(&graph, &catalog, &input)
                        });
                        let sim_visited = sim_rep
                            .as_ref()
                            .map(|rep| rep.visited_nodes(sim_step));
                        let sim_active_id = sim_rep
                            .as_ref()
                            .and_then(|rep| rep.active_node(sim_step).map(str::to_owned));

                        graph
                            .nodes
                            .iter()
                            .enumerate()
                            .map(|(idx, node)| {
                                let step_num = idx + 1;
                                let spec = find_owned(&catalog, &node.kind);
                                let category = spec.map_or("Bloc", |s| s.category.as_str());
                                let kind_label = spec.map_or_else(
                                    || node.kind.as_str().to_owned(),
                                    |s| s.label.clone(),
                                );
                                let show_kind_label =
                                    !node.name.trim().eq_ignore_ascii_case(kind_label.trim());
                                let domain = NodeDomain::from_spec(spec, category);
                                let domain_label = domain.label();
                                let incomplete =
                                    spec.is_some_and(|s| !s.missing_fields(&node.config).is_empty());
                                let chips = node_chips(node, spec);
                                let has_input = spec.is_none_or(|s| s.has_input);
                                let ports = node_output_ports(&graph, node, &catalog);
                                let port_count = ports.len();

                                let output_slots: Vec<OutputSlotData> = ports
                                    .into_iter()
                                    .enumerate()
                                    .map(|(port_idx, port)| {
                                        let port_key = port.as_deref().unwrap_or("default");
                                        let pres = output_presentation(port_key);
                                        let existing_to = graph
                                            .outgoing(&node.id)
                                            .find(|edge| edge.port == port)
                                            .map(|edge| edge.to.clone());
                                        OutputSlotData {
                                            port,
                                            pres,
                                            port_idx,
                                            existing_to,
                                        }
                                    })
                                    .collect();

                                let id = node.id.clone();
                                let node_name = node.name.clone();
                                let style_id = id.clone();
                                let drag_id = id.clone();
                                let up_main_id = id.clone();
                                let click_main_id = id.clone();
                                let input_id = id.clone();
                                let valid_id = id.clone();
                                let dup_id = id.clone();
                                let remove_id = id.clone();
                                let selected_check = id.clone();
                                let dragging_check = id.clone();
                                let is_sim_visited = sim_visited
                                    .as_ref()
                                    .is_some_and(|set| set.contains(&id));
                                let is_sim_active = sim_active_id.as_deref() == Some(id.as_str());
                                let domain_icon = domain.icon();
                                let accent_var = domain.accent_var();
                                let slot_style = Signal::derive(move || {
                                    let (x, y) = placed.with(|nodes| {
                                        nodes
                                            .iter()
                                            .find(|p| p.id == style_id)
                                            .map_or((ORIGIN_X, ORIGIN_Y), |p| (p.x, p.y))
                                    });
                                    format!(
                                        "left:{x}px;top:{y}px;width:{CARD_WIDTH}px;height:{CARD_HEIGHT}px;--node-accent:{accent_var};"
                                    )
                                });
                                let subtitle = spec
                                    .and_then(|s| {
                                        (!s.description.trim().is_empty())
                                            .then(|| s.description.clone())
                                    })
                                    .unwrap_or_else(|| domain.default_hint().to_owned());

                                view! {
                                    <DragItem
                                        editor=editor
                                        payload=DragPayload::Card(id.clone())
                                        target=id.clone()
                                        draggable=false
                                        class="canvas-node-slot"
                                        style=slot_style
                                    >
                                        <article
                                            class="canvas-node"
                                            class:is-selected=move || editor.is_selected(&selected_check)
                                            class:is-incomplete=incomplete
                                            class:is-sim-visited=is_sim_visited
                                            class:is-sim-active=is_sim_active
                                            class:is-dragging=move || dragging_node.with(|d| {
                                                d.as_ref().is_some_and(|s| {
                                                    s.moved
                                                        && (s.primary_id == dragging_check
                                                            || s.origins.iter().any(|(id, _)| id == &dragging_check))
                                                })
                                            })
                                            on:mousedown=move |event| {
                                                if event.button() != 0 {
                                                    return;
                                                }
                                                event.stop_propagation();
                                                ignore_click_for_node.set(None);
                                                if !can_manage || editor.pending_wire.get_untracked().is_some() {
                                                    return;
                                                }
                                                event.prevent_default();
                                                let mut active = editor.selected_ids.get_untracked();
                                                if !active.contains(&drag_id) {
                                                    active.clear();
                                                    active.insert(drag_id.clone());
                                                }
                                                let origins: Vec<(String, Point)> = active
                                                    .into_iter()
                                                    .map(|id| {
                                                        let pt = editor.node_point(&id);
                                                        (id, pt)
                                                    })
                                                    .collect();
                                                dragging_node.set(Some(DragNodeState {
                                                    primary_id: drag_id.clone(),
                                                    start_x: f64::from(event.client_x()),
                                                    start_y: f64::from(event.client_y()),
                                                    origins,
                                                    moved: false,
                                                }));
                                            }
                                            on:mouseup=move |event| {
                                                if editor.pending_wire.get_untracked().is_some()
                                                    && dragging_node.get_untracked().is_none()
                                                {
                                                    event.stop_propagation();
                                                    ignore_click_for_node.set(Some(up_main_id.clone()));
                                                    connect_to_node(up_main_id.clone());
                                                }
                                            }
                                            on:click=move |event| {
                                                event.stop_propagation();
                                                if ignore_click_for_node
                                                    .get_untracked()
                                                    .as_deref()
                                                    == Some(click_main_id.as_str())
                                                {
                                                    ignore_click_for_node.set(None);
                                                    return;
                                                }
                                                if editor.pending_wire.get_untracked().is_some() {
                                                    connect_to_node(click_main_id.clone());
                                                } else if event.ctrl_key()
                                                    || event.meta_key()
                                                    || event.shift_key()
                                                {
                                                    editor.toggle_node_selection(&click_main_id);
                                                } else {
                                                    editor.select_node(Some(click_main_id.clone()), true);
                                                }
                                            }
                                        >
                                            {has_input.then(move || {
                                                let up_target_id = input_id.clone();
                                                let click_target_id = input_id.clone();
                                                let valid_target_id = valid_id.clone();
                                                view! {
                                                    <button
                                                        type="button"
                                                        class="canvas-port is-input"
                                                        class:is-valid-target=move || {
                                                            editor.pending_wire.with(|pw| {
                                                                pw.as_ref().is_some_and(|wire| {
                                                                    editor.graph.with(|g| {
                                                                        editor.catalog.with(|c| {
                                                                            can_connect(
                                                                                g,
                                                                                &wire.from_id,
                                                                                &wire.port,
                                                                                &valid_target_id,
                                                                                c,
                                                                            )
                                                                        })
                                                                    })
                                                                })
                                                            })
                                                        }
                                                        title="Entrée du bloc"
                                                        aria-label="Relier à l'entrée de ce bloc"
                                                        on:mousedown=move |event| event.stop_propagation()
                                                        on:mouseup=move |event| {
                                                            if editor.pending_wire.get_untracked().is_some() {
                                                                event.stop_propagation();
                                                                connect_to_node(up_target_id.clone());
                                                            }
                                                        }
                                                        on:click=move |event| {
                                                            event.stop_propagation();
                                                            connect_to_node(click_target_id.clone());
                                                        }
                                                    >
                                                        <span class="flow-graph__port-dot" aria-hidden="true"></span>
                                                    </button>
                                                }
                                            })}

                                            <div class="canvas-node__header">
                                                <span class="canvas-node__icon" aria-hidden="true">
                                                    <Icon name=domain_icon size=NODE_ICON_SIZE />
                                                </span>
                                                <div class="canvas-node__meta">
                                                    <span class="canvas-node__domain">{domain_label}</span>
                                                    {show_kind_label.then(|| view! {
                                                        <span class="canvas-node__kind">{kind_label}</span>
                                                    })}
                                                </div>
                                                <span class="canvas-node__step text-mono" title="Numéro d'étape">{format!("#{step_num}")}</span>
                                                {incomplete.then(|| view! {
                                                    <span class="canvas-node__warn" title="Réglage obligatoire manquant">"!"</span>
                                                })}
                                                {can_manage.then(move || view! {
                                                    <div class="canvas-node__actions">
                                                        <button
                                                            type="button"
                                                            class="header-icon-btn canvas-node__action-btn"
                                                            title="Dupliquer ce bloc"
                                                            aria-label="Dupliquer ce bloc"
                                                            on:mousedown=move |event| event.stop_propagation()
                                                            on:click=move |event| {
                                                                event.stop_propagation();
                                                                editor.duplicate_node(&dup_id);
                                                            }
                                                        >
                                                            <Icon name=IconName::Copy size=ACTION_ICON_SIZE />
                                                        </button>
                                                        <button
                                                            type="button"
                                                            class="header-icon-btn canvas-node__action-btn is-danger"
                                                            title="Supprimer ce bloc"
                                                            aria-label="Supprimer ce bloc"
                                                            on:mousedown=move |event| event.stop_propagation()
                                                            on:click=move |event| {
                                                                event.stop_propagation();
                                                                editor.remove_node(&remove_id);
                                                            }
                                                        >
                                                            <Icon name=IconName::Trash size=ACTION_ICON_SIZE />
                                                        </button>
                                                    </div>
                                                })}
                                            </div>

                                            <div class="canvas-node__body">
                                                <button
                                                    type="button"
                                                    class="canvas-node__main"
                                                >
                                                    <span class="canvas-node__title-row">
                                                        <span class="canvas-node__title">{node_name}</span>
                                                    </span>
                                                    {if !chips.is_empty() {
                                                        view! {
                                                            <div class="canvas-node__chips">
                                                                {chips.into_iter().map(|chip| view! {
                                                                    <span class="canvas-node__chip">
                                                                        {chip.key.map(|k| view! {
                                                                            <span class="canvas-node__chip-key">{format!("{k}:")}</span>
                                                                        })}
                                                                        <span class="canvas-node__chip-val">{chip.value}</span>
                                                                    </span>
                                                                }).collect_view()}
                                                            </div>
                                                        }.into_any()
                                                    } else {
                                                        view! {
                                                            <span class="canvas-node__subtitle">{subtitle}</span>
                                                        }.into_any()
                                                    }}
                                                </button>
                                            </div>

                                            {(!output_slots.is_empty()).then(move || view! {
                                                <div
                                                    class="canvas-node__outputs"
                                                    style=format!("--output-count: {port_count};")
                                                >
                                                    {output_slots.into_iter().map(|slot| {
                                                        let press_from = id.clone();
                                                        let press_port = slot.port.clone();
                                                        let click_from = id.clone();
                                                        let click_port = slot.port.clone();
                                                        let insert_from = id.clone();
                                                        let insert_port = slot.port.clone();
                                                        let existing_to = slot.existing_to;
                                                        let port_idx = slot.port_idx;
                                                        let pres_label = slot.pres.label;
                                                        let pres_symbol = slot.pres.symbol;
                                                        let pres_tone = slot.pres.tone;
                                                        let add_label = pres_label.clone();
                                                        let is_pill = port_count > 1 || slot.port.is_some();
                                                        let port_symbol = if is_pill { pres_symbol } else { "↓" };
                                                        let port_title = format!("Sortie : {} (cliquer ou glisser pour relier)", pres_label);
                                                        let port_aria = format!("Sortie {}", pres_label);
                                                        view! {
                                                            <div class="canvas-output-slot" data-tone=pres_tone.as_attr()>
                                                                <button
                                                                    type="button"
                                                                    class="canvas-port is-output"
                                                                    class:canvas-port--pill=is_pill
                                                                    title=port_title
                                                                    aria-label=port_aria
                                                                    on:mousedown=move |event| {
                                                                        event.stop_propagation();
                                                                        event.prevent_default();
                                                                        if !can_manage || event.button() != 0 {
                                                                            return;
                                                                        }
                                                                        let same = editor.pending_wire.get_untracked().is_some_and(|w| {
                                                                            w.from_id == press_from && w.port == press_port
                                                                        });
                                                                        if same {
                                                                            wire_opened_on_press.set(false);
                                                                            return;
                                                                        }
                                                                        let anchor = placed.with_untracked(|nodes| {
                                                                            nodes
                                                                                .iter()
                                                                                .find(|p| p.id == press_from)
                                                                                .map_or(Point { x: ORIGIN_X, y: ORIGIN_Y }, |p| {
                                                                                    p.out_port(port_idx, port_count)
                                                                                })
                                                                        });
                                                                        wire_opened_on_press.set(true);
                                                                        editor.pending_wire.set(Some(PendingWire {
                                                                            from_id: press_from.clone(),
                                                                            port: press_port.clone(),
                                                                            start_anchor: anchor,
                                                                            start_client_x: f64::from(event.client_x()),
                                                                            start_client_y: f64::from(event.client_y()),
                                                                            cursor: anchor,
                                                                        }));
                                                                    }
                                                                    on:click=move |event| {
                                                                        event.stop_propagation();
                                                                        if !can_manage {
                                                                            return;
                                                                        }
                                                                        if wire_opened_on_press.get_untracked() {
                                                                            wire_opened_on_press.set(false);
                                                                            return;
                                                                        }
                                                                        let same = editor.pending_wire.get_untracked().is_some_and(|w| {
                                                                            w.from_id == click_from && w.port == click_port
                                                                        });
                                                                        if same {
                                                                            editor.pending_wire.set(None);
                                                                        }
                                                                    }
                                                                >
                                                                    <span>{port_symbol}</span>
                                                                    {is_pill.then(|| view! {
                                                                        <span class="canvas-port__label">{pres_label}</span>
                                                                    })}
                                                                </button>
                                                                {can_manage.then(move || view! {
                                                                    <button
                                                                        type="button"
                                                                        class="canvas-port__add"
                                                                        title=format!("Insérer une étape sur « {} »", add_label)
                                                                        aria-label=format!("Insérer une étape sur {}", add_label)
                                                                        on:mousedown=move |event| event.stop_propagation()
                                                                        on:click=move |event| {
                                                                            event.stop_propagation();
                                                                            let anchor = placed
                                                                                .with_untracked(|nodes| {
                                                                                    nodes
                                                                                        .iter()
                                                                                        .find(|p| p.id == insert_from)
                                                                                        .map(|p| {
                                                                                            let pt = p.out_port(
                                                                                                port_idx,
                                                                                                port_count,
                                                                                            );
                                                                                            board_to_screen(
                                                                                                editor
                                                                                                    .viewport
                                                                                                    .get_untracked(),
                                                                                                Point {
                                                                                                    x: pt.x,
                                                                                                    y: pt.y,
                                                                                                },
                                                                                            )
                                                                                        })
                                                                                })
                                                                                .unwrap_or_else(|| {
                                                                                    board_ref
                                                                                        .get()
                                                                                        .map(|b| {
                                                                                            board_local_point(
                                                                                                &b,
                                                                                                event.client_x(),
                                                                                                event.client_y(),
                                                                                            )
                                                                                        })
                                                                                        .unwrap_or(Point {
                                                                                            x: 160.0,
                                                                                            y: 160.0,
                                                                                        })
                                                                                });
                                                                            editor.open_quick_add(QuickAddState {
                                                                                anchor,
                                                                                board_pos: None,
                                                                                branch: Some(BranchTarget {
                                                                                    source_id: insert_from
                                                                                        .clone(),
                                                                                    port: insert_port
                                                                                        .clone(),
                                                                                    replace_to: existing_to
                                                                                        .clone(),
                                                                                }),
                                                                            });
                                                                        }
                                                                    >
                                                                        "+"
                                                                    </button>
                                                                })}
                                                            </div>
                                                        }
                                                    }).collect_view()}
                                                </div>
                                            })}
                                        </article>
                                    </DragItem>
                                }
                            })
                            .collect_view()
                    };

                    view! {
                        <svg
                            class="flow-canvas__wires"
                            width="100%"
                            height="100%"
                            aria-label="Liaisons du graphe"
                        >
                            <g transform=move || viewport_svg_transform(editor.viewport.get())>
                                {wires_view}
                                {ghost_wire_view}
                            </g>
                        </svg>
                        <div
                            class="flow-graph-stage"
                            style=move || viewport_css_transform(editor.viewport.get())
                        >
                            {cards_view}
                            {move || marquee.get().map(|box_sel| {
                                let rect = Bounds::from_points(box_sel.start, box_sel.current);
                                view! {
                                    <div
                                        class="flow-canvas__marquee"
                                        style=format!(
                                            "left:{:.1}px;top:{:.1}px;width:{:.1}px;height:{:.1}px;",
                                            rect.min_x, rect.min_y, rect.width, rect.height
                                        )
                                    />
                                }
                            })}
                            {move || {
                                if editor.drop_target.get().is_some() || editor.drag.get().is_none() {
                                    return None;
                                }
                                drop_preview.get().map(|preview| view! {
                                    <div
                                        class="canvas-drop-ghost"
                                        style=format!(
                                            "left:{:.1}px;top:{:.1}px;width:{CARD_WIDTH}px;min-height:{CARD_HEIGHT}px;",
                                            preview.pos.x, preview.pos.y
                                        )
                                    >
                                        <span class="canvas-drop-ghost__badge">"+ Déposer sur la grille"</span>
                                        <span class="canvas-drop-ghost__title">{preview.label}</span>
                                    </div>
                                })
                            }}
                        </div>
                    }
                }}
            </div>
            <WireActionPopover editor=editor placed=placed can_manage=can_manage />
            <QuickAddPopover editor=editor board_ref=board_ref />
            <CanvasControlsBar
                editor=editor
                placed=placed
                board_ref=board_ref
                can_manage=can_manage
            />
            <MinimapPanel editor=editor placed=placed board_ref=board_ref />
        </section>
    }
}
