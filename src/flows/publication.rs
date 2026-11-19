//! Intégrations déclenchées après la publication durable d'un résultat.

use super::{FlowContext, OperationRuntime, http_runner};
use crate::{
    api::AppState,
    domain::{Job, JobStepStatus},
    error::AppResult,
    repository,
};
use forgearr_shared::flows::{FlowGraph, FlowNode};
use std::path::Path;
use uuid::Uuid;

pub(super) const DEFERRED_MESSAGE: &str = "Notification en attente de publication";

pub(super) fn after_publication(node: &FlowNode) -> bool {
    node.config
        .get("after_publication")
        .and_then(serde_json::Value::as_bool)
        == Some(true)
}

pub(crate) async fn notify_after_publication(
    state: &AppState,
    job: &Job,
    worker_id: Uuid,
    output: &Path,
) -> AppResult<()> {
    execute_notifications(state, job, worker_id, output, false)
        .await
        .map(|_| ())
}

pub(crate) async fn retry_notifications(
    state: &AppState,
    job: &Job,
    output: &Path,
) -> AppResult<usize> {
    execute_notifications(state, job, job.worker_id.unwrap_or_default(), output, true).await
}

async fn execute_notifications(
    state: &AppState,
    job: &Job,
    worker_id: Uuid,
    output: &Path,
    retry: bool,
) -> AppResult<usize> {
    let (Some(flow_id), Some(version)) = (job.flow_id.as_deref(), job.flow_version) else {
        return Ok(0);
    };
    let flow = repository::get_flow_version(&state.pool, flow_id, version).await?;
    let Ok(graph) = serde_json::from_str::<FlowGraph>(&flow.definition) else {
        return Ok(0);
    };
    if !graph.nodes.iter().any(after_publication) {
        return Ok(0);
    }
    let logs = repository::list_job_steps(&state.pool, job.id).await?;
    let roots = crate::plugins::catalog_roots(&state.pool, &state.config).await;
    let registry = state.catalog_cache.operations(&roots).await;
    let plugin_configs = super::load_flow_plugin_configs(
        &state.pool,
        &state.config.config_dir,
        &registry,
        Some(&flow),
    )
    .await;
    let root = match repository::job_source(&state.pool, job.id).await? {
        Some(source) => source.source_root.into(),
        None => repository::library_worker_local_path_for_job(&state.pool, job, worker_id)
            .await?
            .map_or_else(|| state.config.input_dir.clone(), Into::into),
    };
    let policy = crate::runtime_settings::load_effective(&state.pool, &state.config)
        .await
        .integration_policy();
    let mut ctx = FlowContext::new(
        job.id,
        job.source_name.clone(),
        output.to_owned(),
        serde_json::Value::Null,
    );
    ctx.media_root = Some(root);
    ctx.status = "succeeded".to_owned();
    let mut attempts = 0;
    for step in logs {
        let Some(node) = graph
            .nodes
            .iter()
            .find(|node| node.id == step.node_id && after_publication(node))
        else {
            continue;
        };
        let Some(op) = registry.resolve_node_operation(node) else {
            continue;
        };
        let OperationRuntime::Http(runtime) = &op.runtime else {
            continue;
        };
        if op.capability != "integration" {
            continue;
        }
        let claimed = sqlx::query(
            "UPDATE job_steps SET deferred_notification=1, notification_retry_until=datetime('now', '+30 minutes')
             WHERE job_id=?1 AND step_index=?2
               AND (notification_retry_until IS NULL OR notification_retry_until<=datetime('now'))
               AND ((status='skipped' AND message=?3)
                    OR (?4 AND deferred_notification=1 AND status IN ('failed','skipped')))",
        ).bind(job.id).bind(step.step_index).bind(DEFERRED_MESSAGE).bind(retry)
        .execute(&state.pool).await?.rows_affected();
        if claimed != 1 {
            continue;
        }
        attempts += 1;
        ctx.current_step.clone_from(&node.id);
        let mut config = node.config.clone();
        if let Some(fields) = config.as_object_mut()
            && let Some(param) = &runtime.fail_on_error_param
        {
            fields.insert(param.clone(), serde_json::Value::Bool(true));
        }
        let outcome = http_runner::execute_http_operation_with_policy(
            runtime,
            flow_id,
            &node.id,
            &config,
            &plugin_configs,
            &ctx,
            Some(&state.config.config_dir),
            &state.http_client,
            policy,
        )
        .await;
        let (status, message) = match outcome {
            Ok(http_runner::HttpStepOutcome::Succeeded { message }) => {
                (JobStepStatus::Succeeded, message)
            }
            Ok(http_runner::HttpStepOutcome::Skipped { message }) => {
                (JobStepStatus::Skipped, Some(message))
            }
            Err(err) => (JobStepStatus::Failed, Some(err.message)),
        };
        sqlx::query("UPDATE job_steps SET status=?, message=?, notification_retry_until=NULL WHERE job_id=? AND step_index=?")
            .bind(status).bind(message).bind(job.id).bind(step.step_index)
            .execute(&state.pool).await?;
    }
    Ok(attempts)
}
