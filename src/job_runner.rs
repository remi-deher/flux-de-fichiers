//! Exécuteur partagé d'un bail de tâche entre worker local et worker distant.

use crate::{
    domain::{Claim, JobErrorCode, JobStepEntry, JobStepStatus, ToolRequirement},
    flows::{self, ArrContext, FlowEngine, FlowProgressCallback},
    media::{self, OutputProfile},
    plugins::{self, ProbeContract, default_tool_version},
    tools_cache::ToolCacheManager,
};
use std::{
    fmt,
    path::{Path, PathBuf},
    sync::Arc,
};

/// Résultat unifié du traitement d'un fichier sur un worker.
pub struct JobExecutionResult {
    pub working_file: PathBuf,
    pub warning: bool,
    pub output_profile: OutputProfile,
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

/// Liste dédupliquée des outils nécessaires pour exécuter ce bail (`required_tools` ou déduction du flow).
pub fn collect_wanted_tools(claim: &Claim) -> Vec<ToolRequirement> {
    let mut wanted = if !claim.required_tools.is_empty() {
        claim.required_tools.clone()
    } else if let Some(flow) = claim.frozen_flow.as_ref() {
        flows::required_tool_ids(&flow.definition)
            .into_iter()
            .map(|tool_id| ToolRequirement {
                version: default_tool_version(tool_id).to_owned(),
                tool_id: tool_id.to_owned(),
            })
            .collect()
    } else {
        vec![ToolRequirement {
            tool_id: plugins::REMUX_TOOL_ID.to_owned(),
            version: plugins::REMUX_TOOL_VERSION.to_owned(),
        }]
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

/// Version de l'outil de remuxage annoncée dans le bail (ou version par défaut).
pub fn remux_tool_version(claim: &Claim) -> &str {
    claim
        .required_tools
        .iter()
        .find(|req| req.tool_id == plugins::REMUX_TOOL_ID)
        .map(|req| req.version.as_str())
        .unwrap_or(plugins::REMUX_TOOL_VERSION)
}

/// Vérifie que la sonde requise par le catalogue est prête sur le worker.
pub fn ensure_probe_ready(cache: &ToolCacheManager, claim: &Claim) -> Result<(), JobRunError> {
    let probe = probe_from_claim(claim);
    if !plugins::file_job_may_start(cache, probe.as_ref()) {
        return Err(JobRunError::simple(
            JobErrorCode::ProbeUnavailable,
            "probe_unavailable",
        ));
    }
    Ok(())
}

/// Exécute le flow figé du bail ou, à défaut, le remuxage MKV standard.
#[allow(clippy::too_many_arguments)]
pub async fn execute_claim_processing(
    claim: &Claim,
    cache_mgr: &ToolCacheManager,
    http_client: &reqwest::Client,
    secrets_dir: Option<&Path>,
    arr: Option<&ArrContext>,
    source_snapshot: &Path,
    work_dir: &Path,
    progress_cb: FlowProgressCallback,
) -> Result<JobExecutionResult, JobRunError> {
    if let Some(flow) = claim.frozen_flow.as_ref() {
        let mut engine = FlowEngine::new(cache_mgr, http_client);
        if let Some(dir) = secrets_dir {
            engine = engine.with_secrets_dir(dir);
        }
        if let Some(arr_ctx) = arr {
            engine = engine.with_arr(arr_ctx);
        }
        return engine
            .execute_flow(flow, claim.job_id, source_snapshot, work_dir, progress_cb)
            .await
            .map(|outcome| JobExecutionResult {
                working_file: outcome.working_file,
                warning: outcome.warning,
                output_profile: outcome.output_profile,
                steps: outcome.steps,
            })
            .map_err(|error| JobRunError {
                code: error.code,
                message: error.message,
                steps: error.steps,
            });
    }

    let output = work_dir.join("output.mkv");
    let mkvmerge_bin = cache_mgr.resolve_executable(
        plugins::REMUX_TOOL_ID,
        remux_tool_version(claim),
        "mkvmerge",
        "MKVMERGE",
    );
    let cb = Arc::clone(&progress_cb);
    match media::remux_with_progress(&mkvmerge_bin, source_snapshot, &output, move |pct, line| {
        cb(pct, line);
    })
    .await
    {
        Ok(warning) => Ok(JobExecutionResult {
            working_file: output,
            warning,
            output_profile: OutputProfile::StrictMkvRemux,
            steps: vec![JobStepEntry {
                step_index: 0,
                node_id: "default-remux".into(),
                block_kind: "remux-mkv".into(),
                operation_id: Some("mkvtoolnix.remux".into()),
                status: JobStepStatus::Succeeded,
                message: Some(if warning {
                    "Remux MKV terminé avec avertissement".into()
                } else {
                    "Remux MKV terminé".into()
                }),
                output_excerpt: None,
            }],
        }),
        Err(error) => Err(JobRunError {
            code: JobErrorCode::RemuxFailed,
            message: format!("remux_failed: {error}"),
            steps: vec![JobStepEntry {
                step_index: 0,
                node_id: "default-remux".into(),
                block_kind: "remux-mkv".into(),
                operation_id: Some("mkvtoolnix.remux".into()),
                status: JobStepStatus::Failed,
                message: Some(error.to_string()),
                output_excerpt: None,
            }],
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use flux_shared::flows::FlowRecord;
    use uuid::Uuid;

    fn sample_claim(flow: Option<FlowRecord>, required_tools: Vec<ToolRequirement>) -> Claim {
        Claim {
            job_id: Uuid::new_v4(),
            lease_token: Uuid::new_v4(),
            source_name: "film.mkv".into(),
            source_sha256: "ab".repeat(32),
            source_bytes: 100,
            library_id: None,
            probe_tool_id: None,
            probe_version: None,
            required_tools,
            frozen_flow: flow,
            arr: None,
        }
    }

    #[test]
    fn collect_wanted_tools_uses_claim_requirements_and_adds_probe() {
        let mut claim = sample_claim(
            None,
            vec![ToolRequirement {
                tool_id: plugins::FFMPEG_TOOL_ID.into(),
                version: "2.0.0".into(),
            }],
        );
        claim.probe_tool_id = Some(plugins::MEDIAINFO_TOOL_ID.into());
        claim.probe_version = Some("1.2.3".into());
        let wanted = collect_wanted_tools(&claim);
        assert_eq!(wanted.len(), 2);
        assert_eq!(wanted[0].tool_id, plugins::FFMPEG_TOOL_ID);
        assert_eq!(wanted[0].version, "2.0.0");
        assert_eq!(wanted[1].tool_id, plugins::MEDIAINFO_TOOL_ID);
        assert_eq!(wanted[1].version, "1.2.3");
    }

    #[test]
    fn collect_wanted_tools_infers_flow_tools_when_server_omits_list() {
        let flow = FlowRecord {
            id: "flow.test".into(),
            name: "Test".into(),
            description: None,
            file_mask: "*.mkv".into(),
            version: 1,
            is_default: false,
            definition: serde_json::json!({
                "nodes": [
                    {"id": "1", "type": "block.video.encode", "name": "Encode"},
                    {"id": "2", "type": "block.video.remux-mp4", "name": "MP4"}
                ],
                "connections": []
            })
            .to_string(),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };
        let wanted = collect_wanted_tools(&sample_claim(Some(flow), Vec::new()));
        let ids: Vec<_> = wanted.into_iter().map(|req| req.tool_id).collect();
        assert_eq!(
            ids,
            vec![
                plugins::REMUX_TOOL_ID.to_owned(),
                plugins::FFMPEG_TOOL_ID.to_owned(),
                plugins::MP4BOX_TOOL_ID.to_owned()
            ]
        );
    }
}
