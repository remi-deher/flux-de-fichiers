//! Exécuteur déclaratif des opérations HTTP (webhooks et intégrations multi-étapes *arr / API REST).

use crate::flows::{
    FlowContext,
    contract::{
        HttpExtractKind, HttpExtractRule, HttpRuntime, HttpStep, PathMappingContract,
        scalar_to_string,
    },
    header_secrets::decrypt_headers,
};
use forgearr_shared::jobs::JobErrorCode;
use reqwest::{Client, Method};
use serde_json::{Map, Value};
use std::{collections::HashMap, path::Path, time::Duration};
use thiserror::Error;

/// Résultat d'une étape HTTP exécutée selon son contrat déclaratif.
#[derive(Clone, Debug)]
pub enum HttpStepOutcome {
    Succeeded { message: Option<String> },
    Skipped { message: String },
}

#[derive(Debug, Error)]
#[error("{message}")]
pub struct HttpStepError {
    pub code: JobErrorCode,
    pub message: String,
}

/// Exécute une opération HTTP multi-étapes (webhooks ou intégrations REST).
#[allow(clippy::too_many_arguments)]
pub async fn execute_http_operation(
    rt: &HttpRuntime,
    flow_id: &str,
    node_id: &str,
    node_config: &Value,
    plugin_configs: &HashMap<String, Value>,
    ctx: &FlowContext,
    secrets_dir: Option<&Path>,
    client: &Client,
) -> Result<HttpStepOutcome, HttpStepError> {
    let policy = crate::net_policy::OutboundPolicy::integration_from_env();
    execute_http_operation_with_policy(
        rt,
        flow_id,
        node_id,
        node_config,
        plugin_configs,
        ctx,
        secrets_dir,
        client,
        policy,
    )
    .await
}

/// Variante de `execute_http_operation` avec politique réseau sortante explicite.
#[allow(clippy::too_many_arguments)]
pub async fn execute_http_operation_with_policy(
    rt: &HttpRuntime,
    flow_id: &str,
    node_id: &str,
    node_config: &Value,
    plugin_configs: &HashMap<String, Value>,
    ctx: &FlowContext,
    secrets_dir: Option<&Path>,
    client: &Client,
    outbound_policy: crate::net_policy::OutboundPolicy,
) -> Result<HttpStepOutcome, HttpStepError> {
    let error_code = rt.error_code.unwrap_or(JobErrorCode::IntegrationFailed);
    let fail_on_error = rt
        .fail_on_error_param
        .as_deref()
        .and_then(|key| node_config.get(key))
        .and_then(Value::as_bool)
        .unwrap_or(false);

    let fail_or_skip = |err_msg: String| -> Result<HttpStepOutcome, HttpStepError> {
        if fail_on_error {
            Err(HttpStepError {
                code: error_code,
                message: err_msg,
            })
        } else {
            Ok(HttpStepOutcome::Skipped {
                message: format!("échec non bloquant : {err_msg}"),
            })
        }
    };

    let settings = match &rt.settings_plugin_id {
        Some(plugin_id) => {
            let Some(cfg) = plugin_configs.get(plugin_id) else {
                return fail_or_skip(format!("réglages de « {plugin_id} » non configurés"));
            };
            for req in &rt.required_settings {
                let empty = cfg
                    .get(req)
                    .and_then(Value::as_str)
                    .is_none_or(|s| s.trim().is_empty());
                if empty {
                    return fail_or_skip(format!(
                        "réglage requis « {req} » manquant pour « {plugin_id} »"
                    ));
                }
            }
            Some(cfg)
        }
        None => None,
    };

    let custom_headers = match load_custom_headers(node_config, secrets_dir, flow_id, node_id) {
        Ok(h) => h,
        Err(msg) => {
            return Err(HttpStepError {
                code: error_code,
                message: msg,
            });
        }
    };

    let mut vars: HashMap<String, Value> = HashMap::new();
    if let Some(mapping) = &rt.path_mapping {
        match compute_path_mapping(mapping, settings, ctx) {
            Ok(mapped) => {
                vars.insert(mapping.output_var.clone(), Value::String(mapped));
            }
            Err(msg) => return fail_or_skip(msg),
        }
    }

    let timeout = Duration::from_secs(rt.timeout_seconds.max(1));
    let mut last_message = None;

    for step in &rt.steps {
        match run_single_step(
            step,
            node_config,
            settings,
            &vars,
            ctx,
            &custom_headers,
            client,
            timeout,
            outbound_policy,
        )
        .await
        {
            Ok(StepResult::Continue(extracted, msg)) => {
                for (k, v) in extracted {
                    vars.insert(k, v);
                }
                if msg.is_some() {
                    last_message = msg;
                }
            }
            Ok(StepResult::Skipped(skip_msg)) => {
                return Ok(HttpStepOutcome::Skipped { message: skip_msg });
            }
            Err(err_msg) => return fail_or_skip(err_msg),
        }
    }

    Ok(HttpStepOutcome::Succeeded {
        message: last_message,
    })
}

enum StepResult {
    Continue(Vec<(String, Value)>, Option<String>),
    Skipped(String),
}

#[allow(clippy::too_many_arguments)]
async fn run_single_step(
    step: &HttpStep,
    node_config: &Value,
    settings: Option<&Value>,
    vars: &HashMap<String, Value>,
    ctx: &FlowContext,
    custom_headers: &HashMap<String, String>,
    client: &Client,
    timeout: Duration,
    outbound_policy: crate::net_policy::OutboundPolicy,
) -> Result<StepResult, String> {
    let url_raw = render_string_template(&step.url, node_config, settings, vars, ctx)?;
    if url_raw.trim().is_empty() {
        return Err("URL cible vide".to_owned());
    }
    let validated = crate::net_policy::validate_outbound_url(&url_raw, outbound_policy)
        .await
        .map_err(|err| format!("URL refusée par la politique réseau : {err}"))?;

    let method_str = render_string_template(&step.method, node_config, settings, vars, ctx)?;
    let method = Method::from_bytes(method_str.trim().to_ascii_uppercase().as_bytes())
        .unwrap_or(Method::POST);

    let mut request = client.request(method, validated).timeout(timeout);

    if let Some(query) = &step.query {
        let mut pairs = Vec::with_capacity(query.len());
        for (k, v) in query {
            let val = resolve_json_value(v, node_config, settings, vars, ctx)?;
            pairs.push((k.clone(), scalar_to_string(Some(&val))));
        }
        request = request.query(&pairs);
    }

    if let Some(headers) = &step.headers {
        for (k, v) in headers {
            let val = resolve_json_value(v, node_config, settings, vars, ctx)?;
            request = request.header(k, scalar_to_string(Some(&val)));
        }
    }
    if step.merge_custom_headers {
        for (k, v) in custom_headers {
            request = request.header(k, v);
        }
    }

    if let Some(body) = &step.body {
        let rendered_body = resolve_json_value(body, node_config, settings, vars, ctx)?;
        request = request.json(&rendered_body);
    }

    let response = request
        .send()
        .await
        .map_err(|err| format!("requête HTTP impossible : {err}"))?;
    let status = response.status();
    if !status.is_success() {
        return Err(format!("statut HTTP d'erreur : {status}"));
    }

    if let Some(rule) = &step.extract {
        let payload: Value = response
            .json()
            .await
            .map_err(|err| format!("réponse JSON illisible : {err}"))?;
        let extracted = apply_extract_rule(rule, &payload, vars)?;
        if extracted.is_none() {
            let skip = rule
                .skip_message
                .clone()
                .unwrap_or_else(|| "élément introuvable, étape ignorée".to_owned());
            return Ok(StepResult::Skipped(skip));
        }
        return Ok(StepResult::Continue(
            extracted.unwrap_or_default(),
            step.success_message.clone(),
        ));
    }

    Ok(StepResult::Continue(
        Vec::new(),
        step.success_message.clone(),
    ))
}

fn load_custom_headers(
    node_config: &Value,
    secrets_dir: Option<&Path>,
    flow_id: &str,
    node_id: &str,
) -> Result<HashMap<String, String>, String> {
    let mut headers = HashMap::new();
    if let Some(obj) = node_config.get("custom_headers").and_then(Value::as_object) {
        for (k, v) in obj {
            if let Some(text) = v.as_str() {
                headers.insert(k.clone(), text.to_owned());
            }
        }
    }
    if !headers.is_empty()
        && let Some(dir) = secrets_dir
    {
        let secrets = crate::secrets::SecretBox::open(dir)
            .map_err(|err| format!("clé de chiffrement webhook : {err}"))?;
        decrypt_headers(&secrets, flow_id, node_id, &mut headers)
            .map_err(|err| format!("déchiffrement en-têtes webhook : {err}"))?;
    }
    Ok(headers)
}

fn compute_path_mapping(
    mapping: &PathMappingContract,
    settings: Option<&Value>,
    ctx: &FlowContext,
) -> Result<String, String> {
    let local_base = settings
        .and_then(|s| s.get(&mapping.local_key))
        .and_then(Value::as_str)
        .unwrap_or("");
    let remote_base = settings
        .and_then(|s| s.get(&mapping.remote_key))
        .and_then(Value::as_str)
        .unwrap_or("");
    let local_file = ctx
        .media_root
        .as_deref()
        .map(|root| root.join(&ctx.source_name))
        .unwrap_or_else(|| Path::new(&ctx.source_name).to_path_buf());
    if local_base.is_empty() || remote_base.is_empty() {
        return Ok(local_file.to_string_lossy().into_owned());
    }
    let relative = local_file.strip_prefix(local_base).map_err(|_| {
        format!(
            "{} n'est pas sous le chemin local {}",
            local_file.display(),
            local_base
        )
    })?;
    Ok(Path::new(remote_base)
        .join(relative)
        .to_string_lossy()
        .into_owned())
}

fn apply_extract_rule(
    rule: &HttpExtractRule,
    payload: &Value,
    vars: &HashMap<String, Value>,
) -> Result<Option<Vec<(String, Value)>>, String> {
    let target = scalar_to_string(vars.get(&rule.target_var));
    let items = payload
        .as_array()
        .ok_or_else(|| "réponse JSON attendue sous forme de liste".to_owned())?;
    match rule.kind {
        HttpExtractKind::ArrayPathPrefix => {
            let best = items
                .iter()
                .filter_map(|item| {
                    let folder = item.get(&rule.path_field)?.as_str()?.trim_end_matches('/');
                    let id = item.get(&rule.id_field)?.clone();
                    let matches = target == folder || target.starts_with(&format!("{folder}/"));
                    matches.then_some((folder.len(), folder.to_owned(), id))
                })
                .max_by_key(|(len, _, _)| *len);
            let Some((_, matched_path, id)) = best else {
                return Ok(None);
            };
            let mut out = vec![(rule.save_as.clone(), id)];
            if let Some(save_path_as) = &rule.save_path_as {
                out.push((save_path_as.clone(), Value::String(matched_path)));
            }
            Ok(Some(out))
        }
        HttpExtractKind::ArrayRelativePath => {
            let base = rule
                .base_path_var
                .as_deref()
                .and_then(|v| vars.get(v))
                .and_then(Value::as_str)
                .unwrap_or("")
                .trim_end_matches('/');
            let found = items.iter().find_map(|item| {
                let id = item.get(&rule.id_field)?.clone();
                let abs_match = rule
                    .absolute_path_field
                    .as_deref()
                    .and_then(|f| item.get(f))
                    .and_then(Value::as_str)
                    .is_some_and(|p| p == target);
                let rel_match = item
                    .get(&rule.path_field)
                    .and_then(Value::as_str)
                    .is_some_and(|rel| {
                        let rel = rel.trim_start_matches('/');
                        let composed = if base.is_empty() {
                            rel.to_owned()
                        } else {
                            format!("{base}/{rel}")
                        };
                        composed == target || target.ends_with(&format!("/{rel}"))
                    });
                (abs_match || rel_match).then_some(id)
            });
            let Some(id) = found else {
                return Ok(None);
            };
            Ok(Some(vec![(rule.save_as.clone(), id)]))
        }
    }
}

fn resolve_json_value(
    value: &Value,
    node_config: &Value,
    settings: Option<&Value>,
    vars: &HashMap<String, Value>,
    ctx: &FlowContext,
) -> Result<Value, String> {
    match value {
        Value::String(s) => {
            if let Some(token) = s
                .strip_prefix("{$")
                .and_then(|inner| inner.strip_suffix('}'))
                && !token.contains("}{")
                && let Some(exact) =
                    resolve_exact_value_token(token, node_config, settings, vars, ctx)
            {
                return Ok(exact);
            }
            Ok(Value::String(render_string_template(
                s,
                node_config,
                settings,
                vars,
                ctx,
            )?))
        }
        Value::Array(items) => {
            let mut out = Vec::with_capacity(items.len());
            for item in items {
                out.push(resolve_json_value(item, node_config, settings, vars, ctx)?);
            }
            Ok(Value::Array(out))
        }
        Value::Object(obj) => {
            if obj.len() == 1
                && let Some(var_name) = obj.get("$var").and_then(Value::as_str)
            {
                return Ok(vars.get(var_name).cloned().unwrap_or(Value::Null));
            }
            let mut out = Map::with_capacity(obj.len());
            for (k, v) in obj {
                out.insert(
                    k.clone(),
                    resolve_json_value(v, node_config, settings, vars, ctx)?,
                );
            }
            Ok(Value::Object(out))
        }
        other => Ok(other.clone()),
    }
}

fn resolve_exact_value_token(
    token: &str,
    node_config: &Value,
    settings: Option<&Value>,
    vars: &HashMap<String, Value>,
    ctx: &FlowContext,
) -> Option<Value> {
    let (expr, _default) = token.split_once('|').unwrap_or((token, ""));
    match expr {
        "context.mediaInfo" => Some(ctx.media_info.clone()),
        "context.jobId" => Some(Value::String(ctx.job_id.to_string())),
        "context.sourceName" => Some(Value::String(ctx.source_name.clone())),
        "context.currentStep" => Some(Value::String(ctx.current_step.clone())),
        "context.status" => Some(Value::String(ctx.status.clone())),
        "context.timestamp" => Some(Value::String(ctx.timestamp.clone())),
        other if other.starts_with("var.") => vars.get(&other["var.".len()..]).cloned(),
        other if other.starts_with("setting.") => settings
            .and_then(|s| s.get(&other["setting.".len()..]))
            .cloned(),
        other if other.starts_with("config.") => {
            node_config.get(&other["config.".len()..]).cloned()
        }
        _ => None,
    }
}

fn render_string_template(
    template: &str,
    node_config: &Value,
    settings: Option<&Value>,
    vars: &HashMap<String, Value>,
    ctx: &FlowContext,
) -> Result<String, String> {
    let mut rendered = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(open) = rest.find("{$") {
        rendered.push_str(&rest[..open]);
        let after_open = &rest[open + 2..];
        let Some(close) = after_open.find('}') else {
            rendered.push_str("{$");
            rest = after_open;
            continue;
        };
        let token = &after_open[..close];
        let trailing = &after_open[close + 1..];
        let (expr, default_val) = token.split_once('|').unwrap_or((token, ""));
        let mut piece = resolve_exact_value_token(expr, node_config, settings, vars, ctx)
            .map(|v| scalar_to_string(Some(&v)))
            .unwrap_or_default();
        if piece.is_empty() && !default_val.is_empty() {
            piece = default_val.to_owned();
        }
        if expr.starts_with("setting.") && trailing.starts_with('/') {
            piece = piece.trim_end_matches('/').to_owned();
        }
        rendered.push_str(&piece);
        rest = trailing;
    }
    rendered.push_str(rest);
    Ok(rendered)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        Json, Router,
        routing::{get, post},
    };
    use serde_json::json;
    use tokio::net::TcpListener;
    use uuid::Uuid;

    #[tokio::test]
    async fn executes_declarative_multi_step_integration_and_webhook() -> anyhow::Result<()> {
        let state = json!([
            { "id": 42, "path": "/tv/Show" }
        ]);
        let ep_files = json!([
            { "id": 99, "relativePath": "Season 1/ep1.mkv", "path": "/tv/Show/Season 1/ep1.mkv" }
        ]);
        let app = Router::new()
            .route(
                "/api/v3/series",
                get(move || async move { Json(state.clone()) }),
            )
            .route(
                "/api/v3/episodefile",
                get(move || async move { Json(ep_files.clone()) }),
            )
            .route(
                "/api/v3/command",
                post(|Json(body): Json<Value>| async move { Json(body) }),
            );
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let addr = listener.local_addr()?;
        tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });

        let op_json = json!({
            "kind": "http",
            "settingsPluginId": "sonarr",
            "requiredSettings": ["url", "apiKey"],
            "errorCode": "integration_failed",
            "failOnErrorParam": "fail_on_error",
            "timeoutSeconds": 5,
            "pathMapping": {
                "localKey": "pathMappingLocal",
                "remoteKey": "pathMappingRemote",
                "outputVar": "remotePath"
            },
            "steps": [
                {
                    "url": "{$setting.url}/api/v3/series",
                    "method": "GET",
                    "headers": { "X-Api-Key": "{$setting.apiKey}" },
                    "extract": {
                        "kind": "arrayPathPrefix",
                        "pathField": "path",
                        "idField": "id",
                        "targetVar": "remotePath",
                        "saveAs": "itemId",
                        "savePathAs": "itemPath"
                    }
                },
                {
                    "url": "{$setting.url}/api/v3/episodefile",
                    "method": "GET",
                    "query": { "seriesId": "{$var.itemId}" },
                    "headers": { "X-Api-Key": "{$setting.apiKey}" },
                    "extract": {
                        "kind": "arrayRelativePath",
                        "pathField": "relativePath",
                        "absolutePathField": "path",
                        "basePathVar": "itemPath",
                        "targetVar": "remotePath",
                        "idField": "id",
                        "saveAs": "fileId"
                    }
                },
                {
                    "url": "{$setting.url}/api/v3/command",
                    "method": "POST",
                    "headers": { "X-Api-Key": "{$setting.apiKey}" },
                    "body": { "name": "RenameFiles", "seriesId": { "$var": "itemId" }, "files": [{ "$var": "fileId" }] },
                    "successMessage": "commande *arr transmise"
                }
            ]
        });
        let rt: HttpRuntime = serde_json::from_value(op_json)?;
        let configs = HashMap::from([(
            "sonarr".to_owned(),
            json!({
                "url": format!("http://{addr}/"),
                "apiKey": "secret",
                "pathMappingLocal": "/data",
                "pathMappingRemote": "/tv"
            }),
        )]);
        let mut ctx = FlowContext::new(
            Uuid::new_v4(),
            "Show/Season 1/ep1.mkv".to_owned(),
            Path::new("/tmp/ep1.mkv").to_path_buf(),
            json!({}),
        );
        ctx.media_root = Some(Path::new("/data").to_path_buf());

        let outcome = execute_http_operation(
            &rt,
            "flow-1",
            "n1",
            &json!({ "fail_on_error": true }),
            &configs,
            &ctx,
            None,
            &Client::new(),
        )
        .await?;
        assert!(matches!(
            outcome,
            HttpStepOutcome::Succeeded { message: Some(ref m) } if m == "commande *arr transmise"
        ));
        Ok(())
    }
}
