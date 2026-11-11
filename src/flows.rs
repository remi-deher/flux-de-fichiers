//! Moteur d'exécution des flux et blocs d'action composables (ADR 0008, ADR 0009).

use crate::{domain::FlowRecord, media, tools_cache::ToolCacheManager};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    time::Duration,
};
use uuid::Uuid;

const WEBHOOK_TIMEOUT: Duration = Duration::from_secs(10);

/// Contexte d'exécution instantané transmis d'étape en étape à travers le flux.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FlowContext {
    pub job_id: Uuid,
    pub source_name: String,
    pub working_file: PathBuf,
    pub media_info: serde_json::Value,
    pub status: String,
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
            status: "running".into(),
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
    pub node_type: String,
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
    pub port: Option<String>, // None, Some("true"), Some("false"), Some("fallback")
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

/// Configuration d'une condition d'aiguillage (ADR 0008).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ConditionConfig {
    #[serde(default)]
    pub property: String,
    #[serde(default = "default_op")]
    pub operator: String,
    #[serde(default)]
    pub target: String,
}

fn default_op() -> String {
    "eq".into()
}

/// Évalue une condition d'aiguillage sur les métadonnées techniques extraites.
pub fn evaluate_condition(config: &ConditionConfig, media_info: &serde_json::Value) -> bool {
    let prop = config.property.to_lowercase();
    let op = config.operator.to_lowercase();
    let target = config.target.to_lowercase();

    let tracks = media_info["tracks"].as_array();

    let match_found = if prop == "video.codec" {
        tracks
            .map(|arr| {
                arr.iter().any(|t| {
                    t["type"].as_str() == Some("video")
                        && t["codec"]
                            .as_str()
                            .map(|c| c.to_lowercase().contains(&target))
                            .unwrap_or(false)
                })
            })
            .unwrap_or(false)
    } else if prop == "audio.language" {
        tracks
            .map(|arr| {
                arr.iter().any(|t| {
                    t["type"].as_str() == Some("audio")
                        && t["properties"]["language"]
                            .as_str()
                            .map(|l| l.to_lowercase().contains(&target))
                            .unwrap_or(false)
                })
            })
            .unwrap_or(false)
    } else {
        // Recherche générique dans l'objet media_info
        media_info.to_string().to_lowercase().contains(&target)
    };

    match op.as_str() {
        "eq" | "contains" => match_found,
        "neq" => !match_found,
        _ => match_found,
    }
}

/// Configuration déclarative d'un bloc Webhook / Notification (ADR 0009).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WebhookBlockConfig {
    pub url: String,
    #[serde(default = "default_method")]
    pub method: String,
    #[serde(default)]
    pub custom_headers: HashMap<String, String>,
    #[serde(default)]
    pub fail_on_error: bool,
}

fn default_method() -> String {
    "POST".into()
}

/// Exécute un bloc Webhook / Notification en lui transmettant le contexte instantané du job.
pub async fn execute_webhook_block(
    config: &WebhookBlockConfig,
    ctx: &FlowContext,
    client: &reqwest::Client,
) -> anyhow::Result<()> {
    tracing::info!(
        job_id = %ctx.job_id,
        step = %ctx.current_step,
        target_url = %config.url,
        "executing flow webhook block"
    );

    let method = match config.method.to_uppercase().as_str() {
        "GET" => reqwest::Method::GET,
        "PUT" => reqwest::Method::PUT,
        "PATCH" => reqwest::Method::PATCH,
        _ => reqwest::Method::POST,
    };

    let mut request = client.request(method, &config.url).timeout(WEBHOOK_TIMEOUT);

    for (k, v) in &config.custom_headers {
        request = request.header(k, v);
    }

    let payload = serde_json::json!({
        "event": "flow_step_reached",
        "job_id": ctx.job_id,
        "source_name": ctx.source_name,
        "current_step": ctx.current_step,
        "status": ctx.status,
        "timestamp": ctx.timestamp,
        "media_info": ctx.media_info,
    });

    let result = request.json(&payload).send().await;

    match result {
        Ok(resp) if resp.status().is_success() => {
            tracing::info!(
                job_id = %ctx.job_id,
                step = %ctx.current_step,
                status_code = %resp.status(),
                "flow webhook delivered successfully"
            );
            Ok(())
        }
        Ok(resp) => {
            let status = resp.status();
            tracing::warn!(
                job_id = %ctx.job_id,
                step = %ctx.current_step,
                status_code = %status,
                "flow webhook target returned non-success HTTP status"
            );
            if config.fail_on_error {
                anyhow::bail!("webhook failed with HTTP status: {status}");
            }
            Ok(())
        }
        Err(err) => {
            tracing::warn!(
                job_id = %ctx.job_id,
                step = %ctx.current_step,
                error = %err,
                "flow webhook network request failed"
            );
            if config.fail_on_error {
                return Err(err.into());
            }
            Ok(())
        }
    }
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

        // Étape 0 : Inspection implicite initiale (ADR 0008 & ADR 0009)
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
                .map(|f| f.to_string_lossy().into_owned())
                .unwrap_or_else(|| "source.mkv".into()),
            current_working_file.clone(),
            media_info,
        );

        // Recherche du nœud d'entrée file-input
        let input_node = graph
            .nodes
            .iter()
            .find(|n| n.node_type == "file-input")
            .cloned()
            .unwrap_or_else(|| FlowNode {
                id: "node-1".into(),
                node_type: "file-input".into(),
                name: "Fichier source".into(),
                config: serde_json::Value::Null,
            });

        let mut current_node_id = input_node.id;

        // Boucle de cheminement dans le graphe
        let mut max_steps = 32;
        while max_steps > 0 {
            max_steps -= 1;

            let current_node = graph
                .nodes
                .iter()
                .find(|n| n.id == current_node_id)
                .cloned();

            let node = match current_node {
                Some(n) => n,
                None => break,
            };

            let step_ctx = ctx.with_step(&node.name);

            let mut port_filter: Option<String> = None;

            match node.node_type.as_str() {
                "file-input" => {
                    tracing::info!(job_id = %job_id, "file input initialized with media info");
                }
                "condition" => {
                    let cond_cfg: ConditionConfig = serde_json::from_value(node.config.clone())
                        .unwrap_or_else(|_| ConditionConfig {
                            property: "video.codec".into(),
                            operator: "eq".into(),
                            target: "hevc".into(),
                        });
                    let cond_result = evaluate_condition(&cond_cfg, &step_ctx.media_info);
                    tracing::info!(
                        job_id = %job_id,
                        condition = %cond_cfg.property,
                        target = %cond_cfg.target,
                        result = cond_result,
                        "evaluated conditional branching"
                    );
                    port_filter = Some(if cond_result {
                        "true".into()
                    } else {
                        "false".into()
                    });
                }
                "mkvpropedit" => {
                    // Édition In-Place directe sans copie (ADR 0008)
                    let propedit_bin = self.cache_mgr.resolve_executable(
                        "tool.mkvtoolnix",
                        "1.0.0",
                        "mkvpropedit",
                        "MKVPROPEDIT",
                    );
                    let default_lang = node.config["default_audio_lang"].as_str();
                    let force_subs = node.config["force_subtitles"].as_bool().unwrap_or(false);

                    media::propedit_in_place(
                        &propedit_bin,
                        &current_working_file,
                        default_lang,
                        force_subs,
                    )
                    .await?;
                    tracing::info!(job_id = %job_id, "in-place mkvpropedit completed directly on working file");
                }
                "remux-mkv" | "video-encode" => {
                    // Opération de transformation dans le dossier éphémère /work
                    is_in_place = false;
                    let target_output = work_dir.join("output.mkv");
                    let on_p = on_progress.clone();
                    let warning = media::remux_with_progress(
                        &mkvmerge_bin,
                        &current_working_file,
                        &target_output,
                        move |pct: u32, line: &str| {
                            on_p(pct, line);
                        },
                    )
                    .await?;
                    final_warning = warning;
                    current_working_file = target_output;
                    tracing::info!(job_id = %job_id, "remux step executed into staging working file");
                }
                "webhook" => {
                    if let Ok(webhook_cfg) =
                        serde_json::from_value::<WebhookBlockConfig>(node.config)
                    {
                        let _ =
                            execute_webhook_block(&webhook_cfg, &step_ctx, self.http_client).await;
                    }
                }
                "file-output" => {
                    tracing::info!(job_id = %job_id, "reached final file-output node, terminating pipeline");
                    break;
                }
                _ => {
                    tracing::info!(job_id = %job_id, node_type = %node.node_type, "step pass-through");
                }
            }

            // Sélection de la connexion sortante suivante
            let next_conn = graph.connections.iter().find(|c| {
                if c.from != current_node_id {
                    return false;
                }
                match (&port_filter, &c.port) {
                    (Some(expected), Some(actual)) => expected == actual,
                    (None, None) => true,
                    (Some(_), None) => true,
                    (None, Some(_)) => false,
                }
            });

            match next_conn {
                Some(conn) => {
                    current_node_id = conn.to.clone();
                }
                None => break,
            }
        }

        Ok(FlowExecutionOutcome {
            working_file: current_working_file,
            in_place: is_in_place,
            warning: final_warning,
        })
    }
}
