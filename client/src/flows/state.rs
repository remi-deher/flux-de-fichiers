//! État partagé de l'éditeur : graphe édité, historique annuler/rétablir, coordonnées 2D et tiroirs.

use super::dnd::{
    BranchTarget, DragPayload, disconnect_edge, drop_outcome, duplicate_node as clone_node,
    duplicate_subgraph, insert_on_branch,
};
use super::geometry::{
    GRID_STEP, Point, ROW_STRIDE, Viewport, default_positions, place_with_positions, snap_point,
};
#[cfg(not(test))]
use crate::prefs;
use flux_shared::flows::{
    FlowConnection, FlowGraph, GraphError, OwnedBlockSpec, find_owned, native_owned_catalog,
};
use leptos::prelude::*;
use std::collections::{HashMap, HashSet};

const HISTORY_MAX_LEN: usize = 60;
const DUPLICATE_OFFSET: f64 = 36.0;
const LAYOUT_STORAGE_PREFIX: &str = "flux.flow-layout.";

/// Pile bornée d'instantanés du graphe pour Annuler (`Ctrl+Z`) / Rétablir (`Ctrl+Maj+Z`).
#[derive(Clone, Debug, PartialEq)]
pub struct HistoryStack {
    entries: Vec<FlowGraph>,
    index: usize,
    max_len: usize,
}

impl HistoryStack {
    pub fn new(initial: FlowGraph) -> Self {
        Self {
            entries: vec![initial],
            index: 0,
            max_len: HISTORY_MAX_LEN,
        }
    }

    pub fn push(&mut self, next: FlowGraph) {
        if self.entries.get(self.index) == Some(&next) {
            return;
        }
        self.entries.truncate(self.index + 1);
        self.entries.push(next);
        if self.entries.len() > self.max_len {
            let overflow = self.entries.len() - self.max_len;
            self.entries.drain(0..overflow);
        }
        self.index = self.entries.len().saturating_sub(1);
    }

    pub fn can_undo(&self) -> bool {
        self.index > 0
    }

    pub fn can_redo(&self) -> bool {
        self.index + 1 < self.entries.len()
    }

    pub fn undo(&mut self) -> Option<FlowGraph> {
        if !self.can_undo() {
            return None;
        }
        self.index -= 1;
        self.entries.get(self.index).cloned()
    }

    pub fn redo(&mut self) -> Option<FlowGraph> {
        if !self.can_redo() {
            return None;
        }
        self.index += 1;
        self.entries.get(self.index).cloned()
    }
}

fn layout_storage_key(flow_id: Option<&str>) -> Option<String> {
    let id = flow_id?.trim();
    (!id.is_empty()).then(|| format!("{LAYOUT_STORAGE_PREFIX}{id}"))
}

fn parse_layout(raw: &str) -> HashMap<String, Point> {
    serde_json::from_str::<HashMap<String, Point>>(raw).unwrap_or_default()
}

fn serialize_layout(positions: &HashMap<String, Point>) -> Option<String> {
    serde_json::to_string(positions).ok()
}

#[cfg(not(test))]
fn load_layout(flow_id: Option<&str>) -> HashMap<String, Point> {
    let Some(key) = layout_storage_key(flow_id) else {
        return HashMap::new();
    };
    prefs::load(&key)
        .as_deref()
        .map(parse_layout)
        .unwrap_or_default()
}

#[cfg(test)]
fn load_layout(flow_id: Option<&str>) -> HashMap<String, Point> {
    let _ = layout_storage_key(flow_id);
    HashMap::new()
}

#[cfg(not(test))]
fn save_layout(flow_id: Option<&str>, positions: &HashMap<String, Point>) {
    if let (Some(key), Some(raw)) = (layout_storage_key(flow_id), serialize_layout(positions)) {
        prefs::store(&key, &raw);
    }
}

#[cfg(test)]
fn save_layout(flow_id: Option<&str>, positions: &HashMap<String, Point>) {
    let _ = (layout_storage_key(flow_id), serialize_layout(positions));
}

/// Liaison interactive en cours de tracé depuis un port de sortie.
#[derive(Clone, Debug, PartialEq)]
pub struct PendingWire {
    pub from_id: String,
    pub port: Option<String>,
    pub start_anchor: Point,
    pub start_client_x: f64,
    pub start_client_y: f64,
    pub cursor: Point,
}

/// État du pop-over Quick-Add contextuel ouvert sur le canevas.
#[derive(Clone, Debug, PartialEq)]
pub struct QuickAddState {
    pub anchor: Point,
    pub board_pos: Option<Point>,
    pub branch: Option<BranchTarget>,
}

#[derive(Clone, Copy)]
pub struct Editor {
    pub flow_id: RwSignal<Option<String>>,
    pub graph: RwSignal<FlowGraph>,
    pub saved_json: RwSignal<String>,
    pub selected: RwSignal<Option<String>>,
    pub selected_ids: RwSignal<HashSet<String>>,
    pub dirty: RwSignal<bool>,
    /// Catalogue natif + blocs des dépôts (chargé via l'API).
    pub catalog: RwSignal<Vec<OwnedBlockSpec>>,
    /// Ce que l'on glisse en ce moment (palette ou carte).
    pub drag: RwSignal<Option<DragPayload>>,
    /// Carte survolée par un glisser.
    pub drop_target: RwSignal<Option<String>>,
    /// Raison du dernier dépôt refusé.
    pub drop_notice: RwSignal<Option<&'static str>>,
    /// Panneaux et interactions d'atelier.
    pub inspector_open: RwSignal<bool>,
    pub palette_open: RwSignal<bool>,
    pub palette_pinned: RwSignal<bool>,
    pub palette_focus_tick: RwSignal<u64>,
    pub diagnostics_open: RwSignal<bool>,
    pub settings_open: RwSignal<bool>,
    pub branch_target: RwSignal<Option<BranchTarget>>,
    pub quick_add: RwSignal<Option<QuickAddState>>,
    pub selected_wire: RwSignal<Option<FlowConnection>>,
    pub pending_wire: RwSignal<Option<PendingWire>>,
    pub positions: RwSignal<HashMap<String, Point>>,
    pub previous_positions: RwSignal<Option<HashMap<String, Point>>>,
    pub viewport: RwSignal<Viewport>,
    pub snap_to_grid: RwSignal<bool>,
    pub history: RwSignal<HistoryStack>,
}

fn maybe_snap(pt: Point, snap: bool) -> Point {
    if snap { snap_point(pt, GRID_STEP) } else { pt }
}

impl Editor {
    pub fn for_flow(flow_id: Option<String>, graph: FlowGraph) -> Self {
        let initial_positions = load_layout(flow_id.as_deref());
        let palette_initially_open = graph.nodes.len() <= 1;
        let saved_json = serde_json::to_string(&graph).unwrap_or_default();
        let history = HistoryStack::new(graph.clone());
        Self {
            flow_id: RwSignal::new(flow_id),
            graph: RwSignal::new(graph),
            saved_json: RwSignal::new(saved_json),
            selected: RwSignal::new(None),
            selected_ids: RwSignal::new(HashSet::new()),
            dirty: RwSignal::new(false),
            catalog: RwSignal::new(native_owned_catalog()),
            drag: RwSignal::new(None),
            drop_target: RwSignal::new(None),
            drop_notice: RwSignal::new(None),
            inspector_open: RwSignal::new(false),
            palette_open: RwSignal::new(palette_initially_open),
            palette_pinned: RwSignal::new(false),
            palette_focus_tick: RwSignal::new(0),
            diagnostics_open: RwSignal::new(false),
            settings_open: RwSignal::new(false),
            branch_target: RwSignal::new(None),
            quick_add: RwSignal::new(None),
            selected_wire: RwSignal::new(None),
            pending_wire: RwSignal::new(None),
            positions: RwSignal::new(initial_positions),
            previous_positions: RwSignal::new(None),
            viewport: RwSignal::new(Viewport::default()),
            snap_to_grid: RwSignal::new(true),
            history: RwSignal::new(history),
        }
    }

    /// Modifie le graphe, enregistre l'instantané pour Undo/Redo et marque modifié.
    pub fn mutate_graph(&self, change: impl FnOnce(&mut FlowGraph)) {
        let mut pushed = None;
        self.graph.update(|graph| {
            let before = graph.clone();
            change(graph);
            if *graph != before {
                pushed = Some(graph.clone());
            }
        });
        if let Some(next) = pushed {
            self.history.update(|history| history.push(next));
            self.dirty.set(true);
            self.selected_wire.set(None);
        }
    }

    /// Alias compatible avec les contrôles existants.
    pub fn edit(&self, change: impl FnOnce(&mut FlowGraph)) {
        self.mutate_graph(change);
    }

    pub fn can_undo(&self) -> bool {
        self.history.with(HistoryStack::can_undo)
    }

    pub fn can_redo(&self) -> bool {
        self.history.with(HistoryStack::can_redo)
    }

    pub fn undo(&self) {
        let prev = self.history.try_update(HistoryStack::undo).flatten();
        if let Some(graph) = prev {
            self.apply_history_snapshot(graph);
        }
    }

    pub fn redo(&self) {
        let next = self.history.try_update(HistoryStack::redo).flatten();
        if let Some(graph) = next {
            self.apply_history_snapshot(graph);
        }
    }

    fn apply_history_snapshot(&self, snapshot: FlowGraph) {
        let mut next_ids: HashSet<String> = self
            .selected_ids
            .get_untracked()
            .into_iter()
            .filter(|id| snapshot.node(id).is_some())
            .collect();
        if next_ids.is_empty()
            && let Some(sel) = self.selected.get_untracked()
            && snapshot.node(&sel).is_some()
        {
            next_ids.insert(sel);
        }
        if next_ids.len() == 1 {
            self.selected.set(next_ids.iter().next().cloned());
        } else {
            self.selected.set(None);
            self.inspector_open.set(false);
        }
        self.selected_ids.set(next_ids);
        self.pending_wire.set(None);
        self.selected_wire.set(None);
        self.branch_target.set(None);
        self.quick_add.set(None);
        self.graph.set(snapshot);
        self.dirty.set(true);
    }

    /// Ouvre l'onglet Composants dans le tiroir droit unifié.
    pub fn open_palette(&self) {
        self.inspector_open.set(false);
        self.settings_open.set(false);
        self.palette_open.set(true);
    }

    /// Ouvre l'onglet Composants et demande le focus sur la recherche.
    pub fn focus_palette_search(&self) {
        self.open_palette();
        self.palette_focus_tick.update(|n| *n = n.wrapping_add(1));
    }

    /// Ouvre l'inspecteur du bloc sélectionné dans le tiroir droit unifié.
    pub fn open_inspector(&self) {
        if self.selected.get_untracked().is_none() {
            return;
        }
        self.palette_open.set(false);
        self.settings_open.set(false);
        self.inspector_open.set(true);
    }

    /// Ouvre les réglages du flux dans le tiroir droit unifié.
    pub fn open_settings(&self) {
        self.palette_open.set(false);
        self.inspector_open.set(false);
        self.settings_open.set(true);
    }

    /// Ferme le tiroir droit unifié quel que soit l'onglet actif.
    pub fn close_right_panel(&self) {
        self.palette_open.set(false);
        self.inspector_open.set(false);
        self.settings_open.set(false);
    }

    /// Indique de façon réactive si un onglet du tiroir droit est ouvert.
    pub fn any_right_panel_open(&self) -> bool {
        self.palette_open.get()
            || (self.inspector_open.get() && self.selected.get().is_some())
            || self.settings_open.get()
    }

    /// Indique de façon réactive si un bloc donné est sélectionné (simple ou multiple).
    pub fn is_selected(&self, id: &str) -> bool {
        self.selected.get().as_deref() == Some(id) || self.selected_ids.with(|ids| ids.contains(id))
    }

    fn active_selection_ids(&self) -> HashSet<String> {
        let mut ids = self.selected_ids.get_untracked();
        if ids.is_empty()
            && let Some(id) = self.selected.get_untracked()
        {
            ids.insert(id);
        }
        ids
    }

    /// Aimante un point sur la grille si l'option `snap_to_grid` est active.
    pub fn snap_if_enabled(&self, pt: Point) -> Point {
        maybe_snap(pt, self.snap_to_grid.get_untracked())
    }

    pub fn select_node(&self, id: Option<String>, open_inspector: bool) {
        self.selected_wire.set(None);
        self.selected.set(id.clone());
        let mut ids = HashSet::new();
        if let Some(ref single) = id {
            ids.insert(single.clone());
        }
        self.selected_ids.set(ids);
        if id.is_some() && open_inspector {
            self.open_inspector();
        } else if id.is_none() {
            self.inspector_open.set(false);
        }
    }

    /// Remplace la sélection active par un ensemble arbitraire d'identifiants.
    pub fn set_selection(&self, ids: HashSet<String>) {
        self.selected_wire.set(None);
        if ids.len() == 1 {
            self.selected.set(ids.iter().next().cloned());
            self.selected_ids.set(ids);
            return;
        }
        self.selected.set(None);
        self.inspector_open.set(false);
        self.selected_ids.set(ids);
    }

    /// Ajoute ou retire un bloc de la sélection multiple (`Ctrl+Clic` / `Maj+Clic`).
    pub fn toggle_node_selection(&self, id: &str) {
        let mut ids = self.active_selection_ids();
        if !ids.remove(id) {
            ids.insert(id.to_owned());
        }
        if ids.len() == 1 {
            let only = ids.iter().next().cloned();
            let keep_open = self.inspector_open.get_untracked();
            self.select_node(only, keep_open);
        } else {
            self.set_selection(ids);
        }
    }

    /// Sélectionne tous les blocs présents dans le graphe (`Ctrl+A`).
    pub fn select_all_nodes(&self) {
        let ids: HashSet<String> = self
            .graph
            .with_untracked(|g| g.nodes.iter().map(|n| n.id.clone()).collect());
        self.set_selection(ids);
    }

    /// Efface toute sélection de bloc(s).
    pub fn clear_selection(&self) {
        self.select_node(None, false);
    }

    pub fn mark_saved(&self, new_flow_id: Option<String>) {
        if let Some(id) = new_flow_id {
            self.flow_id.set(Some(id));
        }
        let raw = self
            .graph
            .with_untracked(|g| serde_json::to_string(g).unwrap_or_default());
        self.saved_json.set(raw);
        self.dirty.set(false);
        self.persist_layout();
    }

    fn persist_layout(&self) {
        let flow_id = self.flow_id.get_untracked();
        self.positions
            .with_untracked(|map| save_layout(flow_id.as_deref(), map));
    }

    pub fn node_point(&self, id: &str) -> Point {
        self.positions
            .with_untracked(|custom| custom.get(id).copied())
            .or_else(|| {
                self.graph
                    .with_untracked(|graph| default_positions(graph).get(id).copied())
            })
            .unwrap_or(Point { x: 72.0, y: 64.0 })
    }

    /// Ouvre le pop-over Quick-Add contextuel sur le canevas.
    pub fn open_quick_add(&self, state: QuickAddState) {
        self.pending_wire.set(None);
        self.selected_wire.set(None);
        self.branch_target.set(None);
        self.quick_add.set(Some(state));
    }

    /// Crée et place le bloc choisi depuis Quick-Add, en raccordant la branche éventuelle.
    pub fn complete_quick_add(&self, spec: &OwnedBlockSpec) -> Result<String, GraphError> {
        let Some(state) = self.quick_add.get_untracked() else {
            return Err(GraphError::UnknownNode);
        };
        let defaults = spec.default_config();
        let new_id = if let Some(branch) = state.branch {
            let src_point = self.node_point(&branch.source_id);
            let mut outcome = Err(GraphError::UnknownNode);
            self.mutate_graph(|graph| {
                outcome = insert_on_branch(graph, &branch, spec, defaults);
            });
            let created = outcome?;
            let target_pos = state.board_pos.unwrap_or(Point {
                x: src_point.x,
                y: src_point.y + ROW_STRIDE,
            });
            self.set_node_position(&created, target_pos);
            created
        } else {
            let mut created = String::new();
            let kind = spec.kind_enum();
            let label = spec.label.clone();
            self.mutate_graph(|graph| {
                created = graph.append(kind, &label, defaults);
            });
            let target_pos = state.board_pos.or_else(|| {
                self.graph
                    .with_untracked(|graph| default_positions(graph).get(&created).copied())
            });
            if let Some(pt) = target_pos {
                self.set_node_position(&created, pt);
            }
            created
        };
        self.quick_add.set(None);
        self.select_node(Some(new_id.clone()), true);
        Ok(new_id)
    }

    pub fn set_node_position_live(&self, id: &str, pt: Point) {
        self.move_nodes_live(&[(id.to_owned(), pt)], false);
    }

    /// Met à jour les coordonnées d'un ou plusieurs blocs pendant un glissement, avec snap optionnel.
    pub fn move_nodes_live(&self, updates: &[(String, Point)], snap: bool) {
        self.positions.update(|map| {
            for (id, pt) in updates {
                if pt.x.is_finite() && pt.y.is_finite() {
                    map.insert(id.clone(), maybe_snap(*pt, snap));
                }
            }
        });
    }

    pub fn commit_positions(&self) {
        self.persist_layout();
    }

    pub fn set_node_position(&self, id: &str, pt: Point) {
        self.set_node_position_live(id, pt);
        self.commit_positions();
    }

    /// Recalcule une disposition propre en sauvegardant les anciennes coordonnées pour annulation.
    pub fn auto_layout(&self) {
        let current_snapshot: HashMap<String, Point> = self.graph.with_untracked(|graph| {
            self.positions.with_untracked(|custom| {
                place_with_positions(graph, custom)
                    .into_iter()
                    .map(|placed| {
                        (
                            placed.id,
                            Point {
                                x: placed.x,
                                y: placed.y,
                            },
                        )
                    })
                    .collect()
            })
        });
        self.previous_positions.set(Some(current_snapshot));
        let fresh = self.graph.with_untracked(default_positions);
        self.positions.set(fresh);
        self.persist_layout();
    }

    pub fn undo_layout(&self) {
        if let Some(prev) = self.previous_positions.get_untracked() {
            self.positions.set(prev);
            self.previous_positions.set(None);
            self.persist_layout();
        }
    }

    /// Ajoute un bloc depuis la palette, ou l'insère sur la branche ciblée si active.
    pub fn add_or_insert_block(&self, spec: &OwnedBlockSpec) -> Result<String, GraphError> {
        let defaults = spec.default_config();
        let target = self.branch_target.get_untracked();
        let new_id = if let Some(branch) = target {
            let src_point = self.node_point(&branch.source_id);
            let mut outcome = Err(GraphError::UnknownNode);
            self.mutate_graph(|graph| {
                outcome = insert_on_branch(graph, &branch, spec, defaults);
            });
            let created = outcome?;
            self.set_node_position(
                &created,
                Point {
                    x: src_point.x,
                    y: src_point.y + ROW_STRIDE,
                },
            );
            self.branch_target.set(None);
            created
        } else {
            let mut created = String::new();
            let kind = spec.kind_enum();
            let label = spec.label.clone();
            self.mutate_graph(|graph| {
                created = graph.append(kind, &label, defaults);
            });
            let pos = self
                .graph
                .with_untracked(|graph| default_positions(graph).get(&created).copied());
            if let Some(pt) = pos {
                self.set_node_position(&created, pt);
            }
            created
        };

        let stay_on_palette = self.palette_pinned.get_untracked();
        self.select_node(Some(new_id.clone()), !stay_on_palette);
        if stay_on_palette {
            self.open_palette();
        }
        Ok(new_id)
    }

    /// Duplique un bloc avec un léger décalage visuel et ouvre son inspecteur.
    pub fn duplicate_node(&self, id: &str) -> Option<String> {
        let orig = self.node_point(id);
        let mut created = None;
        self.mutate_graph(|graph| {
            created = clone_node(graph, id);
        });
        if let Some(ref new_id) = created {
            self.set_node_position(
                new_id,
                Point {
                    x: orig.x + DUPLICATE_OFFSET,
                    y: orig.y + DUPLICATE_OFFSET,
                },
            );
            self.select_node(Some(new_id.clone()), true);
        }
        created
    }

    /// Duplique la sélection courante (un bloc ou un sous-graphe en conservant ses liaisons internes).
    pub fn duplicate_selected(&self) -> Vec<String> {
        let targets = self.active_selection_ids();
        if targets.is_empty() {
            return Vec::new();
        }
        if targets.len() == 1
            && let Some(only) = targets.iter().next()
        {
            return self.duplicate_node(only).into_iter().collect();
        }
        let orig_points: HashMap<String, Point> = targets
            .iter()
            .map(|id| (id.clone(), self.node_point(id)))
            .collect();
        let mut id_map = HashMap::new();
        self.mutate_graph(|graph| {
            id_map = duplicate_subgraph(graph, &targets);
        });
        if id_map.is_empty() {
            return Vec::new();
        }
        let snap = self.snap_to_grid.get_untracked();
        let moves: Vec<(String, Point)> = id_map
            .iter()
            .filter_map(|(old_id, new_id)| {
                let orig = orig_points.get(old_id)?;
                Some((
                    new_id.clone(),
                    Point {
                        x: orig.x + DUPLICATE_OFFSET,
                        y: orig.y + DUPLICATE_OFFSET,
                    },
                ))
            })
            .collect();
        self.move_nodes_live(&moves, snap);
        self.commit_positions();
        let new_ids: HashSet<String> = id_map.values().cloned().collect();
        let created: Vec<String> = new_ids.iter().cloned().collect();
        self.set_selection(new_ids);
        created
    }

    fn remove_node_ids(&self, targets: &HashSet<String>) -> usize {
        if targets.is_empty() {
            return 0;
        }
        let count = targets.len();
        self.mutate_graph(|graph| {
            for id in targets {
                graph.remove(id);
            }
        });
        self.positions.update(|map| {
            for id in targets {
                map.remove(id);
            }
        });
        self.persist_layout();
        self.selected_ids.update(|ids| {
            for id in targets {
                ids.remove(id);
            }
        });
        let remaining = self.selected_ids.get_untracked();
        if remaining.len() == 1 {
            self.selected.set(remaining.iter().next().cloned());
        } else if self
            .selected
            .get_untracked()
            .is_some_and(|id| targets.contains(&id))
            || remaining.len() > 1
        {
            self.selected.set(None);
            self.inspector_open.set(false);
        }
        if self.branch_target.get_untracked().is_some_and(|b| {
            targets.contains(&b.source_id)
                || b.replace_to
                    .as_deref()
                    .is_some_and(|to| targets.contains(to))
        }) {
            self.branch_target.set(None);
        }
        if self
            .pending_wire
            .get_untracked()
            .is_some_and(|wire| targets.contains(&wire.from_id))
        {
            self.pending_wire.set(None);
        }
        count
    }

    /// Retire tous les blocs actuellement sélectionnés en une seule étape d'historique.
    pub fn remove_selected_nodes(&self) -> usize {
        let targets = self.active_selection_ids();
        self.remove_node_ids(&targets)
    }

    /// Retire la liaison actuellement sélectionnée, si présente.
    pub fn disconnect_selected_wire(&self) -> bool {
        let Some(conn) = self.selected_wire.get_untracked() else {
            return false;
        };
        self.mutate_graph(|graph| {
            disconnect_edge(graph, &conn.from, &conn.port, &conn.to);
        });
        self.selected_wire.set(None);
        true
    }

    /// Retire un bloc du graphe et nettoie les sélections ou cibles associées.
    pub fn remove_node(&self, id: &str) {
        let targets: HashSet<String> = [id.to_owned()].into_iter().collect();
        let _ = self.remove_node_ids(&targets);
    }

    /// Fin d'un glisser, abouti ou non.
    pub fn end_drag(&self) {
        self.drag.set(None);
        self.drop_target.set(None);
    }

    /// Dépose ce qui est glissé sur la carte `target` ; renvoie le message d'un refus.
    pub fn drop_on(&self, target: &str) -> Result<(), &'static str> {
        let Some(payload) = self.drag.get_untracked() else {
            return Ok(());
        };
        self.end_drag();
        let dropped = self
            .graph
            .with_untracked(|graph| drop_outcome(graph, &payload, target))?;
        let target_pt = self.node_point(target);
        self.mutate_graph(|graph| *graph = dropped.graph);
        if matches!(payload, DragPayload::Palette { .. }) {
            self.set_node_position(
                &dropped.node_id,
                Point {
                    x: target_pt.x,
                    y: target_pt.y + ROW_STRIDE,
                },
            );
        }
        self.select_node(Some(dropped.node_id), true);
        Ok(())
    }
}

/// Blocs dont un réglage obligatoire est vide : `(nom du bloc, champs manquants)`.
pub fn incomplete_blocks(
    graph: &FlowGraph,
    catalog: &[OwnedBlockSpec],
) -> Vec<(String, Vec<String>)> {
    graph
        .nodes
        .iter()
        .filter_map(|node| {
            let missing = find_owned(catalog, &node.kind)?.missing_fields(&node.config);
            (!missing.is_empty()).then(|| (node.name.clone(), missing))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use flux_shared::flows::FlowBlockKind;
    use serde_json::json;

    #[test]
    fn only_blocks_with_empty_required_fields_are_reported() {
        let catalog = native_owned_catalog();
        let mut graph = FlowGraph::default();
        graph.append(FlowBlockKind::FileInput, "Entrée", json!({}));
        graph.append(FlowBlockKind::Webhook, "Hook", json!({"url": ""}));
        graph.append(FlowBlockKind::parse("macro.unknown"), "Inconnu", json!({}));
        assert_eq!(
            incomplete_blocks(&graph, &catalog),
            vec![("Hook".to_owned(), vec!["URL".to_owned()])]
        );
    }

    #[test]
    fn history_stack_supports_undo_redo_and_truncates_future_on_edit() {
        let mut g1 = FlowGraph::default();
        g1.append(FlowBlockKind::FileInput, "Entrée", json!({}));
        let mut history = HistoryStack::new(g1.clone());
        assert!(!history.can_undo());
        assert!(!history.can_redo());

        // Pousser un état identique est ignoré.
        history.push(g1.clone());
        assert!(!history.can_undo());

        let mut g2 = g1.clone();
        g2.append(FlowBlockKind::RemuxMkv, "Remux", json!({}));
        history.push(g2.clone());
        assert!(history.can_undo());

        let mut g3 = g2.clone();
        g3.append(FlowBlockKind::FileOutput, "Fin", json!({}));
        history.push(g3.clone());

        assert_eq!(history.undo(), Some(g2.clone()));
        assert!(history.can_redo());
        assert_eq!(history.undo(), Some(g1));
        assert!(!history.can_undo());
        assert_eq!(history.redo(), Some(g2.clone()));

        let mut alt = g2;
        alt.append(FlowBlockKind::Webhook, "Hook", json!({}));
        history.push(alt.clone());
        assert!(!history.can_redo());
        assert_eq!(history.entries.last(), Some(&alt));
    }

    #[test]
    fn editor_mutations_undo_duplicate_and_auto_layout_behave_consistently() {
        let mut graph = FlowGraph::default();
        let input = graph.append(FlowBlockKind::FileInput, "Entrée", json!({}));
        let editor = Editor::for_flow(Some("flow.test".into()), graph);

        let Some(webhook) = native_owned_catalog()
            .into_iter()
            .find(|s| s.kind == "webhook")
        else {
            panic!("webhook absent du catalogue natif");
        };
        let Ok(added_id) = editor.add_or_insert_block(&webhook) else {
            panic!("ajout du webhook refusé");
        };
        assert_eq!(editor.graph.get_untracked().nodes.len(), 2);
        assert!(editor.can_undo());
        assert!(editor.inspector_open.get_untracked());

        let copy = editor.duplicate_node(&added_id);
        assert!(copy.is_some());
        assert_eq!(editor.graph.get_untracked().nodes.len(), 3);

        editor.set_node_position(&input, Point { x: 500.0, y: 500.0 });
        editor.auto_layout();
        assert!(editor.previous_positions.get_untracked().is_some());
        editor.undo_layout();
        assert_eq!(editor.node_point(&input), Point { x: 500.0, y: 500.0 });

        editor.undo();
        assert_eq!(editor.graph.get_untracked().nodes.len(), 2);
        assert!(editor.can_redo());
        editor.redo();
        assert_eq!(editor.graph.get_untracked().nodes.len(), 3);

        if let Some(copy_id) = copy {
            editor.remove_node(&copy_id);
        }
        assert_eq!(editor.graph.get_untracked().nodes.len(), 2);

        assert_eq!(layout_storage_key(None), None);
        assert_eq!(layout_storage_key(Some("  ")), None);
        assert_eq!(
            layout_storage_key(Some("flow.1")).as_deref(),
            Some("flux.flow-layout.flow.1")
        );
        let mut stored = HashMap::new();
        stored.insert("n1".to_owned(), Point { x: 120.0, y: 80.0 });
        let Some(raw) = serialize_layout(&stored) else {
            panic!("sérialisation des positions impossible");
        };
        assert_eq!(parse_layout(&raw), stored);
        assert!(parse_layout("{invalide").is_empty());
    }

    #[test]
    fn right_dock_tabs_are_mutually_exclusive() {
        let mut graph = FlowGraph::default();
        let input = graph.append(FlowBlockKind::FileInput, "Entrée", json!({}));
        let editor = Editor::for_flow(Some("flow.dock".into()), graph);

        assert!(editor.palette_open.get_untracked());
        assert!(!editor.inspector_open.get_untracked());
        assert!(!editor.settings_open.get_untracked());

        editor.open_settings();
        assert!(editor.settings_open.get_untracked());
        assert!(!editor.palette_open.get_untracked());

        editor.select_node(Some(input), true);
        assert!(editor.inspector_open.get_untracked());
        assert!(!editor.settings_open.get_untracked());
        assert!(!editor.palette_open.get_untracked());

        editor.focus_palette_search();
        assert!(editor.palette_open.get_untracked());
        assert!(!editor.inspector_open.get_untracked());
        assert_eq!(editor.palette_focus_tick.get_untracked(), 1);

        editor.close_right_panel();
        assert!(!editor.palette_open.get_untracked());
        assert!(!editor.inspector_open.get_untracked());
        assert!(!editor.settings_open.get_untracked());
    }

    #[test]
    fn quick_add_places_and_connects_in_unbounded_coordinates() {
        let mut graph = FlowGraph::default();
        let input = graph.append(FlowBlockKind::FileInput, "Entrée", json!({}));
        let editor = Editor::for_flow(Some("flow.qa".into()), graph);
        let Some(encode) = native_owned_catalog()
            .into_iter()
            .find(|s| s.kind == "block.video.encode")
        else {
            panic!("encode absent du catalogue natif");
        };

        editor.open_quick_add(QuickAddState {
            anchor: Point { x: 200.0, y: 180.0 },
            board_pos: Some(Point {
                x: -180.0,
                y: -120.0,
            }),
            branch: Some(BranchTarget {
                source_id: input.clone(),
                port: None,
                replace_to: None,
            }),
        });
        let Ok(new_id) = editor.complete_quick_add(&encode) else {
            panic!("quick_add aurait dû créer le bloc");
        };
        assert!(editor.quick_add.get_untracked().is_none());
        assert_eq!(
            editor.node_point(&new_id),
            Point {
                x: -180.0,
                y: -120.0
            }
        );
        assert!(
            editor
                .graph
                .with_untracked(|g| g.outgoing(&input).any(|e| e.to == new_id))
        );
    }

    #[test]
    fn multi_selection_duplicates_subgraph_and_removes_batch_in_one_undo_step() {
        let mut graph = FlowGraph::default();
        let input = graph.append(FlowBlockKind::FileInput, "Entrée", json!({}));
        let remux = graph.append(FlowBlockKind::RemuxMkv, "Remux", json!({}));
        let output = graph.append(FlowBlockKind::FileOutput, "Sortie", json!({}));
        let editor = Editor::for_flow(Some("flow.batch".into()), graph);

        editor.select_node(Some(remux.clone()), false);
        editor.toggle_node_selection(&output);
        assert_eq!(editor.selected_ids.get_untracked().len(), 2);
        assert_eq!(editor.selected.get_untracked(), None);

        let copies = editor.duplicate_selected();
        assert_eq!(copies.len(), 2);
        assert_eq!(editor.graph.get_untracked().nodes.len(), 5);
        assert_eq!(editor.selected_ids.get_untracked().len(), 2);

        let removed = editor.remove_selected_nodes();
        assert_eq!(removed, 2);
        assert_eq!(editor.graph.get_untracked().nodes.len(), 3);
        assert!(editor.selected_ids.get_untracked().is_empty());

        editor.undo();
        assert_eq!(editor.graph.get_untracked().nodes.len(), 5);

        editor.select_all_nodes();
        assert_eq!(editor.selected_ids.get_untracked().len(), 5);
        editor.clear_selection();
        assert!(editor.selected_ids.get_untracked().is_empty());
        assert!(!editor.is_selected(&input));
    }
}
