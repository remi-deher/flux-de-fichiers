//! Flux liés : validation des sous-flux et du flux d'échec, flux d'échec global (ADR 0022).

use super::{AppState, ManageFlowsAuth, ViewFlowsAuth};
use crate::{
    error::{AppError, AppResult},
    repository,
};
use axum::{Json, extract::State};
use forgearr_shared::flows::{DefaultFailureFlow, FlowGraph, check_sub_flow_links};
use sqlx::SqlitePool;
use std::collections::HashMap;

/// Identifiant provisoire d'un flux validé avant son premier enregistrement.
pub(super) const UNSAVED_FLOW_ID: &str = "(nouveau flux)";

/// Erreurs de liens d'un graphe : sous-flux absents, circulaires ou trop profonds, flux d'échec
/// inconnu. `known` donne le graphe courant des autres flux ; celui de `flow_id` est `graph`.
pub(super) fn link_errors(
    flow_id: &str,
    graph: &FlowGraph,
    known: &HashMap<String, FlowGraph>,
) -> Vec<String> {
    let mut errors = Vec::new();
    let load = |id: &str| {
        if id == flow_id {
            Some(graph.clone())
        } else {
            known.get(id).cloned()
        }
    };
    if let Err(err) = check_sub_flow_links(flow_id, graph, &load) {
        errors.push(err.to_string());
    }
    if let Some(failure) = graph.failure_flow_id()
        && failure != flow_id
        && !known.contains_key(failure)
    {
        errors.push(format!("flux d'échec introuvable : {failure}"));
    }
    errors
}

async fn known_graphs(pool: &SqlitePool) -> AppResult<HashMap<String, FlowGraph>> {
    Ok(repository::list_flows(pool)
        .await?
        .into_iter()
        .map(|flow| (flow.id, FlowGraph::parse(&flow.definition)))
        .collect())
}

/// Erreurs de liens de la définition `definition` du flux `flow_id`, contre les flux enregistrés.
pub(super) async fn definition_link_errors(
    pool: &SqlitePool,
    flow_id: &str,
    definition: &str,
) -> AppResult<Vec<String>> {
    let graph = FlowGraph::parse(definition);
    Ok(link_errors(flow_id, &graph, &known_graphs(pool).await?))
}

/// Refuse l'enregistrement d'une définition aux liens invalides.
pub(super) async fn ensure_valid_links(
    pool: &SqlitePool,
    flow_id: &str,
    definition: &str,
) -> AppResult<()> {
    let errors = definition_link_errors(pool, flow_id, definition).await?;
    if errors.is_empty() {
        return Ok(());
    }
    tracing::warn!(flow_id = %flow_id, errors = ?errors, "flux lié refusé à l'enregistrement du flux");
    Err(AppError::Invalid)
}

/// Flux d'échec global appliqué aux flux qui n'en désignent pas.
pub async fn get_default_failure_flow(
    State(s): State<AppState>,
    _auth: ViewFlowsAuth,
) -> AppResult<Json<DefaultFailureFlow>> {
    Ok(Json(DefaultFailureFlow {
        flow_id: repository::default_failure_flow(&s.pool).await?,
    }))
}

/// Définit ou efface le flux d'échec global ; le flux désigné doit exister.
pub async fn put_default_failure_flow(
    State(s): State<AppState>,
    _auth: ManageFlowsAuth,
    Json(payload): Json<DefaultFailureFlow>,
) -> AppResult<Json<DefaultFailureFlow>> {
    let flow_id = payload
        .flow_id
        .as_deref()
        .map(str::trim)
        .filter(|id| !id.is_empty());
    if let Some(id) = flow_id {
        repository::get_flow(&s.pool, id)
            .await
            .map_err(|_| AppError::Invalid)?;
    }
    repository::set_default_failure_flow(&s.pool, flow_id).await?;
    Ok(Json(DefaultFailureFlow {
        flow_id: flow_id.map(str::to_owned),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn graph(value: serde_json::Value) -> FlowGraph {
        serde_json::from_value(value).unwrap_or_default()
    }

    fn calling(target: &str) -> FlowGraph {
        graph(json!({"nodes": [
            {"id": "s", "type": "essentiel.sub-flow", "name": "s", "config": {"flowId": target}}
        ]}))
    }

    #[test]
    fn accepts_existing_links_and_refuses_cycles_through_the_saved_flow() {
        let known = HashMap::from([
            ("flow.b".to_owned(), calling("flow.a")),
            ("flow.c".to_owned(), FlowGraph::default()),
        ]);
        assert!(link_errors("flow.a", &calling("flow.c"), &known).is_empty());
        let cycle = link_errors("flow.a", &calling("flow.b"), &known);
        assert_eq!(cycle.len(), 1);
        assert!(cycle[0].contains("circulaire"));
    }

    #[test]
    fn refuses_an_unknown_failure_flow() {
        let mut with_failure = FlowGraph::default();
        with_failure.set_failure_flow_id(Some("flow.absent"));
        let errors = link_errors("flow.a", &with_failure, &HashMap::new());
        let expected = vec!["flux d'échec introuvable : flow.absent".to_owned()];
        assert_eq!(errors, expected);

        with_failure.set_failure_flow_id(Some("flow.a"));
        assert!(link_errors("flow.a", &with_failure, &HashMap::new()).is_empty());
    }
}
