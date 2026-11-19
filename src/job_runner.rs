//! Exécuteur partagé d'un bail de tâche entre worker local et worker distant.

use crate::{
    domain::{Claim, JobErrorCode, JobStepEntry, ToolRequirement},
    flows::{FlowEngine, FlowProgressCallback, OperationRegistry},
    media::OutputSpec,
    plugins::{self, ProbeContract},
    tools_cache::ToolCacheManager,
};
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
        if let Some(probe_op) = registry.primary_probe_for_file(Path::new(&claim.source_name)) {
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

    let engine = FlowEngine {
        registry,
        cache_mgr,
        http_client,
        secrets_dir,
        plugin_configs: &claim.plugin_configs,
        tool_versions: &tool_versions,
        media_root: claim.media_root.as_deref(),
        allow_private_integrations: claim.allow_private_integrations,
    };

    let outcome = match claim.frozen_flow.as_ref() {
        Some(flow) => {
            engine
                .execute_flow(claim.job_id, flow, source_snapshot, work_dir, progress_cb)
                .await
        }
        None => {
            engine
                .execute_default(source_snapshot, work_dir, progress_cb)
                .await
        }
    };

    outcome
        .map(|res| JobExecutionResult {
            working_file: res.final_file,
            warning: res.warning,
            output_spec: res.output_spec,
            steps: res.steps,
        })
        .map_err(|error| JobRunError {
            code: error.code,
            message: error.message,
            steps: error.steps,
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use flux_shared::flows::FlowRecord;
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
        Ok(())
    }
}
