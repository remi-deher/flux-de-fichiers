//! Le moteur tel qu'il serait une fois le plan appliqué.

use super::{
    context::PlanContext,
    walk::{Change, Selected},
};
use crate::plugins::{CatalogItemType, installed::EngineCatalog};
use std::collections::HashSet;

/// Changements que le moteur lirait : d'un flux, il ne lit que les copies de l'utilisateur.
pub(super) fn engine_changes(selected: &[Selected]) -> impl Iterator<Item = &Change> {
    selected
        .iter()
        .filter(|item| item.kind != CatalogItemType::Flow)
        .filter_map(|item| item.change.as_ref())
}

pub(super) fn engine_after(context: &PlanContext, selected: &[Selected]) -> EngineCatalog {
    let engine = &context.engine;
    let replaced: HashSet<&str> = engine_changes(selected)
        .map(|change| change.content.entry.id.as_str())
        .collect();
    let is_replaced = |id: &str| replaced.contains(id);
    let is_owned_by_replaced = |owner: Option<&String>| owner.is_some_and(|id| is_replaced(id));

    let kept = engine.entries.iter().filter(|(_, entry)| {
        !is_replaced(&entry.id) && !is_owned_by_replaced(entry.owner.as_ref())
    });
    let added = engine_changes(selected).map(|change| {
        let root = context.sources[change.source].root.clone();
        (root, change.content.entry.clone())
    });
    let entries = kept.cloned().chain(added).collect();

    let kept = engine
        .blocks
        .iter()
        .filter(|block| !is_owned_by_replaced(block.owner.as_ref()));
    let added = engine_changes(selected).flat_map(|change| &change.declared.blocks);
    let blocks: Vec<_> = kept.chain(added).cloned().collect();

    let added = engine_changes(selected).flat_map(|change| {
        let owner = &change.content.entry;
        change
            .declared
            .operations
            .iter()
            .map(move |operation| (owner, &operation.raw))
    });
    let operations = engine.operations.replacing(&replaced, added, &blocks);
    EngineCatalog {
        entries,
        blocks,
        operations,
    }
}
