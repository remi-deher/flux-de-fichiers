//! Moteur d'exécution de flux 100% déclaratif piloté par contrats d'opération (`OperationRegistry`).

pub mod binary_runner;
mod block;
mod condition;
mod deferred_variables;
pub mod contract;
mod custom_args;
pub mod header_secrets;
pub mod http_runner;
pub mod lease_secrets;
pub mod placement;
pub(crate) mod publication;
mod script_branch;
mod sub_flow;
pub mod script_runner;
pub mod substitution;
mod variables_step;

#[cfg(test)]
mod engine_tests;
#[cfg(test)]
mod placement_flow_tests;
#[cfg(test)]
mod script_branch_flow_tests;
#[cfg(test)]
mod sub_flow_tests;
#[cfg(test)]
mod variables_flow_tests;

use crate::{media::OutputSpec, tools_cache::ToolCacheManager};
use chrono::Utc;
use forgearr_shared::{
    capabilities::WorkerCapabilities,
    flows::{BranchPort, FlowVariables, LinkedFlow, ScriptRouting},
    relay::RelayRequest,
};
use serde_json::{Map, Value};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Arc,
};
use uuid::Uuid;

pub use condition::{
    ConditionConfig, definition_condition_errors, evaluate_condition,
    evaluate_condition_with_variables, graph_condition_errors,
};
pub use contract::{
    BinaryExecutionMode, BinaryRuntime, ContractError, HttpRuntime, OperationContract,
    OperationRegistry, OperationRuntime, PrimitiveKind, ScriptInterpreter, ScriptRuntime, ToolRef,
};
pub use custom_args::{CustomArgsError, parse_custom_args, safe_custom_args};
pub use forgearr_shared::flows::{FlowBlockKind, FlowConnection, FlowGraph, FlowNode, FlowRecord};
pub use header_secrets::{
    decrypt_definition_headers, extract_and_mask_definition_headers, has_webhook_headers,
    inject_definition_headers, mask_definition, migrate_flow_secrets, rotate_definition,
    seal_definition,
};
pub use lease_secrets::{
    decrypt_plugin_settings_value, load_flow_plugin_configs, seal_claim_secrets_for_worker,
    unseal_claim_secrets_for_worker,
};

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
    /// Variables de la tâche (intrinsèques et `vars.*`), lues par les modèles `{$…}`.
    pub variables: FlowVariables,
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
            variables: FlowVariables::default(),
        }
    }

    /// Change de fichier de travail et recalcule les variables `file.*`.
    pub fn set_working_file(&mut self, file: PathBuf) {
        substitution::set_file_variables(&mut self.variables, &file);
        self.working_file = file;
    }
}

/// Identité de l'exécution transmise par le bail (variables `library.*`, `worker.name`, reprise).
#[derive(Clone, Copy, Debug, Default)]
pub struct FlowRunInfo<'a> {
    pub library_id: Option<Uuid>,
    pub library_name: Option<&'a str>,
    pub worker_name: Option<&'a str>,
    /// Variables personnalisées restaurées après un relais.
    pub resume_variables: Option<&'a Map<String, Value>>,
    /// Flux liés figés avec la tâche (sous-flux, flux d'échec).
    pub linked_flows: &'a [LinkedFlow],
    /// Erreur de la tâche, présente seulement pendant l'exécution du flux d'échec.
    pub failure: Option<FailureInfo<'a>>,
}

/// Erreur transmise au flux d'échec (variables `error.code`, `error.message`).
#[derive(Clone, Copy, Debug)]
pub struct FailureInfo<'a> {
    pub code: &'a str,
    pub message: &'a str,
}

/// Résultat final d'une exécution complète de flux (ou de l'opération de repli par défaut).
#[derive(Debug)]
pub struct FlowExecutionOutcome {
    pub final_file: PathBuf,
    pub is_in_place: bool,
    pub warning: bool,
    pub output_spec: OutputSpec,
    pub steps: Vec<crate::domain::JobStepEntry>,
    /// Présent quand le bloc « Relayer la tâche » a arrêté l'exécution locale (ADR 0018).
    pub relay: Option<RelayRequest>,
}

/// Échec complet d'exécution avec le journal exact des étapes exécutées.
#[derive(Debug)]
pub struct FlowExecutionError {
    pub code: forgearr_shared::jobs::JobErrorCode,
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
    pub logical_source_name: Option<&'a str>,
    pub allow_private_integrations: Option<bool>,
    /// Capacités matérielles du worker qui exécute (bloc « Capacité du worker »).
    pub worker_capabilities: Option<&'a WorkerCapabilities>,
    /// Nœud où reprendre un flux relayé, au lieu du bloc d'entrée.
    pub resume_node_id: Option<&'a str>,
    /// Identité de l'exécution, source des variables intrinsèques.
    pub run_info: FlowRunInfo<'a>,
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
                code: forgearr_shared::jobs::JobErrorCode::OperationFailed,
                message: "aucune opération par défaut (defaultFallback) dans le catalogue"
                    .to_owned(),
                steps: Vec::new(),
            })?;
        let OperationRuntime::Binary(bin_rt) = &op.runtime else {
            return Err(FlowExecutionError {
                code: forgearr_shared::jobs::JobErrorCode::OperationFailed,
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
                source_file,
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
            output_spec: bin_rt.output_spec().unwrap_or_default(),
            steps: vec![crate::domain::JobStepEntry {
                step_index: 0,
                node_id: "default".to_owned(),
                block_kind: op.id.clone(),
                operation_id: Some(op.id.clone()),
                status: crate::domain::JobStepStatus::Succeeded,
                message: outcome.message,
                output_excerpt: outcome.excerpt,
                variables: None,
            }],
            relay: None,
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
        let mut graph: FlowGraph =
            serde_json::from_str(&flow.definition).map_err(|err| FlowExecutionError {
                code: forgearr_shared::jobs::JobErrorCode::OperationFailed,
                message: format!("définition de flux invalide : {err}"),
                steps: Vec::new(),
            })?;

        let media_info = match binary_runner::probe_media_tree(
            self.registry,
            self.cache_mgr,
            self.tool_versions,
            source_file,
        )
        .await
        {
            Ok(info) => info,
            Err(err) if self.registry.graph_accepts_non_media(&graph) => {
                tracing::info!(
                    job_id = %job_id,
                    error = %err.message,
                    "entrée non média acceptée par le flux ; sonde différée"
                );
                Value::Object(serde_json::Map::new())
            }
            Err(err) => {
                return Err(FlowExecutionError {
                    code: err.code,
                    message: err.message,
                    steps: Vec::new(),
                });
            }
        };

        let mut current_working_file = source_file.to_path_buf();
        let mut is_in_place = true;
        let mut source_modified_in_place = false;
        let mut final_warning = false;
        let mut last_output_spec: Option<OutputSpec> = None;
        let mut steps: Vec<crate::domain::JobStepEntry> = Vec::new();
        let mut ctx = FlowContext::new(
            job_id,
            self.logical_source_name
                .map(str::to_owned)
                .unwrap_or_else(|| {
                    source_file
                        .file_name()
                        .map(|name| name.to_string_lossy().into_owned())
                        .unwrap_or_else(|| "source".into())
                }),
            current_working_file.clone(),
            media_info,
        );
        ctx.media_root = self.media_root.map(Path::to_path_buf);
        let job_id_text = job_id.to_string();
        let library_id_text = self.run_info.library_id.map(|id| id.to_string());
        substitution::set_identity_variables(
            &mut ctx.variables,
            &substitution::RunIdentity {
                job_id: Some(&job_id_text),
                flow_id: Some(&flow.id),
                source_name: Some(&ctx.source_name),
                library_id: library_id_text.as_deref(),
                library_name: self.run_info.library_name,
                worker_name: self.run_info.worker_name,
            },
        );
        ctx.set_working_file(current_working_file.clone());
        if let Some(failure) = self.run_info.failure {
            substitution::set_failure_variables(&mut ctx.variables, failure.code, failure.message);
        }
        if let Some(resumed) = self.run_info.resume_variables
            && let Err(err) = ctx.variables.merge_custom(resumed)
        {
            return Err(FlowExecutionError {
                code: forgearr_shared::jobs::JobErrorCode::OperationFailed,
                message: format!("variables de reprise invalides : {err}"),
                steps,
            });
        }

        let Some(input_node_id) = block::entry_node_id(self.registry, &graph) else {
            return Err(FlowExecutionError {
                code: forgearr_shared::jobs::JobErrorCode::OperationFailed,
                message: "le flux ne contient aucun bloc d'entrée".to_owned(),
                steps,
            });
        };

        let mut current_node_id = match self.resume_node_id {
            Some(resume) if graph.nodes.iter().any(|node| node.id == resume) => resume.to_owned(),
            Some(resume) => {
                return Err(FlowExecutionError {
                    code: forgearr_shared::jobs::JobErrorCode::OperationFailed,
                    message: format!("nœud de reprise introuvable dans le flux : {resume}"),
                    steps,
                });
            }
            None => input_node_id,
        };
        let mut active_flow_id = flow.id.clone();
        let mut frames: Vec<sub_flow::SubFlowFrame> = Vec::new();
        let mut relay: Option<RelayRequest> = None;
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
                    code: forgearr_shared::jobs::JobErrorCode::OperationFailed,
                    message,
                    steps,
                });
            };

            let mut step_ctx = ctx.clone();
            step_ctx.current_step = node.name.clone();
            // Configuration rendue avec les variables du flux (les nœuds HTTP rendent la leur).
            let node_config = substitution::substitute_node_config(&node.config, &ctx.variables);
            let mut next_port: Option<String> = None;
            let mut stop_pipeline = false;
            let mut step_status = crate::domain::JobStepStatus::Succeeded;
            let mut step_message: Option<String> = None;
            let mut step_excerpt: Option<String> = None;
            let mut step_variables: Option<Map<String, Value>> = None;
            let mut jump_to: Option<String> = None;

            match &op.runtime {
                OperationRuntime::Primitive(prim) => match prim.primitive {
                    PrimitiveKind::Entry => {}
                    PrimitiveKind::Terminal => {
                        stop_pipeline = true;
                    }
                    PrimitiveKind::Condition => {
                        let cond_cfg = condition::read_condition(&node.id, &node_config);
                        let matched = evaluate_condition_with_variables(
                            &cond_cfg,
                            &ctx.media_info,
                            &ctx.variables,
                        );
                        next_port = Some(BranchPort::from_bool(matched).as_str().to_owned());
                    }
                    PrimitiveKind::Capability => {
                        match placement::evaluate_capability(&node_config, self.worker_capabilities)
                        {
                            Ok(port) => next_port = Some(port.to_owned()),
                            Err(message) => {
                                return Err(operation_failure(
                                    steps,
                                    step_index,
                                    &node,
                                    &op.id,
                                    message,
                                    None,
                                ));
                            }
                        }
                    }
                    PrimitiveKind::SubFlow => {
                        match sub_flow::enter_sub_flow(
                            self.registry,
                            self.run_info.linked_flows,
                            &node,
                            &active_flow_id,
                            &frames,
                        ) {
                            Ok(entry) => {
                                step_message = Some(entry.step_message());
                                remaining_steps += entry.graph.nodes.len();
                                jump_to = Some(entry.entry_node_id);
                                frames.push(sub_flow::SubFlowFrame {
                                    graph: std::mem::replace(&mut graph, entry.graph),
                                    flow_id: std::mem::replace(&mut active_flow_id, entry.flow_id),
                                    call_node_id: node.id.clone(),
                                });
                            }
                            Err(message) => {
                                return Err(operation_failure(
                                    steps, step_index, &node, &op.id, message, None,
                                ));
                            }
                        }
                    }
                    PrimitiveKind::Relay => {
                        let decided = if !frames.is_empty() {
                            Err("le relais est impossible dans un sous-flux".to_owned())
                        } else if source_modified_in_place {
                            Err(
                                "le transfert est impossible après modification de la source en place"
                                    .to_owned(),
                            )
                        } else {
                            next_connection(&graph.connections, &node.id, None)
                                .ok_or_else(|| {
                                    "le bloc « Relayer la tâche » n'a aucune sortie « Reprise »"
                                        .to_owned()
                                })
                                .and_then(|resume| {
                                    placement::build_relay_request(&node_config, &resume.to)
                                })
                        };
                        match decided {
                            Ok(mut request) => {
                                request.variables = ctx.variables.custom.clone();
                                relay = Some(request);
                                stop_pipeline = true;
                                step_message =
                                    Some("tâche relayée vers un autre worker".to_owned());
                            }
                            Err(message) => {
                                steps.push(failed_step(
                                    step_index,
                                    &node,
                                    Some(op.id.clone()),
                                    message.clone(),
                                    None,
                                ));
                                return Err(FlowExecutionError {
                                    code: forgearr_shared::jobs::JobErrorCode::OperationFailed,
                                    message,
                                    steps,
                                });
                            }
                        }
                    }
                    PrimitiveKind::SetVariable => {
                        match variables_step::apply_set_variable(&node_config, &mut ctx.variables) {
                            Ok(message) => step_message = Some(message),
                            Err(message) => {
                                steps.push(failed_step(
                                    step_index,
                                    &node,
                                    Some(op.id.clone()),
                                    message.clone(),
                                    None,
                                ));
                                return Err(FlowExecutionError {
                                    code: forgearr_shared::jobs::JobErrorCode::OperationFailed,
                                    message,
                                    steps,
                                });
                            }
                        }
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
                            source_file,
                            media_info: &ctx.media_info,
                            step_index,
                            on_progress: on_progress.clone(),
                        },
                        op,
                        bin_rt,
                        &node_config,
                    )
                    .await;
                    match outcome {
                        Ok(res) => {
                            if res.modified_in_place && current_working_file == source_file {
                                source_modified_in_place = true;
                            }
                            if let Some(updated_info) = res.updated_media_info {
                                ctx.media_info = updated_info;
                            }
                            if let Some(next_file) = res.next_working_file {
                                current_working_file = next_file;
                            }
                            // Taille et nom à jour, y compris après une modification en place.
                            ctx.set_working_file(current_working_file.clone());
                            if res.is_staged_output {
                                is_in_place = false;
                            }
                            final_warning |= res.warning;
                            if let Some(spec) = bin_rt.output_spec() {
                                last_output_spec = Some(spec);
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
                OperationRuntime::Http(_)
                    if op.capability == "integration" && publication::after_publication(&node) =>
                {
                    step_status = crate::domain::JobStepStatus::Skipped;
                    step_message = Some(publication::DEFERRED_MESSAGE.to_owned());
                    step_variables = Some(ctx.variables.custom.clone());
                }
                OperationRuntime::Http(http_rt) => {
                    let outbound_policy = crate::net_policy::OutboundPolicy {
                        allow_private_hosts: self.allow_private_integrations.unwrap_or_else(|| {
                            crate::net_policy::OutboundPolicy::integration_from_env()
                                .allow_private_hosts
                        }),
                        require_https: false,
                    };
                    let http_config =
                        substitution::substitute_flow_variables_only(&node.config, &ctx.variables);
                    match http_runner::execute_http_operation_with_policy(
                        http_rt,
                        &active_flow_id,
                        &node.id,
                        &http_config,
                        self.plugin_configs,
                        &step_ctx,
                        self.secrets_dir,
                        self.http_client,
                        outbound_policy,
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
                OperationRuntime::Script(script_rt) => {
                    let routing = ScriptRouting::from_config(&node.config);
                    let mut effective_rt = script_rt.clone();
                    if effective_rt.code.is_none()
                        && node.config.get("mode").and_then(Value::as_str) != Some("inline")
                        && let Some(script_id) = node
                            .config
                            .get("scriptId")
                            .and_then(Value::as_str)
                            .map(str::trim)
                            .filter(|s| !s.is_empty())
                        && let Some(target_op) =
                            self.registry.get(&format!("operation.{script_id}"))
                        && let OperationRuntime::Script(target_rt) = &target_op.runtime
                    {
                        let fallback_tool = effective_rt.tool.clone();
                        effective_rt = target_rt.clone();
                        if effective_rt.tool.is_none() {
                            effective_rt.tool = fallback_tool;
                        }
                    }
                    match script_runner::execute_script_operation(
                        op,
                        &effective_rt,
                        self.cache_mgr,
                        self.tool_versions,
                        &active_flow_id,
                        &node.id,
                        &node_config,
                        work_dir,
                        &current_working_file,
                        source_file,
                        &ctx.media_info,
                        &ctx.variables,
                        step_index,
                        on_progress.clone(),
                    )
                    .await
                    {
                        Ok(script_runner::ScriptStepOutcome::Succeeded {
                            next_working_file,
                            updated_media_info,
                            next_port: script_port,
                            stop_pipeline: script_stop,
                            variables,
                            message,
                            excerpt,
                        }) => {
                            if let Some(returned) = variables
                                && let Err(err) = ctx.variables.merge_custom(&returned)
                            {
                                let message = format!("variables renvoyées par le script : {err}");
                                return Err(operation_failure(
                                    steps, step_index, &node, &op.id, message, excerpt,
                                ));
                            }
                            next_port = match script_branch::resolve_script_port(
                                routing,
                                script_runner::ScriptOutcomeStatus::Ok,
                                script_port,
                            ) {
                                Ok(port) => port,
                                Err(message) => {
                                    return Err(operation_failure(
                                        steps, step_index, &node, &op.id, message, excerpt,
                                    ));
                                }
                            };
                            let reprobe =
                                next_working_file.is_some() && updated_media_info.is_none();
                            if let Some(updated_info) = updated_media_info {
                                ctx.media_info = updated_info;
                            }
                            if let Some(next_file) = next_working_file {
                                current_working_file = next_file;
                                ctx.set_working_file(current_working_file.clone());
                                is_in_place = false;
                            }
                            // Nouveau fichier sans métadonnées fournies : il est inspecté à son tour.
                            if reprobe {
                                match binary_runner::probe_media_tree(
                                    self.registry,
                                    self.cache_mgr,
                                    self.tool_versions,
                                    &current_working_file,
                                )
                                .await
                                {
                                    Ok(info) => ctx.media_info = info,
                                    Err(err) => {
                                        let message = format!(
                                            "fichier produit par le script illisible : {}",
                                            err.message
                                        );
                                        steps.push(failed_step(
                                            step_index,
                                            &node,
                                            Some(op.id.clone()),
                                            message.clone(),
                                            None,
                                        ));
                                        return Err(FlowExecutionError {
                                            code: err.code,
                                            message,
                                            steps,
                                        });
                                    }
                                }
                            }
                            stop_pipeline = script_stop;
                            step_message = message;
                            step_excerpt = excerpt;
                        }
                        Ok(script_runner::ScriptStepOutcome::Skipped {
                            message,
                            excerpt,
                            next_port: skip_port,
                        }) => {
                            next_port = match script_branch::resolve_script_port(
                                routing,
                                script_runner::ScriptOutcomeStatus::Skip,
                                skip_port,
                            ) {
                                Ok(port) => port,
                                Err(message) => {
                                    return Err(operation_failure(
                                        steps, step_index, &node, &op.id, message, excerpt,
                                    ));
                                }
                            };
                            step_status = crate::domain::JobStepStatus::Skipped;
                            step_message = Some(message);
                            step_excerpt = excerpt;
                        }
                        Err(err) => {
                            steps.push(failed_step(
                                step_index,
                                &node,
                                Some(op.id.clone()),
                                err.message.clone(),
                                err.excerpt.clone(),
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
                variables: step_variables,
            });

            if let Some(entry_node_id) = jump_to {
                current_node_id = entry_node_id;
                continue;
            }
            // Relais : la tâche quitte ce worker, aucun appelant n'est repris.
            let mut next_node = if stop_pipeline {
                None
            } else {
                next_connection(&graph.connections, &current_node_id, next_port.as_deref())
                    .map(|conn| conn.to.clone())
            };
            // Fin d'un sous-flux : reprise à la sortie du bloc appelant.
            while next_node.is_none() && relay.is_none() {
                let Some(frame) = frames.pop() else {
                    break;
                };
                graph = frame.graph;
                active_flow_id = frame.flow_id;
                next_node = next_connection(&graph.connections, &frame.call_node_id, None)
                    .map(|conn| conn.to.clone());
            }
            let Some(next) = next_node else {
                tracing::info!(
                    job_id = %job_id,
                    node_id = %current_node_id,
                    "terminal node or no outgoing connection, pipeline ending"
                );
                break;
            };
            current_node_id = next;
        }

        let resolved_spec = last_output_spec
            .or_else(|| self.registry.output_spec_for_flow(Some(flow), None))
            .unwrap_or_default();

        Ok(FlowExecutionOutcome {
            final_file: current_working_file,
            is_in_place,
            warning: final_warning,
            output_spec: resolved_spec,
            steps,
            relay,
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
        variables: None,
    }
}

/// Ajoute l'étape en échec et construit l'erreur `OperationFailed` qui arrête le flux.
fn operation_failure(
    mut steps: Vec<crate::domain::JobStepEntry>,
    step_index: i32,
    node: &FlowNode,
    operation_id: &str,
    message: String,
    excerpt: Option<String>,
) -> FlowExecutionError {
    steps.push(failed_step(
        step_index,
        node,
        Some(operation_id.to_owned()),
        message.clone(),
        excerpt,
    ));
    FlowExecutionError {
        code: forgearr_shared::jobs::JobErrorCode::OperationFailed,
        message,
        steps,
    }
}

fn next_connection<'a>(
    connections: &'a [FlowConnection],
    source_id: &str,
    port: Option<&str>,
) -> Option<&'a FlowConnection> {
    if let Some(port) = port {
        return connections
            .iter()
            .find(|c| c.from == source_id && c.port.as_deref() == Some(port));
    }
    connections
        .iter()
        .find(|c| c.from == source_id && c.port.is_none())
        .or_else(|| connections.iter().find(|c| c.from == source_id))
}
