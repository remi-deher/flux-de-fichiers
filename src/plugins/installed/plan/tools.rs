//! Outils que le plan recommande de télécharger.

use super::{context::PlanContext, walk::Selected};
use crate::plugins::{CatalogItemType, is_tool_installable, summarize_tool};
use forgearr_shared::plugins::{CatalogEntry, ToolStatus, store::plan::PlanTool};
use std::path::PathBuf;

pub(super) fn recommended_tools(context: &PlanContext, selected: &[Selected]) -> Vec<PlanTool> {
    selected
        .iter()
        .filter(|item| item.kind == CatalogItemType::App)
        .filter_map(|item| recommended(context, item))
        .collect()
}

/// `None` : le moteur ne lit plus cet outil, et rien ne le répare.
fn recommended(context: &PlanContext, item: &Selected) -> Option<PlanTool> {
    let (root, entry) = located(context, item)?;
    let installable = is_tool_installable(&[root], &item.id);
    let entry = CatalogEntry {
        version: item.effective_version.clone(),
        ..entry
    };
    let summary = summarize_tool(entry, String::new(), installable, &context.tools_cache);
    Some(PlanTool {
        origin: item.origin.clone(),
        id: summary.id,
        name: summary.name,
        version: summary.version,
        status: summary.status,
        installable,
        preselected: installable && summary.status == ToolStatus::NotInstalled,
        required_by: item.required_by.clone(),
    })
}

fn located(context: &PlanContext, item: &Selected) -> Option<(PathBuf, CatalogEntry)> {
    match &item.change {
        Some(change) => Some((
            context.sources[change.source].root.clone(),
            change.content.entry.clone(),
        )),
        None => context.engine.located(&item.id).cloned(),
    }
}
