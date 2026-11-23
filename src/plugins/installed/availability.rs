//! Tâches que retient un élément à réparer (ADR 0029) : elles attendent en file, au lieu d'échouer.

use super::{
    EngineCatalog,
    usage::{parse_graph, unreadable_dependency},
};
use crate::{
    config::Config,
    error::AppResult,
    plugins::{CatalogCache, EngineSources, engine_sources},
    repository::{self, TaskFlow},
};
use forgearr_shared::flows::FlowGraph;
use sqlx::SqlitePool;

/// Élément à réparer dont ces graphes dépendent (`None` : tâche sans flux) ; aucun graphe : ce qui sert à
/// toute tâche.
pub async fn unreadable_item(
    engine: &EngineSources,
    cache: &CatalogCache,
    graphs: &[Option<FlowGraph>],
) -> Option<String> {
    if engine.damaged.is_empty() {
        return None;
    }
    let catalog = EngineCatalog::cached(cache, &engine.roots).await;
    unreadable_dependency(&engine.damaged, &catalog, graphs).map(str::to_owned)
}

/// Versions de flux que des éléments à réparer retiennent, parmi celles qui attendent.
pub async fn held_flows(
    pool: &SqlitePool,
    config: &Config,
    cache: &CatalogCache,
) -> AppResult<Vec<TaskFlow>> {
    let engine = engine_sources(pool, config).await?;
    if engine.damaged.is_empty() {
        return Ok(Vec::new());
    }
    let awaiting = repository::awaiting_flows(pool).await?;
    let catalog = EngineCatalog::cached(cache, &engine.roots).await;
    let is_held = |graph: Option<FlowGraph>| {
        unreadable_dependency(&engine.damaged, &catalog, &[graph]).is_some()
    };
    let without_flow: Option<TaskFlow> = is_held(None).then_some(None);
    let versions = awaiting
        .into_iter()
        .filter(|flow| is_held(Some(parse_graph(&flow.definition))))
        .map(|flow| Some((flow.flow_id, flow.version)));
    Ok(without_flow.into_iter().chain(versions).collect())
}
