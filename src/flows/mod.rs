//! Moteur d'exécution des flux et blocs d'action composables (ADR 0008, ADR 0009).

mod arr;
mod block;
mod condition;
mod custom_args;
mod encode;
mod encode_run;
mod header_secrets;
mod lease_secrets;
mod mp4box;
mod route;
mod tool;
mod webhook;

pub use arr::{
    ArrAction, ArrBlock, ArrConnection, ArrContext, ArrLeaseContext, ArrService, execute_arr_block,
};
pub use block::{
    BlockOperation, FlowBlockKind, StepDecision, decide_step, decide_step_for_operation,
    resolve_operation,
};
pub use condition::{ConditionConfig, evaluate_condition};
pub use custom_args::{CustomArgsError, ffmpeg_custom_args, mp4box_custom_args, parse_custom_args};
pub use encode::{EncodePlan, argv_for, audio_plan, video_plan};
pub use header_secrets::{
    decrypt_definition_headers, decrypt_headers, has_webhook_headers, mask_definition,
    migrate_flow_secrets, rotate_definition, seal_definition,
};
pub use lease_secrets::{arr_services_in, lease_arr_context};
pub use mp4box::{Mp4Plan, mp4_plan, mp4box_argv};
pub use route::next_connection;
pub use tool::{InPlaceEdit, in_place_edit};
pub use webhook::{WebhookBlockConfig, execute_webhook_block, webhook_method};

use crate::{
    domain::{FlowRecord, JobErrorCode, JobStepEntry, JobStepStatus},
    media::{self, OutputProfile},
    tool_run::ToolRunError,
    tools_cache::ToolCacheManager,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};
use uuid::Uuid;

/// Contexte d'exécution instantané transmis d'étape en étape à travers le flux.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FlowContext {
    pub job_id: Uuid,
    pub source_name: String,
    pub working_file: PathBuf,
    pub media_info: serde_json::Value,
    pub status: crate::domain::JobStatus,
    pub current_step: String,
    pub timestamp: DateTime<Utc>,
    #[serde(default)]
    pub variables: HashMap<String, serde_json::Value>,
}

impl FlowContext {
    pub fn new(
        job_id: Uuid,
        source_name: String,
        working_file: PathBuf,
        media_info: serde_json::Value,
    ) -> Self {
        Self {
            job_id,
            source_name,
            working_file,
            media_info,
            status: crate::domain::JobStatus::Running,
            current_step: "init".into(),
            timestamp: Utc::now(),
            variables: HashMap::new(),
        }
    }

    pub fn with_step(&self, step_name: &str) -> Self {
        let mut cloned = self.clone();
        cloned.current_step = step_name.into();
        cloned.timestamp = Utc::now();
        cloned
    }
}

pub use flux_shared::flows::{FlowConnection, FlowGraph, FlowNode};

/// Résultat final de l'exécution d'un flux.
#[derive(Clone, Debug)]
pub struct FlowExecutionOutcome {
    pub working_file: PathBuf,
    pub in_place: bool,
    pub warning: bool,
    pub output_profile: OutputProfile,
    pub steps: Vec<JobStepEntry>,
}

/// Échec typé d'une exécution de flux, portant le code d'erreur métier et les étapes déjà exécutées.
#[derive(Debug, thiserror::Error)]
#[error("{message}")]
pub struct FlowExecutionError {
    pub code: JobErrorCode,
    pub message: String,
    pub steps: Vec<JobStepEntry>,
}

/// Identifiants d'outils distribués nécessaires pour exécuter `definition` (inclut toujours `mkvmerge`).
pub fn required_tool_ids(definition: &str) -> Vec<&'static str> {
    let mut ids = vec![crate::plugins::REMUX_TOOL_ID];
    if let Ok(graph) = serde_json::from_str::<FlowGraph>(definition) {
        for node in &graph.nodes {
            let op_id = node.extra.get("operationId").and_then(|v| v.as_str());
            if let Some(tool_id) = resolve_operation(op_id, &node.kind).required_tool_id()
                && !ids.contains(&tool_id)
            {
                ids.push(tool_id);
            }
        }
    }
    ids
}

/// Détermine le profil de validation attendu côté central pour un flux figé et une extension produite.
pub fn output_profile_for_flow(
    flow: Option<&FlowRecord>,
    requested_ext: Option<&str>,
) -> Option<OutputProfile> {
    let Some(flow) = flow else {
        return match requested_ext {
            None | Some("mkv") => Some(OutputProfile::StrictMkvRemux),
            _ => None,
        };
    };
    let Ok(graph) = serde_json::from_str::<FlowGraph>(&flow.definition) else {
        return Some(OutputProfile::StrictMkvRemux);
    };
    let ops: Vec<BlockOperation> = graph
        .nodes
        .iter()
        .map(|node| {
            let op_id = node.extra.get("operationId").and_then(|v| v.as_str());
            resolve_operation(op_id, &node.kind)
        })
        .collect();
    if requested_ext == Some("mp4") {
        return ops
            .contains(&BlockOperation::RemuxMp4)
            .then_some(OutputProfile::Mp4Container);
    }
    if !matches!(requested_ext, None | Some("mkv")) {
        return None;
    }
    if ops.contains(&BlockOperation::VideoEncode) || ops.contains(&BlockOperation::AudioEncode) {
        Some(OutputProfile::TranscodedMkv)
    } else if ops.contains(&BlockOperation::RemuxMkv) {
        Some(OutputProfile::StrictMkvRemux)
    } else if ops.contains(&BlockOperation::Mkvpropedit) {
        Some(OutputProfile::MkvMetadataEdit)
    } else if ops.contains(&BlockOperation::RemuxMp4) && requested_ext.is_none() {
        Some(OutputProfile::Mp4Container)
    } else {
        Some(OutputProfile::StrictMkvRemux)
    }
}

/// Moteur d'interprétation et d'exécution pas à pas des flux (Pipeline Engine).
pub struct FlowEngine<'a> {
    pub cache_mgr: &'a ToolCacheManager,
    pub http_client: &'a reqwest::Client,
    /// Dossier de configuration qui porte la clé de chiffrement des en-têtes de webhook.
    pub secrets_dir: Option<&'a Path>,
    /// Réglages des plugins Sonarr et Radarr, pour les blocs d'intégration.
    pub arr: Option<&'a ArrContext>,
}

pub type FlowProgressCallback = std::sync::Arc<dyn Fn(u32, &str) + Send + Sync>;

impl<'a> FlowEngine<'a> {
    pub fn new(cache_mgr: &'a ToolCacheManager, http_client: &'a reqwest::Client) -> Self {
        Self {
            cache_mgr,
            http_client,
            secrets_dir: None,
            arr: None,
        }
    }

    /// Donne aux blocs Sonarr et Radarr les réglages de leurs plugins.
    pub fn with_arr(mut self, arr: &'a ArrContext) -> Self {
        self.arr = Some(arr);
        self
    }

    /// Autorise le déchiffrement des en-têtes de webhook avec la clé de ce dossier de configuration.
    pub fn with_secrets_dir(mut self, secrets_dir: &'a Path) -> Self {
        self.secrets_dir = Some(secrets_dir);
        self
    }

    /// Exécute un flux complet sur un fichier source donné.
    pub async fn execute_flow(
        &self,
        flow: &FlowRecord,
        job_id: Uuid,
        source_file: &Path,
        work_dir: &Path,
        on_progress: FlowProgressCallback,
    ) -> Result<FlowExecutionOutcome, FlowExecutionError> {
        tracing::info!(job_id = %job_id, flow_id = %flow.id, flow_name = %flow.name, "starting pipeline engine execution");
        let graph: FlowGraph =
            serde_json::from_str(&flow.definition).map_err(|err| FlowExecutionError {
                code: JobErrorCode::RemuxFailed,
                message: format!("définition de flux invalide : {err}"),
                steps: Vec::new(),
            })?;
        let mkvmerge_bin = self.cache_mgr.resolve_executable(
            crate::plugins::REMUX_TOOL_ID,
            crate::plugins::REMUX_TOOL_VERSION,
            "mkvmerge",
            "MKVMERGE",
        );
        let mut media_info = media::inspect_with(&mkvmerge_bin, source_file)
            .await
            .map_err(|err| FlowExecutionError {
                code: JobErrorCode::RemuxFailed,
                message: format!("inspection initiale impossible : {err}"),
                steps: Vec::new(),
            })?;
        media_info = self.maybe_enrich_mediainfo(media_info, source_file).await;
        let mut current_working_file = source_file.to_path_buf();
        let mut is_in_place = true;
        let mut final_warning = false;
        let mut output_profile = OutputProfile::StrictMkvRemux;
        let mut has_remux_mkv = false;
        let mut has_encode = false;
        let mut has_edit = false;
        let mut steps: Vec<JobStepEntry> = Vec::new();
        let mut ctx = FlowContext::new(
            job_id,
            source_file
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| "source.mkv".into()),
            current_working_file.clone(),
            media_info,
        );
        let input_node = graph
            .nodes
            .iter()
            .find(|node| block::is_entry_block(&node.kind))
            .cloned()
            .unwrap_or_else(|| FlowNode {
                id: "node-1".into(),
                kind: FlowBlockKind::parse("block.core.file-input"),
                name: "Fichier source".into(),
                config: serde_json::Value::Null,
                extra: serde_json::Map::new(),
            });
        let mut current_node_id = input_node.id;
        let mut max_steps = 32;
        let mut step_index: i32 = 0;
        while max_steps > 0 {
            max_steps -= 1;
            let Some(node) = graph
                .nodes
                .iter()
                .find(|candidate| candidate.id == current_node_id)
                .cloned()
            else {
                break;
            };
            step_index += 1;
            let explicit_op = node.extra.get("operationId").and_then(|v| v.as_str());
            let resolved_op = resolve_operation(explicit_op, &node.kind);
            let op_id = explicit_op
                .map(str::to_owned)
                .unwrap_or_else(|| resolved_op.operation_id().to_owned());
            let step_ctx = ctx.with_step(&node.name);
            let decision = decide_step_for_operation(
                explicit_op,
                &node.kind,
                &node.config,
                &step_ctx.media_info,
            );
            tracing::info!(job_id = %job_id, block = %node.kind, operation_id = %op_id, "flow step");
            let mut step_message: Option<String> = decision
                .port
                .as_ref()
                .map(|port| format!("branche -> {port}"));
            let mut step_excerpt: Option<String> = None;
            let mut step_status = JobStepStatus::Succeeded;

            let fail_step = |steps: &mut Vec<JobStepEntry>,
                             code: JobErrorCode,
                             message: String,
                             excerpt: Option<String>| {
                steps.push(JobStepEntry {
                    step_index,
                    node_id: node.id.clone(),
                    block_kind: node.kind.as_str().to_owned(),
                    operation_id: Some(op_id.clone()),
                    status: JobStepStatus::Failed,
                    message: Some(message.clone()),
                    output_excerpt: excerpt,
                });
                FlowExecutionError {
                    code,
                    message,
                    steps: std::mem::take(steps),
                }
            };

            if let Some(edit) = &decision.edit {
                has_edit = true;
                let propedit_bin = self.cache_mgr.resolve_executable(
                    crate::plugins::REMUX_TOOL_ID,
                    crate::plugins::REMUX_TOOL_VERSION,
                    "mkvpropedit",
                    "MKVPROPEDIT",
                );
                if let Err(err) = media::propedit_in_place(
                    &propedit_bin,
                    &current_working_file,
                    edit.default_audio_lang.as_deref(),
                    edit.force_subtitles,
                )
                .await
                {
                    return Err(fail_step(
                        &mut steps,
                        JobErrorCode::RemuxFailed,
                        format!("mkvpropedit : {err}"),
                        None,
                    ));
                }
                step_message = Some("édition en place des en-têtes Matroska".into());
                tracing::info!(job_id = %job_id, "in-place mkvpropedit completed directly on working file");
            }
            if decision.reinpect {
                let refreshed =
                    match media::inspect_with(&mkvmerge_bin, &current_working_file).await {
                        Ok(info) => {
                            self.maybe_enrich_mediainfo(info, &current_working_file)
                                .await
                        }
                        Err(err) => {
                            return Err(fail_step(
                                &mut steps,
                                JobErrorCode::RemuxFailed,
                                format!("ré-inspection média : {err}"),
                                None,
                            ));
                        }
                    };
                ctx.media_info = refreshed;
                step_message = Some("métadonnées techniques actualisées".into());
                tracing::info!(job_id = %job_id, "media inspect refreshed for working file");
            }
            if decision.transform {
                is_in_place = false;
                has_remux_mkv = true;
                output_profile = OutputProfile::StrictMkvRemux;
                let target_output = work_dir.join("output.mkv");
                let on_p = on_progress.clone();
                match media::remux_with_progress(
                    &mkvmerge_bin,
                    &current_working_file,
                    &target_output,
                    move |pct: u32, line: &str| {
                        on_p(pct, line);
                    },
                )
                .await
                {
                    Ok(warn) => {
                        final_warning = warn;
                        current_working_file = target_output;
                        ctx.working_file = current_working_file.clone();
                        step_message = Some(if warn {
                            "remux MKV terminé avec avertissements".into()
                        } else {
                            "remux MKV sans perte".into()
                        });
                    }
                    Err(err) => {
                        return Err(fail_step(
                            &mut steps,
                            JobErrorCode::RemuxFailed,
                            format!("remux MKV : {err}"),
                            None,
                        ));
                    }
                }
                tracing::info!(job_id = %job_id, "remux step executed into staging working file");
            }
            if let Some(plan) = &decision.remux_mp4 {
                if let Err(err) = mp4box_custom_args(&node.config) {
                    return Err(fail_step(
                        &mut steps,
                        JobErrorCode::RemuxFailed,
                        format!("paramètres MP4Box invalides : {err}"),
                        None,
                    ));
                }
                is_in_place = false;
                output_profile = OutputProfile::Mp4Container;
                let mp4box_bin = self.cache_mgr.resolve_executable(
                    crate::plugins::MP4BOX_TOOL_ID,
                    crate::plugins::MP4BOX_TOOL_VERSION,
                    "MP4Box",
                    "MP4BOX",
                );
                let target_output = work_dir.join("remuxed.mp4");
                let args = mp4box_argv(plan, &current_working_file, &target_output);
                let on_p = on_progress.clone();
                match media::mp4box_with_progress(
                    &mp4box_bin,
                    &args,
                    move |pct: u32, line: &str| {
                        on_p(pct, line);
                    },
                )
                .await
                {
                    Ok(outcome) => {
                        step_excerpt = outcome.excerpt;
                        current_working_file = target_output;
                        ctx.working_file = current_working_file.clone();
                        step_message = Some("remuxage MP4 terminé".into());
                    }
                    Err(err) => {
                        let excerpt = match &err {
                            ToolRunError::Failed { excerpt, .. } => excerpt.clone(),
                            _ => None,
                        };
                        return Err(fail_step(
                            &mut steps,
                            JobErrorCode::RemuxFailed,
                            format!("MP4Box : {err}"),
                            excerpt,
                        ));
                    }
                }
                tracing::info!(job_id = %job_id, "mp4 remux step completed");
            }
            if let Some(plan) = &decision.encode {
                if let Err(err) = ffmpeg_custom_args(&node.config) {
                    return Err(fail_step(
                        &mut steps,
                        JobErrorCode::EncodeFailed,
                        format!("paramètres FFmpeg invalides : {err}"),
                        None,
                    ));
                }
                is_in_place = false;
                has_encode = true;
                output_profile = OutputProfile::TranscodedMkv;
                let ffmpeg_bin = self.cache_mgr.resolve_executable(
                    crate::plugins::FFMPEG_TOOL_ID,
                    crate::plugins::FFMPEG_TOOL_VERSION,
                    "ffmpeg",
                    "FFMPEG",
                );
                let target_output = work_dir.join(match plan {
                    EncodePlan::Video(_) => "encoded.mkv",
                    EncodePlan::Audio(_) => "encoded-audio.mkv",
                });
                let duration_secs = media::media_duration_secs(&ctx.media_info);
                match encode_run::run_encode(
                    &ffmpeg_bin,
                    plan,
                    &current_working_file,
                    &target_output,
                    duration_secs,
                    &on_progress,
                    job_id,
                )
                .await
                {
                    Ok(encode_outcome) => {
                        step_excerpt = encode_outcome.excerpt;
                        step_message = Some(if encode_outcome.fallback_to_cpu {
                            "encodage terminé (repli matériel -> CPU)".into()
                        } else {
                            "encodage terminé".into()
                        });
                        current_working_file = target_output;
                        ctx.working_file = current_working_file.clone();
                    }
                    Err(err) => {
                        let excerpt = match &err {
                            ToolRunError::Failed { excerpt, .. } => excerpt.clone(),
                            _ => None,
                        };
                        return Err(fail_step(
                            &mut steps,
                            JobErrorCode::EncodeFailed,
                            format!("encodage FFmpeg : {err}"),
                            excerpt,
                        ));
                    }
                }
                tracing::info!(job_id = %job_id, block = %node.kind, "plugin encode step completed");
            }
            if let Some(webhook_cfg) = &decision.notify {
                let mut webhook_cfg = webhook_cfg.clone();
                if let Some(dir) = self.secrets_dir {
                    let secrets = crate::secrets::SecretBox::open(dir).map_err(|err| {
                        fail_step(
                            &mut steps,
                            JobErrorCode::WebhookFailed,
                            format!("clé de chiffrement webhook : {err}"),
                            None,
                        )
                    })?;
                    if let Err(err) =
                        decrypt_headers(&secrets, &flow.id, &node.id, &mut webhook_cfg)
                    {
                        return Err(fail_step(
                            &mut steps,
                            JobErrorCode::WebhookFailed,
                            format!("déchiffrement en-têtes webhook : {err}"),
                            None,
                        ));
                    }
                }
                if let Err(err) =
                    execute_webhook_block(&webhook_cfg, &step_ctx, self.http_client).await
                {
                    return Err(fail_step(
                        &mut steps,
                        JobErrorCode::WebhookFailed,
                        format!("webhook : {err}"),
                        None,
                    ));
                }
                step_message = Some("notification webhook délivrée".into());
            }
            if let Some(block) = &decision.integration {
                let outcome = match self.arr {
                    Some(arr) => {
                        arr.run(block, &step_ctx.source_name, self.http_client)
                            .await
                    }
                    None => Err(anyhow::anyhow!("réglages des plugins indisponibles")),
                };
                if let Err(error) = outcome {
                    if block.fail_on_error {
                        return Err(fail_step(
                            &mut steps,
                            JobErrorCode::IntegrationFailed,
                            format!("intégration *arr : {error}"),
                            None,
                        ));
                    }
                    step_status = JobStepStatus::Skipped;
                    step_message = Some(format!("échec non bloquant : {error}"));
                    tracing::warn!(job_id = %job_id, service = ?block.service, error = %error, "bloc d'intégration en échec, flux poursuivi");
                } else {
                    step_message = Some("commande *arr transmise".into());
                }
            }
            steps.push(JobStepEntry {
                step_index,
                node_id: node.id.clone(),
                block_kind: node.kind.as_str().to_owned(),
                operation_id: Some(op_id),
                status: step_status,
                message: step_message,
                output_excerpt: step_excerpt,
            });
            if decision.stop {
                tracing::info!(job_id = %job_id, "reached final file-output node, terminating pipeline");
                break;
            }
            let Some(connection) = next_connection(
                &graph.connections,
                &current_node_id,
                decision.port.as_deref(),
            ) else {
                break;
            };
            current_node_id = connection.to.clone();
        }
        if has_edit
            && !has_remux_mkv
            && !has_encode
            && output_profile == OutputProfile::StrictMkvRemux
        {
            output_profile = OutputProfile::MkvMetadataEdit;
        }
        Ok(FlowExecutionOutcome {
            working_file: current_working_file,
            in_place: is_in_place,
            warning: final_warning,
            output_profile,
            steps,
        })
    }

    async fn maybe_enrich_mediainfo(
        &self,
        mut media_info: serde_json::Value,
        path: &Path,
    ) -> serde_json::Value {
        let arch = crate::tools_cache::host_arch();
        if let Ok(mediainfo_bin) = self.cache_mgr.verify_cached_binary(
            crate::plugins::MEDIAINFO_TOOL_ID,
            crate::plugins::MEDIAINFO_TOOL_VERSION,
            arch,
            "mediainfo",
            None,
        ) && let Ok(parsed) = media::inspect_mediainfo(&mediainfo_bin, path).await
        {
            media_info = media::attach_mediainfo(media_info, parsed);
        }
        media_info
    }
}

#[cfg(test)]
mod engine_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::JobStatus;

    #[test]
    fn flow_context_starts_running_and_with_step_renames_the_step() {
        let ctx = FlowContext::new(
            Uuid::nil(),
            "film.mkv".into(),
            PathBuf::from("film.mkv"),
            serde_json::json!({}),
        );
        assert_eq!(ctx.status, JobStatus::Running);
        assert_eq!(ctx.current_step, "init");
        assert_eq!(ctx.with_step("Remux").current_step, "Remux");
    }

    #[test]
    fn flow_graph_keeps_block_types_on_the_wire() -> Result<(), serde_json::Error> {
        let graph: FlowGraph = serde_json::from_str(
            r#"{"nodes":[{"id":"node-1","type":"file-input","name":"Source"},{"id":"node-2","type":"block.video.remux-mp4","name":"MP4"},{"id":"node-3","type":"legacy-unknown","name":"X"}],"connections":[{"from":"node-1","to":"node-2"}]}"#,
        )?;
        assert_eq!(graph.nodes[0].kind, FlowBlockKind::parse("file-input"));
        assert_eq!(
            graph.nodes[1].kind,
            FlowBlockKind::parse("block.video.remux-mp4")
        );
        assert_eq!(graph.nodes[2].kind, FlowBlockKind::parse("legacy-unknown"));
        let encoded = serde_json::to_value(&graph)?;
        assert_eq!(encoded["nodes"][1]["type"], "block.video.remux-mp4");
        Ok(())
    }
}
