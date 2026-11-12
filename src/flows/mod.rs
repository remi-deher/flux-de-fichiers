//! Moteur d'exécution des flux et blocs d'action composables (ADR 0008, ADR 0009).

mod block;
mod condition;
mod route;
mod tool;
mod webhook;

pub use block::{FlowBlockKind, StepDecision, decide_step};
pub use condition::{ConditionConfig, evaluate_condition};
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

/// Nœud unitaire d'un graphe de flux.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FlowNode {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: FlowBlockKind,
    pub name: String,
    #[serde(default)]
    pub config: serde_json::Value,
}

/// Connexion orientée entre deux nœuds d'un flux.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FlowConnection {
    pub from: String,
    pub to: String,
    #[serde(default)]
    pub port: Option<String>,
}

/// Représentation complète d'un graphe de flux désérialisé.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FlowGraph {
    pub nodes: Vec<FlowNode>,
    pub connections: Vec<FlowConnection>,
}

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
}

pub type FlowProgressCallback = std::sync::Arc<dyn Fn(u32, &str) + Send + Sync>;

impl<'a> FlowEngine<'a> {
    pub fn new(cache_mgr: &'a ToolCacheManager, http_client: &'a reqwest::Client) -> Self {
        Self {
            cache_mgr,
            http_client,
        }
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
        let mkvmerge_bin =
            self.cache_mgr
                .resolve_executable("tool.mkvtoolnix", "1.0.0", "mkvmerge", "MKVMERGE");
        let media_info = media::inspect_with(&mkvmerge_bin, source_file).await?;
        let mut current_working_file = source_file.to_path_buf();
        let mut is_in_place = true;
        let mut final_warning = false;
        let ctx = FlowContext::new(
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
                    "tool.mkvtoolnix",
                    "1.0.0",
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
            if let Some(webhook_cfg) = &decision.notify {
                let _ = execute_webhook_block(webhook_cfg, &step_ctx, self.http_client).await;
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
            r#"{"nodes":[{"id":"node-1","type":"file-input","name":"Source"},{"id":"node-2","type":"remux-mp4","name":"MP4"}],"connections":[{"from":"node-1","to":"node-2"}]}"#,
        )?;
        assert_eq!(graph.nodes[0].kind, FlowBlockKind::FileInput);
        assert_eq!(
            graph.nodes[1].kind,
            FlowBlockKind::Other("remux-mp4".into())
        );
        let encoded = serde_json::to_value(&graph)?;
        assert_eq!(encoded["nodes"][1]["type"], "remux-mp4");
        Ok(())
    }
}
