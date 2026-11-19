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
    let (Some(flow_id), Some(version)) = (job.flow_id.as_deref(), job.flow_version) else {
        return Ok(());
    };
    let flow = repository::get_flow_version(&state.pool, flow_id, version).await?;
    let Ok(graph) = serde_json::from_str::<FlowGraph>(&flow.definition) else {
        return Ok(());
    };
    if !graph.nodes.iter().any(after_publication) {
        return Ok(());
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
    for step in logs.into_iter().filter(|step| {
        step.status == JobStepStatus::Skipped && step.message.as_deref() == Some(DEFERRED_MESSAGE)
    }) {
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
        ctx.current_step.clone_from(&node.id);
        let mut config = node.config.clone();
        if let Some(fields) = config.as_object_mut() {
            fields.insert("fail_on_error".to_owned(), serde_json::Value::Bool(false));
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
        sqlx::query("UPDATE job_steps SET status=?, message=? WHERE job_id=? AND step_index=? AND message=?")
            .bind(status).bind(message).bind(job.id).bind(step.step_index).bind(DEFERRED_MESSAGE)
            .execute(&state.pool).await?;
    }
    Ok(())
}
