//! Moteur d'exécution de flux 100% déclaratif piloté par contrats d'opération (`OperationRegistry`).

pub mod binary_runner;
mod block;
mod condition;
pub mod contract;
mod custom_args;
pub mod header_secrets;
pub mod http_runner;
pub mod lease_secrets;

#[cfg(test)]
mod engine_tests;

use crate::{media::OutputProfile, tools_cache::ToolCacheManager};
use chrono::Utc;
use serde_json::Value;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Arc,
};
use uuid::Uuid;

pub use condition::{ConditionConfig, evaluate_condition};
pub use contract::{
    BinaryExecutionMode, BinaryRuntime, ContractError, HttpRuntime, OperationContract,
    OperationRegistry, OperationRuntime, PrimitiveKind, ToolRef,
};
pub use custom_args::{CustomArgsError, parse_custom_args, safe_custom_args};
pub use flux_shared::flows::{FlowBlockKind, FlowConnection, FlowGraph, FlowNode, FlowRecord};
pub use header_secrets::{
    decrypt_definition_headers, has_webhook_headers, mask_definition, migrate_flow_secrets,
    rotate_definition, seal_definition,
};
pub use lease_secrets::{decrypt_plugin_settings_value, load_flow_plugin_configs};

const MAX_FLOW_STEPS: usize = 32;

/// Callback thread-safe de progression `(pourcentage, ligne console)`.
pub type FlowProgressCallback = Arc<dyn Fn(u32, &str) + Send + Sync>;

/// Contexte d'exécution d'une étape de flux.
#[derive(Clone, Debug)]
pub struct FlowContext {
    pub job_id: Uuid,
    pub source_name: String,
    pub working_file: PathBuf,
    pub media_root: Option<PathBuf>,
    pub media_info: Value,
    pub current_step: String,
    pub status: String,
    pub timestamp: String,
}

impl FlowContext {
    pub fn new(
        job_id: Uuid,
        source_name: String,
        working_file: PathBuf,
        media_info: Value,
    ) -> Self {
        Self {
            job_id,
            source_name,
            working_file,
            media_root: None,
            media_info,
            current_step: "start".to_owned(),
            status: "running".to_owned(),
            timestamp: Utc::now().to_rfc3339(),
        }
    }
}

/// Résultat final d'une exécution complète de flux (ou de l'opération de repli par défaut).
#[derive(Debug)]
pub struct FlowExecutionOutcome {
    pub final_file: PathBuf,
    pub is_in_place: bool,
    pub warning: bool,
    pub output_profile: OutputProfile,
    pub steps: Vec<crate::domain::JobStepEntry>,
}

/// Échec complet d'exécution avec le journal exact des étapes exécutées.
#[derive(Debug)]
pub struct FlowExecutionError {
    pub code: flux_shared::jobs::JobErrorCode,
    pub message: String,
    pub steps: Vec<crate::domain::JobStepEntry>,
}

/// Moteur générique qui exécute tout flux exclusivement selon les contrats d'opération déclarés.
pub struct FlowEngine<'a> {
    pub registry: &'a OperationRegistry,
    pub cache_mgr: &'a ToolCacheManager,
    pub http_client: &'a reqwest::Client,
    pub secrets_dir: Option<&'a Path>,
    pub plugin_configs: &'a HashMap<String, Value>,
    pub tool_versions: &'a HashMap<String, String>,
    pub media_root: Option<&'a Path>,
}

impl<'a> FlowEngine<'a> {
    /// Exécute l'opération marquée `defaultFallback: true` lorsqu'aucun flux n'est rattaché.
    pub async fn execute_default(
        &self,
        source_file: &Path,
        work_dir: &Path,
        on_progress: FlowProgressCallback,
    ) -> Result<FlowExecutionOutcome, FlowExecutionError> {
        let op = self
            .registry
            .default_fallback_operation()
            .ok_or_else(|| FlowExecutionError {
                code: flux_shared::jobs::JobErrorCode::RemuxFailed,
                message: "aucune opération par défaut (defaultFallback) dans le catalogue"
                    .to_owned(),
                steps: Vec::new(),
            })?;
        let OperationRuntime::Binary(bin_rt) = &op.runtime else {
            return Err(FlowExecutionError {
                code: flux_shared::jobs::JobErrorCode::RemuxFailed,
                message: format!("contrat par défaut « {} » non binaire", op.id),
                steps: Vec::new(),
            });
        };
        let media_info = binary_runner::probe_media_tree(
            self.registry,
            self.cache_mgr,
            self.tool_versions,
            source_file,
        )
        .await
        .map_err(|err| FlowExecutionError {
            code: err.code,
            message: err.message,
            steps: Vec::new(),
        })?;

        let outcome = binary_runner::execute_binary_operation(
            binary_runner::BinaryExecContext {
                registry: self.registry,
                cache_mgr: self.cache_mgr,
                tool_versions: self.tool_versions,
                work_dir,
                working_file: source_file,
                media_info: &media_info,
                step_index: 0,
                on_progress,
            },
            op,
            bin_rt,
            &Value::Null,
        )
        .await
        .map_err(|err| FlowExecutionError {
            code: err.code,
            message: err.message,
            steps: Vec::new(),
        })?;

        Ok(FlowExecutionOutcome {
            final_file: outcome
                .next_working_file
                .unwrap_or_else(|| source_file.to_path_buf()),
            is_in_place: !outcome.is_staged_output,
            warning: outcome.warning,
            output_profile: bin_rt
                .output_profile
                .unwrap_or(OutputProfile::StrictMkvRemux),
            steps: vec![crate::domain::JobStepEntry {
                step_index: 0,
                node_id: "default".to_owned(),
                block_kind: op.id.clone(),
                operation_id: Some(op.id.clone()),
                status: crate::domain::JobStepStatus::Succeeded,
                message: outcome.message,
                output_excerpt: outcome.excerpt,
            }],
        })
    }

    /// Exécute un graphe de flux pas à pas uniquement via les contrats de `self.registry`.
    pub async fn execute_flow(
        &self,
        job_id: Uuid,
        flow: &FlowRecord,
        source_file: &Path,
        work_dir: &Path,
        on_progress: FlowProgressCallback,
    ) -> Result<FlowExecutionOutcome, FlowExecutionError> {
        let graph: FlowGraph =
            serde_json::from_str(&flow.definition).map_err(|err| FlowExecutionError {
                code: flux_shared::jobs::JobErrorCode::RemuxFailed,
                message: format!("définition de flux invalide : {err}"),
                steps: Vec::new(),
            })?;

        let media_info = binary_runner::probe_media_tree(
            self.registry,
            self.cache_mgr,
            self.tool_versions,
            source_file,
        )
        .await
        .map_err(|err| FlowExecutionError {
            code: err.code,
            message: err.message,
            steps: Vec::new(),
        })?;

        let mut current_working_file = source_file.to_path_buf();
        let mut is_in_place = true;
        let mut final_warning = false;
        let mut last_output_profile: Option<OutputProfile> = None;
        let mut steps: Vec<crate::domain::JobStepEntry> = Vec::new();
        let mut ctx = FlowContext::new(
            job_id,
            source_file
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| "source.mkv".into()),
            current_working_file.clone(),
            media_info,
        );
        ctx.media_root = self.media_root.map(Path::to_path_buf);

        let Some(input_node) = graph
            .nodes
            .iter()
            .find(|node| block::is_entry_node(self.registry, node))
            .cloned()
            .or_else(|| graph.nodes.first().cloned())
        else {
            return Err(FlowExecutionError {
                code: flux_shared::jobs::JobErrorCode::RemuxFailed,
                message: "le flux ne contient aucun bloc d'entrée".to_owned(),
                steps,
            });
        };

        let mut current_node_id = input_node.id;
        let mut remaining_steps = MAX_FLOW_STEPS;
        let mut step_index: i32 = 0;

        while remaining_steps > 0 {
            remaining_steps -= 1;
            let Some(node) = graph
                .nodes
                .iter()
                .find(|candidate| candidate.id == current_node_id)
                .cloned()
            else {
                break;
            };
            step_index += 1;

            let Some(op) = self.registry.resolve_node_operation(&node) else {
                let message = format!("aucun contrat d'opération pour le bloc « {} »", node.kind);
                steps.push(failed_step(step_index, &node, None, message.clone(), None));
                return Err(FlowExecutionError {
                    code: flux_shared::jobs::JobErrorCode::RemuxFailed,
                    message,
                    steps,
                });
            };

            let mut step_ctx = ctx.clone();
            step_ctx.current_step = node.name.clone();
            let mut next_port: Option<String> = None;
            let mut stop_pipeline = false;
            let mut step_status = crate::domain::JobStepStatus::Succeeded;
            let mut step_message: Option<String> = None;
            let mut step_excerpt: Option<String> = None;

            match &op.runtime {
                OperationRuntime::Primitive(prim) => match prim.primitive {
                    PrimitiveKind::Entry => {}
                    PrimitiveKind::Terminal => {
                        stop_pipeline = true;
                    }
                    PrimitiveKind::Condition => {
                        let cond_cfg: ConditionConfig =
                            serde_json::from_value(node.config.clone()).unwrap_or_default();
                        let matched = evaluate_condition(&cond_cfg, &ctx.media_info);
                        next_port = Some(if matched { "true" } else { "false" }.to_owned());
                    }
                },
                OperationRuntime::Binary(bin_rt) => {
                    let outcome = binary_runner::execute_binary_operation(
                        binary_runner::BinaryExecContext {
                            registry: self.registry,
                            cache_mgr: self.cache_mgr,
                            tool_versions: self.tool_versions,
                            work_dir,
                            working_file: &current_working_file,
                            media_info: &ctx.media_info,
                            step_index,
                            on_progress: on_progress.clone(),
                        },
                        op,
                        bin_rt,
                        &node.config,
                    )
                    .await;
                    match outcome {
                        Ok(res) => {
                            if let Some(updated_info) = res.updated_media_info {
                                ctx.media_info = updated_info;
                            }
                            if let Some(next_file) = res.next_working_file {
                                current_working_file = next_file;
                                ctx.working_file = current_working_file.clone();
                            }
                            if res.is_staged_output {
                                is_in_place = false;
                            }
                            final_warning |= res.warning;
                            if bin_rt.output_profile.is_some() {
                                last_output_profile = bin_rt.output_profile;
                            }
                            step_message = res.message;
                            step_excerpt = res.excerpt;
                        }
                        Err(err) => {
                            steps.push(failed_step(
                                step_index,
                                &node,
                                Some(op.id.clone()),
                                err.message.clone(),
                                err.excerpt,
                            ));
                            return Err(FlowExecutionError {
                                code: err.code,
                                message: err.message,
                                steps,
                            });
                        }
                    }
                }
                OperationRuntime::Http(http_rt) => {
                    match http_runner::execute_http_operation(
                        http_rt,
                        &flow.id,
                        &node.id,
                        &node.config,
                        self.plugin_configs,
                        &step_ctx,
                        self.secrets_dir,
                        self.http_client,
                    )
                    .await
                    {
                        Ok(http_runner::HttpStepOutcome::Succeeded { message }) => {
                            step_message = message;
                        }
                        Ok(http_runner::HttpStepOutcome::Skipped { message }) => {
                            step_status = crate::domain::JobStepStatus::Skipped;
                            step_message = Some(message);
                        }
                        Err(err) => {
                            steps.push(failed_step(
                                step_index,
                                &node,
                                Some(op.id.clone()),
                                err.message.clone(),
                                None,
                            ));
                            return Err(FlowExecutionError {
                                code: err.code,
                                message: err.message,
                                steps,
                            });
                        }
                    }
                }
            }

            steps.push(crate::domain::JobStepEntry {
                step_index,
                node_id: node.id.clone(),
                block_kind: node.kind.as_str().to_owned(),
                operation_id: Some(op.id.clone()),
                status: step_status,
                message: step_message,
                output_excerpt: step_excerpt,
            });

            if stop_pipeline {
                tracing::info!(job_id = %job_id, "reached terminal node, terminating pipeline");
                break;
            }
            let Some(conn) =
                next_connection(&graph.connections, &current_node_id, next_port.as_deref())
            else {
                tracing::info!(
                    job_id = %job_id,
                    node_id = %current_node_id,
                    "no outgoing connection, pipeline ending"
                );
                break;
            };
            current_node_id = conn.to.clone();
        }

        let resolved_profile = last_output_profile
            .or_else(|| self.registry.output_profile_for_flow(Some(flow), None))
            .unwrap_or(OutputProfile::StrictMkvRemux);

        Ok(FlowExecutionOutcome {
            final_file: current_working_file,
            is_in_place,
            warning: final_warning,
            output_profile: resolved_profile,
            steps,
        })
    }
}

fn failed_step(
    step_index: i32,
    node: &FlowNode,
    operation_id: Option<String>,
    message: String,
    excerpt: Option<String>,
) -> crate::domain::JobStepEntry {
    crate::domain::JobStepEntry {
        step_index,
        node_id: node.id.clone(),
        block_kind: node.kind.as_str().to_owned(),
        operation_id,
        status: crate::domain::JobStepStatus::Failed,
        message: Some(message),
        output_excerpt: excerpt,
    }
}

fn next_connection<'a>(
    connections: &'a [FlowConnection],
    source_id: &str,
    port: Option<&str>,
) -> Option<&'a FlowConnection> {
    if let Some(port) = port
        && let Some(exact) = connections
            .iter()
            .find(|c| c.from == source_id && c.port.as_deref() == Some(port))
    {
        return Some(exact);
    }
    connections.iter().find(|c| c.from == source_id)
}
