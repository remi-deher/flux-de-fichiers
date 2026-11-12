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

    async fn serve_status(code: u16) -> anyhow::Result<std::net::SocketAddr> {
        use axum::{Router, http::StatusCode, routing::post};
        let status = StatusCode::from_u16(code)?;
        let router = Router::new().route("/hook", post(move || async move { status }));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let address = listener.local_addr()?;
        tokio::spawn(async move {
            let _ = axum::serve(listener, router).await;
        });
        Ok(address)
    }

    fn config(address: std::net::SocketAddr, fail_on_error: bool) -> WebhookBlockConfig {
        WebhookBlockConfig {
            url: format!("http://{address}/hook"),
            method: "POST".into(),
            custom_headers: HashMap::new(),
            fail_on_error,
        }
    }

    fn context() -> FlowContext {
        FlowContext::new(
            uuid::Uuid::nil(),
            "film.mkv".into(),
            std::path::PathBuf::from("film.mkv"),
            serde_json::json!({}),
        )
    }

    #[tokio::test]
    async fn webhook_failure_is_reported_only_when_fail_on_error_is_set() -> anyhow::Result<()> {
        let address = serve_status(500).await?;
        let client = reqwest::Client::new();
        assert!(
            execute_webhook_block(&config(address, true), &context(), &client)
                .await
                .is_err()
        );
        execute_webhook_block(&config(address, false), &context(), &client).await?;
        Ok(())
    }

    #[tokio::test]
    async fn webhook_success_is_not_an_error() -> anyhow::Result<()> {
        let address = serve_status(204).await?;
        execute_webhook_block(&config(address, true), &context(), &reqwest::Client::new()).await?;
        Ok(())
    }
}
