//! Client HTTP sortant vers Ollama, OpenAI/compatible, Anthropic Claude et ChatGPT (OAuth).
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
const CHATGPT_ORIGINATOR: &str = "flux_de_fichiers";
const CHATGPT_ACCOUNT_HEADER: &str = "chatgpt-account-id";
const SSE_DATA_PREFIX: &str = "data:";

#[derive(Clone, Debug)]
pub struct ProviderEndpoint {
    pub provider: AiProvider,
    pub base_url: String,
    pub model: String,
    /// Clé d'API, ou jeton d'accès OAuth pour les fournisseurs à connexion OAuth.
    pub api_key: Option<String>,
    /// Identifiant de compte exigé par ChatGPT (connexion OAuth uniquement).
    pub account_id: Option<String>,
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

pub(super) async fn pinned_client_for(
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
        // Le jeton a déjà été validé ou rafraîchi ; ce fournisseur ne liste pas ses modèles.
        AiProvider::ChatGptOauth => {
            return AiTestResponse {
                reachable: true,
                latency_ms: 0,
                available_models: Vec::new(),
                message: "Compte ChatGPT connecté (jeton valide). Le modèle est vérifié à la première question.".into(),
            };
        }
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
        AiProvider::Ollama | AiProvider::OpenAiCompatible | AiProvider::ChatGptOauth => {
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
        AiProvider::OpenAiCompatible | AiProvider::Anthropic | AiProvider::ChatGptOauth => {
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
        AiProvider::ChatGptOauth => {
            complete_chatgpt(endpoint, system_prompt, entries, tools, policy).await
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

fn responses_message(role: &str, content_type: &str, text: &str) -> Value {
    json!({
        "type": "message",
        "role": role,
        "content": [{ "type": content_type, "text": text }],
    })
}

/// Convertit l'historique au format `input` de l'API Responses.
fn build_responses_input(entries: &[ConversationEntry]) -> Vec<Value> {
    let mut items = Vec::new();
    for entry in entries {
        match entry {
            ConversationEntry::UserText(text) => {
                items.push(responses_message("user", "input_text", text));
            }
            ConversationEntry::AssistantText(text) => {
                items.push(responses_message("assistant", "output_text", text));
            }
            ConversationEntry::AssistantToolCalls { content, calls } => {
                if let Some(text) = content.as_deref().filter(|t| !t.trim().is_empty()) {
                    items.push(responses_message("assistant", "output_text", text));
                }
                items.extend(calls.iter().map(|call| {
                    json!({
                        "type": "function_call",
                        "call_id": call.id,
                        "name": call.name,
                        "arguments": call.arguments.to_string(),
                    })
                }));
            }
            ConversationEntry::ToolResult {
                call_id, content, ..
            } => {
                items.push(json!({
                    "type": "function_call_output",
                    "call_id": call_id,
                    "output": content,
                }));
            }
        }
    }
    items
}

fn responses_tool_definitions(tools: &[McpTool]) -> Vec<Value> {
    tools
        .iter()
        .map(|tool| {
            json!({
                "type": "function",
                "name": tool.name,
                "description": tool.description,
                "parameters": tool.input_schema,
            })
        })
        .collect()
}

/// Lit le flux SSE complet d'une réponse de l'API Responses.
fn parse_responses_sse(body: &str) -> Result<ModelTurnResponse, String> {
    let mut message_text: Vec<String> = Vec::new();
    let mut delta_text = String::new();
    let mut calls = Vec::new();
    let mut completed = false;
    for line in body.lines() {
        let Some(data) = line.trim().strip_prefix(SSE_DATA_PREFIX) else {
            continue;
        };
        let Ok(event) = serde_json::from_str::<Value>(data.trim()) else {
            continue;
        };
        match event.get("type").and_then(Value::as_str) {
            Some("response.output_text.delta") => {
                if let Some(delta) = event.get("delta").and_then(Value::as_str) {
                    delta_text.push_str(delta);
                }
            }
            Some("response.output_item.done") => {
                let item = event.get("item").unwrap_or(&Value::Null);
                match item.get("type").and_then(Value::as_str) {
                    Some("message") => message_text.extend(
                        item.get("content")
                            .and_then(Value::as_array)
                            .into_iter()
                            .flatten()
                            .filter_map(|part| part.get("text").and_then(Value::as_str))
                            .map(ToOwned::to_owned),
                    ),
                    Some("function_call") => {
                        if let Some(name) = item.get("name").and_then(Value::as_str) {
                            let id = item.get("call_id").and_then(Value::as_str).map_or_else(
                                || format!("chatgpt_call_{}", calls.len()),
                                str::to_owned,
                            );
                            calls.push(ToolCallRequest {
                                id,
                                name: name.to_owned(),
                                arguments: parse_tool_arguments(item.get("arguments")),
                            });
                        }
                    }
                    _ => {}
                }
            }
            Some("response.completed") => completed = true,
            Some("response.failed" | "response.incomplete" | "error") => {
                return Err(format!(
                    "ChatGPT a interrompu la réponse : {}",
                    truncate_error(&event.to_string())
                ));
            }
            _ => {}
        }
    }
    if !completed {
        return Err("Flux ChatGPT fermé avant la fin de la réponse".to_owned());
    }
    let text = if message_text.is_empty() {
        delta_text
    } else {
        message_text.join("\n")
    };
    if calls.is_empty() {
        Ok(ModelTurnResponse::FinalText(text))
    } else {
        Ok(ModelTurnResponse::ToolCalls {
            assistant_content: (!text.trim().is_empty()).then_some(text),
            calls,
        })
    }
}

async fn complete_chatgpt(
    endpoint: &ProviderEndpoint,
    system_prompt: &str,
    entries: &[ConversationEntry],
    tools: &[McpTool],
    policy: OutboundPolicy,
) -> Result<ModelTurnResponse, String> {
    let token = endpoint
        .api_key
        .as_deref()
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .ok_or_else(|| "Compte ChatGPT non connecté".to_owned())?;
    let target = join_base(&endpoint.base_url, "/responses");
    let (url, client) = pinned_client_for(&target, policy, LLM_REQUEST_TIMEOUT)
        .await
        .map_err(|e| format!("URL ChatGPT refusée ({e:?})"))?;
    let mut payload = json!({
        "model": endpoint.model,
        "instructions": system_prompt,
        "input": build_responses_input(entries),
        "store": false,
        "stream": true,
    });
    if !tools.is_empty() {
        payload["tools"] = Value::Array(responses_tool_definitions(tools));
        payload["tool_choice"] = json!("auto");
        payload["parallel_tool_calls"] = json!(false);
    }
    let mut req = client
        .post(url)
        .bearer_auth(token)
        .header("originator", CHATGPT_ORIGINATOR)
        .header("accept", "text/event-stream")
        .json(&payload);
    if let Some(account) = endpoint.account_id.as_deref() {
        req = req.header(CHATGPT_ACCOUNT_HEADER, account);
    }
    let resp = req.send().await.map_err(|err| {
        format!(
            "Erreur réseau ChatGPT : {}",
            truncate_error(&err.to_string())
        )
    })?;
    let status = resp.status();
    let body = resp.text().await.map_err(|err| {
        format!(
            "Réponse ChatGPT illisible : {}",
            truncate_error(&err.to_string())
        )
    })?;
    if !status.is_success() {
        return Err(format!(
            "ChatGPT HTTP {} : {}",
            status.as_u16(),
            truncate_error(&body)
        ));
    }
    parse_responses_sse(&body)
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

    fn sse(events: &[Value]) -> String {
        events
            .iter()
            .map(|event| format!("event: x\ndata: {event}\n\n"))
            .collect()
    }

    #[test]
    fn responses_input_maps_messages_tool_calls_and_results() {
        let entries = vec![
            ConversationEntry::UserText("Bonjour".into()),
            ConversationEntry::AssistantToolCalls {
                content: Some("Je regarde".into()),
                calls: vec![ToolCallRequest {
                    id: "c1".into(),
                    name: "list_jobs".into(),
                    arguments: json!({ "limit": 2 }),
                }],
            },
            ConversationEntry::ToolResult {
                call_id: "c1".into(),
                tool_name: "list_jobs".into(),
                content: "[]".into(),
                is_error: false,
            },
        ];
        let items = build_responses_input(&entries);
        let kinds: Vec<_> = items
            .iter()
            .filter_map(|item| item.get("type").and_then(Value::as_str))
            .collect();
        assert_eq!(
            kinds,
            [
                "message",
                "message",
                "function_call",
                "function_call_output"
            ]
        );
        assert_eq!(items[0]["content"][0]["type"], "input_text");
        assert_eq!(items[1]["content"][0]["type"], "output_text");
        assert_eq!(items[2]["arguments"], "{\"limit\":2}");
        assert_eq!(items[3]["call_id"], "c1");
    }

    #[test]
    fn responses_sse_yields_final_text() -> Result<(), String> {
        let body = sse(&[
            json!({ "type": "response.output_text.delta", "delta": "Bon" }),
            json!({
                "type": "response.output_item.done",
                "item": { "type": "message", "content": [{ "type": "output_text", "text": "Bonjour" }] }
            }),
            json!({ "type": "response.completed", "response": {} }),
        ]);
        match parse_responses_sse(&body)? {
            ModelTurnResponse::FinalText(text) => assert_eq!(text, "Bonjour"),
            ModelTurnResponse::ToolCalls { .. } => panic!("texte attendu"),
        }
        Ok(())
    }

    #[test]
    fn responses_sse_falls_back_to_deltas_and_extracts_function_calls() -> Result<(), String> {
        let deltas = sse(&[
            json!({ "type": "response.output_text.delta", "delta": "Salut" }),
            json!({ "type": "response.completed" }),
        ]);
        assert!(matches!(
            parse_responses_sse(&deltas)?,
            ModelTurnResponse::FinalText(text) if text == "Salut"
        ));

        let calls = sse(&[
            json!({
                "type": "response.output_item.done",
                "item": {
                    "type": "function_call",
                    "call_id": "call_9",
                    "name": "list_libraries",
                    "arguments": "{\"x\":1}"
                }
            }),
            json!({ "type": "response.completed" }),
        ]);
        match parse_responses_sse(&calls)? {
            ModelTurnResponse::ToolCalls { calls, .. } => {
                assert_eq!(calls.len(), 1);
                assert_eq!(calls[0].id, "call_9");
                assert_eq!(calls[0].arguments["x"], 1);
            }
            ModelTurnResponse::FinalText(_) => panic!("appel d'outil attendu"),
        }
        Ok(())
    }

    #[test]
    fn responses_sse_reports_failures_and_truncated_streams() {
        let failed = sse(&[json!({ "type": "response.failed", "response": { "error": "boom" } })]);
        assert!(parse_responses_sse(&failed).is_err_and(|e| e.contains("boom")));
        let cut = sse(&[json!({ "type": "response.output_text.delta", "delta": "x" })]);
        assert!(parse_responses_sse(&cut).is_err());
    }
}
