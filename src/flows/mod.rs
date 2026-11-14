//! Moteur d'exécution des flux et blocs d'action composables (ADR 0008, ADR 0009).

mod arr;
mod block;
mod condition;
mod encode;
mod header_secrets;
mod route;
mod tool;
mod webhook;

pub use arr::{ArrAction, ArrBlock, ArrConnection, ArrContext, ArrService, execute_arr_block};
pub use block::{FlowBlockKind, StepDecision, decide_step};
pub use condition::{ConditionConfig, evaluate_condition};
pub use encode::{EncodePlan, argv_for, audio_plan, video_plan};
pub use header_secrets::{
    decrypt_headers, has_webhook_headers, mask_definition, migrate_flow_secrets, rotate_definition,
    seal_definition,
};
pub use route::next_connection;
pub use tool::{InPlaceEdit, in_place_edit};
pub use webhook::{WebhookBlockConfig, execute_webhook_block, webhook_method};

use crate::{domain::FlowRecord, media, tools_cache::ToolCacheManager};
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
    ) -> anyhow::Result<FlowExecutionOutcome> {
        tracing::info!(job_id = %job_id, flow_id = %flow.id, flow_name = %flow.name, "starting pipeline engine execution");
        let graph: FlowGraph = serde_json::from_str(&flow.definition)?;
        let mkvmerge_bin = self.cache_mgr.resolve_executable(
            crate::plugins::REMUX_TOOL_ID,
            crate::plugins::REMUX_TOOL_VERSION,
            "mkvmerge",
            "MKVMERGE",
        );
        let media_info = media::inspect_with(&mkvmerge_bin, source_file).await?;
        let mut current_working_file = source_file.to_path_buf();
        let mut is_in_place = true;
        let mut final_warning = false;
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
            .find(|node| node.kind == FlowBlockKind::FileInput)
            .cloned()
            .unwrap_or_else(|| FlowNode {
                id: "node-1".into(),
                kind: FlowBlockKind::FileInput,
                name: "Fichier source".into(),
                config: serde_json::Value::Null,
                extra: serde_json::Map::new(),
            });
        let mut current_node_id = input_node.id;
        let mut max_steps = 32;
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
            let step_ctx = ctx.with_step(&node.name);
            let decision = decide_step(&node.kind, &node.config, &step_ctx.media_info);
            tracing::info!(job_id = %job_id, block = %node.kind, "flow step");
            if let Some(edit) = &decision.edit {
                let propedit_bin = self.cache_mgr.resolve_executable(
                    crate::plugins::REMUX_TOOL_ID,
                    crate::plugins::REMUX_TOOL_VERSION,
                    "mkvpropedit",
                    "MKVPROPEDIT",
                );
                media::propedit_in_place(
                    &propedit_bin,
                    &current_working_file,
                    edit.default_audio_lang.as_deref(),
                    edit.force_subtitles,
                )
                .await?;
                tracing::info!(job_id = %job_id, "in-place mkvpropedit completed directly on working file");
            }
            if decision.reinpect {
                ctx.media_info = media::inspect_with(&mkvmerge_bin, &current_working_file).await?;
                tracing::info!(job_id = %job_id, "media inspect refreshed for working file");
            }
            if decision.transform {
                is_in_place = false;
                let target_output = work_dir.join("output.mkv");
                let on_p = on_progress.clone();
                final_warning = media::remux_with_progress(
                    &mkvmerge_bin,
                    &current_working_file,
                    &target_output,
                    move |pct: u32, line: &str| {
                        on_p(pct, line);
                    },
                )
                .await?;
                current_working_file = target_output;
                tracing::info!(job_id = %job_id, "remux step executed into staging working file");
            }
            if let Some(plan) = &decision.encode {
                is_in_place = false;
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
                let args = argv_for(plan, &current_working_file, &target_output);
                let on_p = on_progress.clone();
                media::encode_with_progress(&ffmpeg_bin, &args, move |pct: u32, line: &str| {
                    on_p(pct, line);
                })
                .await?;
                current_working_file = target_output;
                ctx.working_file = current_working_file.clone();
                tracing::info!(job_id = %job_id, block = %node.kind, "plugin encode step completed");
            }
            if let Some(webhook_cfg) = &decision.notify {
                let mut webhook_cfg = webhook_cfg.clone();
                if let Some(dir) = self.secrets_dir {
                    decrypt_headers(
                        &crate::secrets::SecretBox::open(dir)?,
                        &flow.id,
                        &node.id,
                        &mut webhook_cfg,
                    )?;
                }
                execute_webhook_block(&webhook_cfg, &step_ctx, self.http_client).await?;
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
                        return Err(error);
                    }
                    tracing::warn!(job_id = %job_id, service = ?block.service, error = %error, "bloc d'intégration en échec, flux poursuivi");
                }
            }
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
        Ok(FlowExecutionOutcome {
            working_file: current_working_file,
            in_place: is_in_place,
            warning: final_warning,
        })
    }
}

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
        assert_eq!(graph.nodes[0].kind, FlowBlockKind::FileInput);
        assert_eq!(graph.nodes[1].kind, FlowBlockKind::RemuxMp4);
        assert_eq!(
            graph.nodes[2].kind,
            FlowBlockKind::Other("legacy-unknown".into())
        );
        let encoded = serde_json::to_value(&graph)?;
        assert_eq!(encoded["nodes"][1]["type"], "block.video.remux-mp4");
        Ok(())
    }
}
