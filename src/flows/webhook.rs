use super::FlowContext;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, time::Duration};

const WEBHOOK_TIMEOUT: Duration = Duration::from_secs(10);

/// Configuration déclarative d'un bloc Webhook / Notification (ADR 0009).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
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

pub fn webhook_method(method: &str) -> reqwest::Method {
    match method.to_uppercase().as_str() {
        "GET" => reqwest::Method::GET,
        "PUT" => reqwest::Method::PUT,
        "PATCH" => reqwest::Method::PATCH,
        _ => reqwest::Method::POST,
    }
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

    let mut request = client
        .request(webhook_method(&config.method), &config.url)
        .timeout(WEBHOOK_TIMEOUT);
    for (key, value) in &config.custom_headers {
        request = request.header(key, value);
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
    match request.json(&payload).send().await {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn webhook_method_maps_known_verbs_and_defaults_to_post() {
        assert_eq!(webhook_method("get"), reqwest::Method::GET);
        assert_eq!(webhook_method("PUT"), reqwest::Method::PUT);
        assert_eq!(webhook_method("patch"), reqwest::Method::PATCH);
        assert_eq!(webhook_method("post"), reqwest::Method::POST);
        assert_eq!(webhook_method("delete"), reqwest::Method::POST);
    }
}
