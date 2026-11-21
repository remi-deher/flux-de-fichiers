//! Flux liés figés à la création d'une tâche : flux d'échec et sous-flux (ADR 0026).

use crate::error::AppResult;
use forgearr_shared::flows::{FlowGraph, LinkedFlow, LinkedFlowRole, MAX_SUB_FLOW_DEPTH};
use sqlx::{SqliteConnection, SqlitePool};
use std::collections::VecDeque;
use uuid::Uuid;

/// Réglage d'instance : flux d'échec appliqué aux flux qui n'en désignent pas.
pub const DEFAULT_FAILURE_FLOW_KEY: &str = "default_failure_flow_id";

/// Flux d'échec global, s'il est défini.
pub async fn default_failure_flow(pool: &SqlitePool) -> AppResult<Option<String>> {
    let mut conn = pool.acquire().await?;
    default_failure_flow_with(&mut conn).await
}

async fn default_failure_flow_with(conn: &mut SqliteConnection) -> AppResult<Option<String>> {
    let value: Option<String> =
        sqlx::query_scalar("SELECT value FROM instance_settings WHERE key=?1")
            .bind(DEFAULT_FAILURE_FLOW_KEY)
            .fetch_optional(conn)
            .await?;
    Ok(value.filter(|id| !id.trim().is_empty()))
}

/// Définit (ou efface, avec `None`) le flux d'échec global.
pub async fn set_default_failure_flow(pool: &SqlitePool, flow_id: Option<&str>) -> AppResult<()> {
    match flow_id.map(str::trim).filter(|id| !id.is_empty()) {
        Some(id) => super::set_setting(pool, DEFAULT_FAILURE_FLOW_KEY, id).await,
        None => {
            sqlx::query("DELETE FROM instance_settings WHERE key=?1")
                .bind(DEFAULT_FAILURE_FLOW_KEY)
                .execute(pool)
                .await?;
            Ok(())
        }
    }
}

async fn version_definition(
    conn: &mut SqliteConnection,
    flow_id: &str,
    version: i64,
) -> AppResult<Option<String>> {
    Ok(
        sqlx::query_scalar("SELECT definition FROM flow_versions WHERE flow_id=?1 AND version=?2")
            .bind(flow_id)
            .bind(version)
            .fetch_optional(conn)
            .await?,
    )
}

async fn current_flow(
    conn: &mut SqliteConnection,
    flow_id: &str,
) -> AppResult<Option<(i64, String)>> {
    Ok(
        sqlx::query_as("SELECT version, definition FROM flows WHERE id=?1")
            .bind(flow_id)
            .fetch_optional(conn)
            .await?,
    )
}

async fn insert_link(
    conn: &mut SqliteConnection,
    job: Uuid,
    role: LinkedFlowRole,
    flow_id: &str,
    version: i64,
) -> AppResult<()> {
    sqlx::query(
        "INSERT OR IGNORE INTO job_linked_flows (job_id, role, flow_id, version) VALUES (?1, ?2, ?3, ?4)",
    )
    .bind(job)
    .bind(role.as_str())
    .bind(flow_id)
    .bind(version)
    .execute(conn)
    .await?;
    Ok(())
}

/// Fige, pour la tâche `job`, la version courante du flux d'échec (celui du flux, sinon le
/// global) et de tous les sous-flux atteignables depuis le flux figé `root` et le flux d'échec.
/// Remplace un figement antérieur. Un flux lié introuvable est ignoré (il échouera à l'exécution).
/// L'exploration s'arrête à `MAX_SUB_FLOW_DEPTH` niveaux (le flux principal et le flux d'échec
/// sont au niveau 0) : au-delà, l'exécution refuse de toute façon l'appel.
pub(super) async fn freeze_linked_flows(
    conn: &mut SqliteConnection,
    job: Uuid,
    root: Option<(&str, i64)>,
) -> AppResult<()> {
    sqlx::query("DELETE FROM job_linked_flows WHERE job_id=?1")
        .bind(job)
        .execute(&mut *conn)
        .await?;
    let root_graph = match root {
        Some((flow_id, version)) => version_definition(&mut *conn, flow_id, version)
            .await?
            .map(|definition| FlowGraph::parse(&definition)),
        None => None,
    };
    let failure_id = match root_graph.as_ref().and_then(FlowGraph::failure_flow_id) {
        Some(id) => Some(id.to_owned()),
        None => default_failure_flow_with(&mut *conn).await?,
    };

    let mut pending: VecDeque<(String, usize)> = VecDeque::new();
    let mut frozen_sub_flows: Vec<String> = Vec::new();
    if let Some(graph) = &root_graph {
        pending.extend(called_by(graph, 0));
    }
    if let Some(failure_id) = failure_id.as_deref()
        && let Some((version, definition)) = current_flow(&mut *conn, failure_id).await?
    {
        let role = LinkedFlowRole::Failure;
        insert_link(&mut *conn, job, role, failure_id, version).await?;
        pending.extend(called_by(&FlowGraph::parse(&definition), 0));
    }
    while let Some((flow_id, depth)) = pending.pop_front() {
        if depth > MAX_SUB_FLOW_DEPTH || frozen_sub_flows.contains(&flow_id) {
            continue;
        }
        frozen_sub_flows.push(flow_id.clone());
        let Some((version, definition)) = current_flow(&mut *conn, &flow_id).await? else {
            tracing::warn!(job_id = %job, flow_id = %flow_id, "sous-flux introuvable au figement");
            continue;
        };
        let role = LinkedFlowRole::SubFlow;
        insert_link(&mut *conn, job, role, &flow_id, version).await?;
        pending.extend(called_by(&FlowGraph::parse(&definition), depth));
    }
    Ok(())
}

/// Sous-flux appelés par `graph`, situé au niveau `depth`, avec leur propre niveau.
fn called_by(graph: &FlowGraph, depth: usize) -> impl Iterator<Item = (String, usize)> {
    let child_depth = depth + 1;
    graph
        .sub_flow_ids()
        .into_iter()
        .map(move |id| (id, child_depth))
}

/// Versions figées des flux liés de la tâche, prêtes à voyager avec le bail.
pub async fn frozen_linked_flows(pool: &SqlitePool, job: Uuid) -> AppResult<Vec<LinkedFlow>> {
    let rows: Vec<(String, String, i64)> = sqlx::query_as(
        "SELECT role, flow_id, version FROM job_linked_flows WHERE job_id=?1 ORDER BY role, flow_id",
    )
    .bind(job)
    .fetch_all(pool)
    .await?;
    let mut linked = Vec::with_capacity(rows.len());
    for (role, flow_id, version) in rows {
        let Some(role) = LinkedFlowRole::parse(&role) else {
            continue;
        };
        let flow = super::get_flow_version(pool, &flow_id, version).await?;
        linked.push(LinkedFlow { role, flow });
    }
    Ok(linked)
}
