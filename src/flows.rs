//! Moteur d'exécution des flux et blocs d'action composables (ADR 0008, ADR 0009).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, path::PathBuf, time::Duration};
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

/// Configuration déclarative d'un bloc Webhook / Notification.
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

/// Nature d'un bloc d'action pouvant composer un flux.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum FlowStepKind {
    /// Modification directe des en-têtes sans duplication disque (mkvpropedit).
    InPlacePropEdit {
        default_audio_lang: Option<String>,
        force_subtitles: bool,
    },
    /// Réencodage vidéo (FFmpeg).
    VideoEncode {
        codec: String,
        encoder: String,
        quality: u8,
    },
    /// Réencodage audio (FFmpeg).
    AudioEncode { codec: String, bitrate_kbps: u32 },
    /// Remuxage Matroska ou MP4.
    Remux {
        container: String,
        languages: Vec<String>,
    },
    /// Bloc de notification / webhook découplé (ADR 0009).
    Webhook(WebhookBlockConfig),
    /// Déclenchement d'un rescan Sonarr/Radarr via leur API respective.
    ArrRescan {
        app_type: String, // "sonarr" ou "radarr"
        endpoint_url: String,
        api_key: String,
    },
}

/// Étape déclarative d'un flux.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FlowStep {
    pub id: String,
    pub name: String,
    pub kind: FlowStepKind,
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
