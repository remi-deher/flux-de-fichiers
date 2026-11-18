//! Client HTTP sortant vers Ollama, OpenAI/compatible et Anthropic Claude.
//!
//! Mutualise l'épinglage DNS anti-SSRF (`net_policy`) et normalise les échanges multi-tours
//! avec appels d'outils (`function calling`).

use crate::{
    error::{AppError, AppResult},
    mcp::protocol::McpTool,
    net_policy::{NetPolicyError, OutboundPolicy, build_pinned_client, resolve_checked},
};
use flux_shared::ai::{AiChatMessage, AiChatRole, AiProvider, AiTestResponse};
use reqwest::{Client, Url};
use serde_json::{Value, json};
use std::time::{Duration, Instant};

const LLM_REQUEST_TIMEOUT: Duration = Duration::from_secs(90);
const LLM_TEST_TIMEOUT: Duration = Duration::from_secs(12);
const ANTHROPIC_VERSION: &str = "2023-06-01";
const ANTHROPIC_MAX_TOKENS: u32 = 2048;
const MAX_DETECTED_MODELS: usize = 40;
const MAX_PROVIDER_ERROR_CHARS: usize = 240;

#[derive(Clone, Debug)]
pub struct ProviderEndpoint {
    pub provider: AiProvider,
    pub base_url: String,
    pub model: String,
    pub api_key: Option<String>,
}

#[derive(Clone, Debug)]
pub struct ToolCallRequest {
    pub id: String,
    pub name: String,
    pub arguments: Value,
}

#[derive(Clone, Debug)]
pub enum ModelTurnResponse {
    FinalText(String),
    ToolCalls {
        assistant_content: Option<String>,
        calls: Vec<ToolCallRequest>,
    },
}

#[derive(Clone, Debug)]
pub enum ConversationEntry {
    UserText(String),
    AssistantText(String),
    AssistantToolCalls {
        content: Option<String>,
        calls: Vec<ToolCallRequest>,
    },
    ToolResult {
        call_id: String,
        tool_name: String,
        content: String,
        is_error: bool,
    },
}

pub fn initial_entries(messages: &[AiChatMessage]) -> Vec<ConversationEntry> {
    messages
        .iter()
        .map(|msg| match msg.role {
            AiChatRole::User => ConversationEntry::UserText(msg.content.clone()),
            AiChatRole::Assistant => ConversationEntry::AssistantText(msg.content.clone()),
        })
        .collect()
}

fn map_net_error(error: NetPolicyError) -> AppError {
    match error {
        NetPolicyError::InvalidUrl(_) => AppError::Invalid,
        NetPolicyError::ForbiddenHost(_) => AppError::Invalid,
        NetPolicyError::Network(_) => AppError::Internal,
    }
}

async fn pinned_client_for(
    raw_url: &str,
    policy: OutboundPolicy,
    timeout: Duration,
) -> AppResult<(Url, Client)> {
    let url = Url::parse(raw_url).map_err(|_| AppError::Invalid)?;
    let (_host, addrs) = resolve_checked(&url, policy).await.map_err(map_net_error)?;
    let client = build_pinned_client(&url, &addrs, timeout).map_err(map_net_error)?;
    Ok((url, client))
}

fn join_base(base_url: &str, suffix: &str) -> String {
    let trimmed = base_url.trim().trim_end_matches('/');
    let suffix = suffix.trim_start_matches('/');
    format!("{trimmed}/{suffix}")
}

fn openai_endpoint(base_url: &str, leaf: &str) -> String {
    let trimmed = base_url.trim().trim_end_matches('/');
    let leaf = leaf.trim_start_matches('/');
    if trimmed.ends_with("/v1") {
        format!("{trimmed}/{leaf}")
    } else {
        format!("{trimmed}/v1/{leaf}")
    }
}

fn anthropic_endpoint(base_url: &str, leaf: &str) -> String {
    openai_endpoint(base_url, leaf)
}

fn apply_bearer_if_present(
    req: reqwest::RequestBuilder,
    api_key: Option<&str>,
) -> reqwest::RequestBuilder {
    match api_key.map(str::trim).filter(|k| !k.is_empty()) {
        Some(key) => req.bearer_auth(key),
        None => req,
    }
}

fn truncate_error(raw: &str) -> String {
    let clean: String = raw
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    let mut chars = clean.chars();
    let head: String = chars.by_ref().take(MAX_PROVIDER_ERROR_CHARS).collect();
    if chars.next().is_some() {
        format!("{head}…")
    } else {
        head
    }
}

pub async fn test_connection(
    endpoint: &ProviderEndpoint,
    policy: OutboundPolicy,
) -> AiTestResponse {
    let started = Instant::now();
    let target_url = match endpoint.provider {
        AiProvider::Ollama => join_base(&endpoint.base_url, "/api/tags"),
        AiProvider::OpenAiCompatible => openai_endpoint(&endpoint.base_url, "models"),
        AiProvider::Anthropic => anthropic_endpoint(&endpoint.base_url, "models"),
    };
    let (url, client) = match pinned_client_for(&target_url, policy, LLM_TEST_TIMEOUT).await {
        Ok(pair) => pair,
        Err(err) => {
            return AiTestResponse {
                reachable: false,
                latency_ms: u64::try_from(started.elapsed().as_millis()).unwrap_or(0),
                available_models: Vec::new(),
                message: format!("Hôte invalide ou refusé ({err:?})."),
            };
        }
    };

    let mut req = client.get(url);
    req = match endpoint.provider {
        AiProvider::Ollama | AiProvider::OpenAiCompatible => {
            apply_bearer_if_present(req, endpoint.api_key.as_deref())
        }
        AiProvider::Anthropic => {
            let key = endpoint.api_key.as_deref().unwrap_or_default().trim();
            if key.is_empty() {
                return AiTestResponse {
                    reachable: false,
                    latency_ms: 0,
                    available_models: Vec::new(),
                    message: "Une clé d'API Anthropic est requise.".into(),
                };
            }
            req.header("x-api-key", key)
                .header("anthropic-version", ANTHROPIC_VERSION)
        }
    };

    match req.send().await {
        Ok(resp) => {
            let status = resp.status();
            let latency_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(0);
            let body: Value = resp.json().await.unwrap_or(Value::Null);
            if !status.is_success() {
                return AiTestResponse {
                    reachable: false,
                    latency_ms,
                    available_models: Vec::new(),
                    message: format!(
                        "Le fournisseur a répondu HTTP {} ({})",
                        status.as_u16(),
                        truncate_error(&body.to_string())
                    ),
                };
            }
            let models = extract_models(endpoint.provider, &body);
            let model_found = !endpoint.model.trim().is_empty()
                && (models.is_empty()
                    || models.iter().any(|candidate| candidate == &endpoint.model));
            let message = if models.is_empty() {
                "Connexion établie (aucun modèle listé par le serveur).".into()
            } else if model_found {
                format!(
                    "Connexion établie · {} modèle(s) détecté(s) · modèle sélectionné disponible.",
                    models.len()
                )
            } else {
                format!(
                    "Connexion établie · {} modèle(s) détecté(s) (modèle « {} » absent de la liste).",
                    models.len(),
                    endpoint.model
                )
            };
            AiTestResponse {
                reachable: true,
                latency_ms,
                available_models: models,
                message,
            }
        }
        Err(err) => AiTestResponse {
            reachable: false,
            latency_ms: u64::try_from(started.elapsed().as_millis()).unwrap_or(0),
            available_models: Vec::new(),
            message: format!("Serveur injoignable : {}", truncate_error(&err.to_string())),
        },
    }
}

fn extract_models(provider: AiProvider, body: &Value) -> Vec<String> {
    let items = match provider {
        AiProvider::Ollama => body.get("models").and_then(Value::as_array),
        AiProvider::OpenAiCompatible | AiProvider::Anthropic => {
            body.get("data").and_then(Value::as_array)
        }
    };
    let mut names: Vec<String> = items
        .into_iter()
        .flatten()
        .filter_map(|item| {
            item.get("name")
                .or_else(|| item.get("id"))
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(ToOwned::to_owned)
        })
        .collect();
    names.sort();
    names.dedup();
    names.truncate(MAX_DETECTED_MODELS);
    names
}

fn openai_tool_definitions(tools: &[McpTool]) -> Vec<Value> {
    tools
        .iter()
        .map(|tool| {
            json!({
                "type": "function",
                "function": {
                    "name": tool.name,
                    "description": tool.description,
                    "parameters": tool.input_schema,
                }
            })
        })
        .collect()
}

fn anthropic_tool_definitions(tools: &[McpTool]) -> Vec<Value> {
    tools
        .iter()
        .map(|tool| {
            json!({
                "name": tool.name,
                "description": tool.description,
                "input_schema": tool.input_schema,
            })
        })
        .collect()
}

fn parse_tool_arguments(raw: Option<&Value>) -> Value {
    match raw {
        Some(Value::Object(map)) => Value::Object(map.clone()),
        Some(Value::String(text)) => serde_json::from_str::<Value>(text)
            .ok()
            .filter(Value::is_object)
            .unwrap_or_else(|| json!({})),
        _ => json!({}),
    }
}

pub async fn complete_turn(
    endpoint: &ProviderEndpoint,
    system_prompt: &str,
    entries: &[ConversationEntry],
    tools: &[McpTool],
    policy: OutboundPolicy,
) -> Result<ModelTurnResponse, String> {
    match endpoint.provider {
        AiProvider::Ollama => {
            complete_ollama(endpoint, system_prompt, entries, tools, policy).await
        }
        AiProvider::OpenAiCompatible => {
            complete_openai(endpoint, system_prompt, entries, tools, policy).await
        }
        AiProvider::Anthropic => {
            complete_anthropic(endpoint, system_prompt, entries, tools, policy).await
        }
    }
}

fn build_openai_messages(system_prompt: &str, entries: &[ConversationEntry]) -> Vec<Value> {
    let mut messages = vec![json!({ "role": "system", "content": system_prompt })];
    for entry in entries {
        match entry {
            ConversationEntry::UserText(text) => {
                messages.push(json!({ "role": "user", "content": text }));
            }
            ConversationEntry::AssistantText(text) => {
                messages.push(json!({ "role": "assistant", "content": text }));
            }
            ConversationEntry::AssistantToolCalls { content, calls } => {
                let tool_calls: Vec<Value> = calls
                    .iter()
                    .map(|call| {
                        json!({
                            "id": call.id,
                            "type": "function",
                            "function": {
                                "name": call.name,
                                "arguments": call.arguments.to_string(),
                            }
                        })
                    })
                    .collect();
                messages.push(json!({
                    "role": "assistant",
                    "content": content.as_deref().unwrap_or(""),
                    "tool_calls": tool_calls,
                }));
            }
            ConversationEntry::ToolResult {
                call_id,
                tool_name,
                content,
                ..
            } => {
                messages.push(json!({
                    "role": "tool",
                    "tool_call_id": call_id,
                    "name": tool_name,
                    "content": content,
                }));
            }
        }
    }
    messages
}

fn build_ollama_messages(system_prompt: &str, entries: &[ConversationEntry]) -> Vec<Value> {
    let mut messages = vec![json!({ "role": "system", "content": system_prompt })];
    for entry in entries {
        match entry {
            ConversationEntry::UserText(text) => {
                messages.push(json!({ "role": "user", "content": text }));
            }
            ConversationEntry::AssistantText(text) => {
                messages.push(json!({ "role": "assistant", "content": text }));
            }
            ConversationEntry::AssistantToolCalls { content, calls } => {
                let tool_calls: Vec<Value> = calls
                    .iter()
                    .map(|call| {
                        json!({
                            "function": {
                                "name": call.name,
                                "arguments": call.arguments,
                            }
                        })
                    })
                    .collect();
                messages.push(json!({
                    "role": "assistant",
                    "content": content.as_deref().unwrap_or(""),
                    "tool_calls": tool_calls,
                }));
            }
            ConversationEntry::ToolResult {
                tool_name, content, ..
            } => {
                messages.push(json!({
                    "role": "tool",
                    "tool_name": tool_name,
                    "content": content,
                }));
            }
        }
    }
    messages
}

async fn complete_ollama(
    endpoint: &ProviderEndpoint,
    system_prompt: &str,
    entries: &[ConversationEntry],
    tools: &[McpTool],
    policy: OutboundPolicy,
) -> Result<ModelTurnResponse, String> {
    let target = join_base(&endpoint.base_url, "/api/chat");
    let (url, client) = pinned_client_for(&target, policy, LLM_REQUEST_TIMEOUT)
        .await
        .map_err(|e| format!("URL Ollama refusée ou injoignable ({e:?})"))?;
    let mut payload = json!({
        "model": endpoint.model,
        "messages": build_ollama_messages(system_prompt, entries),
        "stream": false,
    });
    if !tools.is_empty() {
        payload["tools"] = Value::Array(openai_tool_definitions(tools));
    }
    let req = apply_bearer_if_present(client.post(url).json(&payload), endpoint.api_key.as_deref());
    let resp = req.send().await.map_err(|err| {
        format!(
            "Erreur réseau Ollama : {}",
            truncate_error(&err.to_string())
        )
    })?;
    let status = resp.status();
    let body: Value = resp.json().await.map_err(|err| {
        format!(
            "Réponse Ollama invalide : {}",
            truncate_error(&err.to_string())
        )
    })?;
    if !status.is_success() {
        return Err(format!(
            "Ollama HTTP {} : {}",
            status.as_u16(),
            truncate_error(&body.to_string())
        ));
    }
    let message = body
        .get("message")
        .ok_or("Champ `message` absent d'Ollama")?;
    let content = message
        .get("content")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned);
    if let Some(raw_calls) = message.get("tool_calls").and_then(Value::as_array)
        && !raw_calls.is_empty()
    {
        let calls = raw_calls
            .iter()
            .enumerate()
            .filter_map(|(idx, item)| {
                let func = item.get("function")?;
                let name = func.get("name").and_then(Value::as_str)?.trim().to_owned();
                let id = item
                    .get("id")
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned)
                    .unwrap_or_else(|| format!("ollama_call_{idx}"));
                let arguments = parse_tool_arguments(func.get("arguments"));
                Some(ToolCallRequest {
                    id,
                    name,
                    arguments,
                })
            })
            .collect::<Vec<_>>();
        if !calls.is_empty() {
            return Ok(ModelTurnResponse::ToolCalls {
                assistant_content: content.filter(|c| !c.trim().is_empty()),
                calls,
            });
        }
    }
    Ok(ModelTurnResponse::FinalText(content.unwrap_or_default()))
}

async fn complete_openai(
    endpoint: &ProviderEndpoint,
    system_prompt: &str,
    entries: &[ConversationEntry],
    tools: &[McpTool],
    policy: OutboundPolicy,
) -> Result<ModelTurnResponse, String> {
    let target = openai_endpoint(&endpoint.base_url, "chat/completions");
    let (url, client) = pinned_client_for(&target, policy, LLM_REQUEST_TIMEOUT)
        .await
        .map_err(|e| format!("URL OpenAI/compatible refusée ({e:?})"))?;
    let mut payload = json!({
        "model": endpoint.model,
        "messages": build_openai_messages(system_prompt, entries),
    });
    if !tools.is_empty() {
        payload["tools"] = Value::Array(openai_tool_definitions(tools));
        payload["tool_choice"] = json!("auto");
    }
    let req = apply_bearer_if_present(client.post(url).json(&payload), endpoint.api_key.as_deref());
    let resp = req.send().await.map_err(|err| {
        format!(
            "Erreur réseau OpenAI : {}",
            truncate_error(&err.to_string())
        )
    })?;
    let status = resp.status();
    let body: Value = resp.json().await.map_err(|err| {
        format!(
            "Réponse OpenAI invalide : {}",
            truncate_error(&err.to_string())
        )
    })?;
    if !status.is_success() {
        return Err(format!(
            "Fournisseur HTTP {} : {}",
            status.as_u16(),
            truncate_error(&body.to_string())
        ));
    }
    let message = body
        .get("choices")
        .and_then(Value::as_array)
        .and_then(|arr| arr.first())
        .and_then(|choice| choice.get("message"))
        .ok_or("Réponse OpenAI sans `choices[0].message`")?;
    let content = message
        .get("content")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned);
    if let Some(raw_calls) = message.get("tool_calls").and_then(Value::as_array)
        && !raw_calls.is_empty()
    {
        let calls = raw_calls
            .iter()
            .enumerate()
            .filter_map(|(idx, item)| {
                let func = item.get("function")?;
                let name = func.get("name").and_then(Value::as_str)?.trim().to_owned();
                let id = item
                    .get("id")
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned)
                    .unwrap_or_else(|| format!("openai_call_{idx}"));
                let arguments = parse_tool_arguments(func.get("arguments"));
                Some(ToolCallRequest {
                    id,
                    name,
                    arguments,
                })
            })
            .collect::<Vec<_>>();
        if !calls.is_empty() {
            return Ok(ModelTurnResponse::ToolCalls {
                assistant_content: content.filter(|c| !c.trim().is_empty()),
                calls,
            });
        }
    }
    Ok(ModelTurnResponse::FinalText(content.unwrap_or_default()))
}

fn build_anthropic_messages(entries: &[ConversationEntry]) -> Vec<Value> {
    let mut messages: Vec<Value> = Vec::new();
    for entry in entries {
        match entry {
            ConversationEntry::UserText(text) => {
                messages.push(json!({ "role": "user", "content": text }));
            }
            ConversationEntry::AssistantText(text) => {
                messages.push(json!({ "role": "assistant", "content": text }));
            }
            ConversationEntry::AssistantToolCalls { content, calls } => {
                let mut blocks = Vec::new();
                if let Some(text) = content.as_deref().filter(|t| !t.trim().is_empty()) {
                    blocks.push(json!({ "type": "text", "text": text }));
                }
                for call in calls {
                    blocks.push(json!({
                        "type": "tool_use",
                        "id": call.id,
                        "name": call.name,
                        "input": call.arguments,
                    }));
                }
                messages.push(json!({ "role": "assistant", "content": blocks }));
            }
            ConversationEntry::ToolResult {
                call_id,
                content,
                is_error,
                ..
            } => {
                let block = json!({
                    "type": "tool_result",
                    "tool_use_id": call_id,
                    "content": content,
                    "is_error": is_error,
                });
                // Anthropic regroupe les tool_result consécutifs dans un seul message user.
                if let Some(last) = messages.last_mut()
                    && last.get("role").and_then(Value::as_str) == Some("user")
                    && let Some(arr) = last.get_mut("content").and_then(Value::as_array_mut)
                {
                    arr.push(block);
                } else {
                    messages.push(json!({ "role": "user", "content": [block] }));
                }
            }
        }
    }
    messages
}

async fn complete_anthropic(
    endpoint: &ProviderEndpoint,
    system_prompt: &str,
    entries: &[ConversationEntry],
    tools: &[McpTool],
    policy: OutboundPolicy,
) -> Result<ModelTurnResponse, String> {
    let key = endpoint
        .api_key
        .as_deref()
        .map(str::trim)
        .filter(|k| !k.is_empty())
        .ok_or_else(|| "Clé d'API Anthropic manquante".to_owned())?;
    let target = anthropic_endpoint(&endpoint.base_url, "messages");
    let (url, client) = pinned_client_for(&target, policy, LLM_REQUEST_TIMEOUT)
        .await
        .map_err(|e| format!("URL Anthropic refusée ({e:?})"))?;
    let mut payload = json!({
        "model": endpoint.model,
        "system": system_prompt,
        "max_tokens": ANTHROPIC_MAX_TOKENS,
        "messages": build_anthropic_messages(entries),
    });
    if !tools.is_empty() {
        payload["tools"] = Value::Array(anthropic_tool_definitions(tools));
    }
    let resp = client
        .post(url)
        .header("x-api-key", key)
        .header("anthropic-version", ANTHROPIC_VERSION)
        .json(&payload)
        .send()
        .await
        .map_err(|err| {
            format!(
                "Erreur réseau Anthropic : {}",
                truncate_error(&err.to_string())
            )
        })?;
    let status = resp.status();
    let body: Value = resp.json().await.map_err(|err| {
        format!(
            "Réponse Anthropic invalide : {}",
            truncate_error(&err.to_string())
        )
    })?;
    if !status.is_success() {
        return Err(format!(
            "Anthropic HTTP {} : {}",
            status.as_u16(),
            truncate_error(&body.to_string())
        ));
    }
    let blocks = body
        .get("content")
        .and_then(Value::as_array)
        .ok_or("Réponse Anthropic sans champ `content`")?;
    let mut text_parts = Vec::new();
    let mut calls = Vec::new();
    for (idx, block) in blocks.iter().enumerate() {
        match block.get("type").and_then(Value::as_str) {
            Some("text") => {
                if let Some(t) = block.get("text").and_then(Value::as_str) {
                    text_parts.push(t.to_owned());
                }
            }
            Some("tool_use") => {
                if let Some(name) = block.get("name").and_then(Value::as_str) {
                    let id = block
                        .get("id")
                        .and_then(Value::as_str)
                        .map(ToOwned::to_owned)
                        .unwrap_or_else(|| format!("anthropic_call_{idx}"));
                    let arguments = parse_tool_arguments(block.get("input"));
                    calls.push(ToolCallRequest {
                        id,
                        name: name.to_owned(),
                        arguments,
                    });
                }
            }
            _ => {}
        }
    }
    let combined_text = text_parts.join("\n");
    if !calls.is_empty() {
        Ok(ModelTurnResponse::ToolCalls {
            assistant_content: (!combined_text.trim().is_empty()).then_some(combined_text),
            calls,
        })
    } else {
        Ok(ModelTurnResponse::FinalText(combined_text))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn openai_and_anthropic_endpoints_normalize_v1_suffix() {
        assert_eq!(
            openai_endpoint("https://api.openai.com/v1", "chat/completions"),
            "https://api.openai.com/v1/chat/completions"
        );
        assert_eq!(
            openai_endpoint("http://localhost:1234/", "chat/completions"),
            "http://localhost:1234/v1/chat/completions"
        );
        assert_eq!(
            anthropic_endpoint("https://api.anthropic.com", "messages"),
            "https://api.anthropic.com/v1/messages"
        );
    }

    #[test]
    fn anthropic_builder_merges_consecutive_tool_results() {
        let entries = vec![
            ConversationEntry::UserText("Diagnostique".into()),
            ConversationEntry::AssistantToolCalls {
                content: None,
                calls: vec![
                    ToolCallRequest {
                        id: "c1".into(),
                        name: "list_jobs".into(),
                        arguments: json!({}),
                    },
                    ToolCallRequest {
                        id: "c2".into(),
                        name: "list_workers".into(),
                        arguments: json!({}),
                    },
                ],
            },
            ConversationEntry::ToolResult {
                call_id: "c1".into(),
                tool_name: "list_jobs".into(),
                content: "[]".into(),
                is_error: false,
            },
            ConversationEntry::ToolResult {
                call_id: "c2".into(),
                tool_name: "list_workers".into(),
                content: "[]".into(),
                is_error: false,
            },
        ];
        let msgs = build_anthropic_messages(&entries);
        assert_eq!(msgs.len(), 3);
        assert_eq!(
            msgs[2]
                .get("content")
                .and_then(Value::as_array)
                .map(Vec::len),
            Some(2)
        );
    }
}
