//! Exécuteur partagé d'un bail de tâche entre worker local et worker distant.

use crate::{
    domain::{Claim, FlowRecord, JobErrorCode, JobStepEntry, JobStepStatus, ToolRequirement},
    flows::{FailureInfo, FlowEngine, FlowProgressCallback, FlowRunInfo, OperationRegistry},
    media::OutputSpec,
    plugins::{self, ProbeContract},
    tools_cache::ToolCacheManager,
};
use forgearr_shared::flows::{LinkedFlowRole, first_linked};
use std::{
    collections::HashMap,
    fmt,
    path::{Path, PathBuf},
};

/// Résultat unifié du traitement d'un fichier sur un worker.
pub struct JobExecutionResult {
    pub working_file: PathBuf,
    pub warning: bool,
    pub output_spec: OutputSpec,
    pub steps: Vec<JobStepEntry>,
    /// Demande de relais à transmettre au central ; aucun résultat n'est alors à publier.
    pub relay: Option<forgearr_shared::relay::RelayRequest>,
}

/// Erreur typée remontée au coordinateur avec la trace d'étapes associée.
#[derive(Debug)]
pub struct JobRunError {
    pub code: JobErrorCode,
    pub message: String,
    pub steps: Vec<JobStepEntry>,
}

impl fmt::Display for JobRunError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for JobRunError {}

impl JobRunError {
    pub fn simple(code: JobErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            steps: Vec::new(),
        }
    }
}

/// Construit le contrat de sonde porté par le bail.
pub fn probe_from_claim(claim: &Claim) -> Option<ProbeContract> {
    match (
        claim.probe_tool_id.as_deref(),
        claim.probe_version.as_deref(),
    ) {
        (Some(tool_id), Some(version)) => Some(ProbeContract {
            tool_id: tool_id.to_owned(),
            version: version.to_owned(),
        }),
        _ => None,
    }
}

/// Liste dédupliquée des outils nécessaires pour exécuter ce bail (`required_tools` ou contrats du bail).
pub fn collect_wanted_tools(claim: &Claim) -> Vec<ToolRequirement> {
    let mut wanted = if !claim.required_tools.is_empty() {
        claim.required_tools.clone()
    } else if !claim.operations.is_empty() {
        let registry =
            OperationRegistry::from_raw(claim.operations.clone(), claim.block_operations.clone());
        registry
            .required_tool_refs_for_flow(claim.frozen_flow.as_ref())
            .into_iter()
            .map(|tool| ToolRequirement {
                tool_id: tool.id,
                version: tool.version,
            })
            .collect()
    } else {
        Vec::new()
    };

    if let Some(probe) = probe_from_claim(claim)
        && !wanted.iter().any(|req| req.tool_id == probe.tool_id)
    {
        wanted.push(ToolRequirement {
            tool_id: probe.tool_id,
            version: probe.version,
        });
    }
    wanted
}

/// Vérifie que la sonde requise pour ce fichier est prête dans le cache du worker.
pub fn ensure_probe_ready(cache: &ToolCacheManager, claim: &Claim) -> Result<(), JobRunError> {
    if let Some(probe) = probe_from_claim(claim) {
        if !plugins::file_job_may_start(cache, Some(&probe)) {
            return Err(JobRunError::simple(
                JobErrorCode::ProbeUnavailable,
                "probe_unavailable",
            ));
        }
        return Ok(());
    }
    if !claim.operations.is_empty() {
        let registry =
            OperationRegistry::from_raw(claim.operations.clone(), claim.block_operations.clone());
        if let Some(probe_op) =
            registry.primary_probe_for_file(Path::new(claim.working_source_name()))
        {
            if let crate::flows::OperationRuntime::Binary(bin) = &probe_op.runtime
                && let Some(tool) = &bin.tool
            {
                let contract = ProbeContract {
                    tool_id: tool.id.clone(),
                    version: tool.version.clone(),
                };
                if !plugins::file_job_may_start(cache, Some(&contract)) {
                    return Err(JobRunError::simple(
                        JobErrorCode::ProbeUnavailable,
                        "probe_unavailable",
                    ));
                }
            }
            return Ok(());
        }
    }
    Err(JobRunError::simple(
        JobErrorCode::ProbeUnavailable,
        "probe_unavailable",
    ))
}

/// Exécute le flow figé du bail ou, à défaut, l'opération `defaultFallback` déclarée dans les contrats.
#[allow(clippy::too_many_arguments)]
pub async fn execute_claim_processing(
    claim: &Claim,
    cache_mgr: &ToolCacheManager,
    http_client: &reqwest::Client,
    secrets_dir: Option<&Path>,
    registry_override: Option<&OperationRegistry>,
    source_snapshot: &Path,
    work_dir: &Path,
    progress_cb: FlowProgressCallback,
) -> Result<JobExecutionResult, JobRunError> {
    let claim_registry;
    let registry = match registry_override {
        Some(reg) if claim.operations.is_empty() => reg,
        _ => {
            claim_registry = OperationRegistry::from_raw(
                claim.operations.clone(),
                claim.block_operations.clone(),
            );
            &claim_registry
        }
    };
    let tool_versions: HashMap<String, String> = claim
        .required_tools
        .iter()
        .map(|req| (req.tool_id.clone(), req.version.clone()))
        .collect();

    let mut worker_capabilities = crate::hardware_probe::detected_capabilities();
    worker_capabilities.software_encoders = crate::software_probe::software_capabilities();
    let engine = FlowEngine {
        registry,
        cache_mgr,
        http_client,
        secrets_dir,
        plugin_configs: &claim.plugin_configs,
        tool_versions: &tool_versions,
        media_root: claim.media_root.as_deref(),
        logical_source_name: Some(&claim.source_name),
        allow_private_integrations: claim.allow_private_integrations,
        worker_capabilities: Some(&worker_capabilities),
        resume_node_id: claim.resume_node_id.as_deref(),
        run_info: FlowRunInfo {
            library_id: claim.library_id,
            library_name: claim.library_name.as_deref(),
            worker_name: claim.worker_name.as_deref(),
            resume_variables: Some(&claim.resume_variables),
            linked_flows: &claim.linked_flows,
            failure: None,
        },
    };

    let outcome = match claim.frozen_flow.as_ref() {
        Some(flow) => {
            engine
                .execute_flow(
                    claim.job_id,
                    flow,
                    source_snapshot,
                    work_dir,
                    progress_cb.clone(),
                )
                .await
        }
        None => {
            engine
                .execute_default(source_snapshot, work_dir, progress_cb.clone())
                .await
        }
    };

    match outcome {
        Ok(res) => Ok(JobExecutionResult {
            working_file: res.final_file,
            warning: res.warning || claim.prior_warning,
            output_spec: res.output_spec,
            steps: res.steps,
            relay: res.relay,
        }),
        Err(error) => {
            let mut failed = JobRunError {
                code: error.code,
                message: error.message,
                steps: error.steps,
            };
            if let Some(failure_flow) = first_linked(&claim.linked_flows, LinkedFlowRole::Failure) {
                let failure_engine = FlowEngine {
                    resume_node_id: None,
                    run_info: FlowRunInfo {
                        resume_variables: None,
                        failure: Some(FailureInfo {
                            code: failed.code.as_str(),
                            message: &failed.message,
                        }),
                        ..engine.run_info
                    },
                    ..engine
                };
                let failure_outcome = failure_engine
                    .execute_flow(
                        claim.job_id,
                        failure_flow,
                        source_snapshot,
                        work_dir,
                        progress_cb,
                    )
                    .await
                    .map(|res| res.steps)
                    .map_err(|err| (err.message, err.steps));
                append_failure_flow_steps(&mut failed.steps, failure_flow, failure_outcome);
            }
            Err(failed)
        }
    }
}

/// Nœud du journal qui résume l'exécution du flux d'échec.
pub const FAILURE_FLOW_STEP_NODE: &str = "failure-flow";

/// Ajoute au journal de la tâche les étapes du flux d'échec et une étape de synthèse.
/// Un échec du flux d'échec est seulement journalisé : aucun autre flux n'est déclenché.
fn append_failure_flow_steps(
    steps: &mut Vec<JobStepEntry>,
    failure_flow: &FlowRecord,
    outcome: Result<Vec<JobStepEntry>, (String, Vec<JobStepEntry>)>,
) {
    let mut next_index = steps.iter().map(|step| step.step_index).max().unwrap_or(0);
    let (status, message, flow_steps) = match outcome {
        Ok(flow_steps) => (
            JobStepStatus::Succeeded,
            format!(
                "flux d'échec « {} » (v{}) exécuté",
                failure_flow.id, failure_flow.version
            ),
            flow_steps,
        ),
        Err((message, flow_steps)) => {
            tracing::warn!(
                flow_id = %failure_flow.id,
                error = %message,
                "le flux d'échec a lui-même échoué"
            );
            (
                JobStepStatus::Failed,
                format!(
                    "flux d'échec « {} » (v{}) en échec : {message}",
                    failure_flow.id, failure_flow.version
                ),
                flow_steps,
            )
        }
    };
    next_index += 1;
    steps.push(JobStepEntry {
        step_index: next_index,
        node_id: FAILURE_FLOW_STEP_NODE.to_owned(),
        block_kind: FAILURE_FLOW_STEP_NODE.to_owned(),
        operation_id: None,
        status,
        message: Some(message),
        output_excerpt: None,
        variables: None,
    });
    for mut step in flow_steps {
        next_index += 1;
        step.step_index = next_index;
        steps.push(step);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use forgearr_shared::flows::FlowRecord;
    use serde_json::json;
    use uuid::Uuid;

    fn sample_claim(flow: Option<FlowRecord>, required_tools: Vec<ToolRequirement>) -> Claim {
        Claim {
            job_id: Uuid::new_v4(),
            lease_token: Uuid::new_v4(),
            lease_epoch: 1,
            source_name: "film.mkv".into(),
            source_sha256: "ab".repeat(32),
            source_bytes: 100,
            library_id: None,
            probe_tool_id: None,
            probe_version: None,
            required_tools,
            frozen_flow: flow,
            operations: HashMap::new(),
            block_operations: HashMap::new(),
            plugin_configs: HashMap::new(),
            sealed_secrets: None,
            allow_private_integrations: None,
            media_root: None,
            local_source_root: None,
            resume_node_id: None,
            handoff_input: None,
            prior_warning: false,
            source_ticket: None,
            library_name: None,
            worker_name: None,
            resume_variables: serde_json::Map::new(),
            linked_flows: Vec::new(),
        }
    }

    #[test]
    fn collect_wanted_tools_uses_claim_requirements_and_adds_probe() {
        let mut claim = sample_claim(
            None,
            vec![ToolRequirement {
                tool_id: "ffmpeg".into(),
                version: "2.0.0".into(),
            }],
        );
        claim.probe_tool_id = Some("mediainfo".into());
        claim.probe_version = Some("1.2.3".into());
        let wanted = collect_wanted_tools(&claim);
        assert_eq!(wanted.len(), 2);
        assert_eq!(wanted[0].tool_id, "ffmpeg");
        assert_eq!(wanted[0].version, "2.0.0");
        assert_eq!(wanted[1].tool_id, "mediainfo");
        assert_eq!(wanted[1].version, "1.2.3");
    }

    #[test]
    fn collect_wanted_tools_extracts_tools_from_claim_operations() {
        let flow = FlowRecord {
            id: "flow.test".into(),
            name: "Test".into(),
            description: None,
            file_mask: "*.mkv".into(),
            version: 1,
            is_default: false,
            definition: json!({
                "nodes": [
                    {"id": "1", "type": "block.custom.encode", "name": "Encode"}
                ],
                "connections": []
            })
            .to_string(),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };
        let mut claim = sample_claim(Some(flow), Vec::new());
        claim.operations.insert(
            "op.probe".into(),
            json!({
                "schemaVersion": "1.1.0",
                "id": "op.probe",
                "version": "1.0.0",
                "name": "Probe",
                "providerId": "plugin.custom",
                "providerKind": "plugin",
                "capability": "probe",
                "status": "validated",
                "runtime": {
                    "kind": "binary",
                    "tool": { "id": "app.probe", "version": "1.0.0" },
                    "executable": "probe",
                    "executionMode": "probe-primary",
                    "argv": [{ "param": "source", "as": "filePath" }]
                }
            }),
        );
        claim.operations.insert(
            "op.custom.encode".into(),
            json!({
                "schemaVersion": "1.1.0",
                "id": "op.custom.encode",
                "version": "1.0.0",
                "name": "Custom",
                "providerId": "plugin.custom",
                "providerKind": "plugin",
                "capability": "encode",
                "status": "validated",
                "runtime": {
                    "kind": "binary",
                    "tool": { "id": "app.custom", "version": "3.1.0" },
                    "executable": "custom",
                    "executionMode": "staged-output",
                    "argv": [{ "param": "source", "as": "filePath" }]
                }
            }),
        );
        claim
            .block_operations
            .insert("block.custom.encode".into(), "op.custom.encode".into());
        let wanted = collect_wanted_tools(&claim);
        let ids: Vec<_> = wanted.into_iter().map(|req| req.tool_id).collect();
        assert_eq!(ids, vec!["app.probe".to_owned(), "app.custom".to_owned()]);
    }

    #[test]
    fn ensure_probe_ready_resolves_per_file_extension() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let cache = ToolCacheManager::new(dir.path());
        let epub_dir = cache.tool_dir("app.calibre", "7.0.0", crate::tools_cache::host_arch());
        std::fs::create_dir_all(&epub_dir)?;
        std::fs::write(epub_dir.join(".ready"), b"")?;

        let mut claim = sample_claim(None, Vec::new());
        claim.source_name = "roman.epub".into();
        claim.operations.insert(
            "op.ebook.probe".into(),
            json!({
                "schemaVersion": "1.1.0",
                "id": "op.ebook.probe",
                "version": "1.0.0",
                "name": "EbookProbe",
                "providerId": "plugin.calibre",
                "providerKind": "plugin",
                "capability": "probe",
                "status": "validated",
                "runtime": {
                    "kind": "binary",
                    "tool": { "id": "app.calibre", "version": "7.0.0" },
                    "executable": "ebook-meta",
                    "executionMode": "probe-primary",
                    "probeMatch": { "extensions": ["epub"] },
                    "argv": [{ "param": "source", "as": "filePath" }]
                }
            }),
        );
        ensure_probe_ready(&cache, &claim)?;

        claim.source_name = "autre.pdf".into();
        assert!(matches!(
            ensure_probe_ready(&cache, &claim),
            Err(JobRunError {
                code: JobErrorCode::ProbeUnavailable,
                ..
            })
        ));
        claim.handoff_input = Some(forgearr_shared::relay::HandoffInput {
            source_lease_epoch: 1,
            artifact: forgearr_shared::relay::HandoffArtifact {
                name: "converti.epub".into(),
                sha256: "ab".repeat(32),
                bytes: 100,
            },
        });
        ensure_probe_ready(&cache, &claim)?;
        Ok(())
    }

    fn step(index: i32, node: &str, status: JobStepStatus) -> JobStepEntry {
        JobStepEntry {
            step_index: index,
            node_id: node.into(),
            block_kind: node.into(),
            operation_id: None,
            status,
            message: None,
            output_excerpt: None,
            variables: None,
        }
    }

    fn failure_flow() -> FlowRecord {
        FlowRecord {
            id: "flow.echec".into(),
            name: "Échec".into(),
            description: None,
            file_mask: "*".into(),
            version: 2,
            is_default: false,
            definition: "{}".into(),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        }
    }

    #[test]
    fn failure_flow_steps_follow_the_failed_steps() {
        let mut steps = vec![
            step(1, "in", JobStepStatus::Succeeded),
            step(2, "encode", JobStepStatus::Failed),
        ];
        let flow_steps = vec![step(1, "in", JobStepStatus::Succeeded)];
        append_failure_flow_steps(&mut steps, &failure_flow(), Ok(flow_steps));
        let indexes: Vec<i32> = steps.iter().map(|entry| entry.step_index).collect();
        assert_eq!(indexes, vec![1, 2, 3, 4]);
        assert_eq!(steps[2].node_id, FAILURE_FLOW_STEP_NODE);
        assert_eq!(steps[2].status, JobStepStatus::Succeeded);
        assert_eq!(
            steps[2].message.as_deref(),
            Some("flux d'échec « flow.echec » (v2) exécuté")
        );
    }

    #[test]
    fn a_failing_failure_flow_is_only_logged() {
        let mut steps = vec![step(1, "encode", JobStepStatus::Failed)];
        let flow_steps = vec![step(1, "hook", JobStepStatus::Failed)];
        let outcome = Err(("webhook refusé".to_owned(), flow_steps));
        append_failure_flow_steps(&mut steps, &failure_flow(), outcome);
        assert_eq!(steps.len(), 3);
        assert_eq!(steps[1].status, JobStepStatus::Failed);
        assert!(
            steps[1]
                .message
                .as_deref()
                .is_some_and(|message| message.contains("webhook refusé"))
        );
    }
}
